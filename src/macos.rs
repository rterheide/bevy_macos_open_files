//! The macOS part: adds `application:openURLs:` to winit's app delegate class.

#![allow(unsafe_code)]

use crate::queue::{self, Request};
use bevy_app::App;
use bevy_log::{info, warn};
use bevy_winit::{EventLoopProxy, EventLoopProxyWrapper, WinitUserEvent};
use objc2::ffi::class_addMethod;
use objc2::runtime::{AnyClass, AnyObject, Imp, Sel};
use objc2::{MainThreadMarker, sel};
use objc2_app_kit::NSApplication;
use objc2_foundation::{NSArray, NSURL};
use std::ffi::CStr;
use std::path::PathBuf;
use std::sync::OnceLock;

/// The class winit registers as the application delegate (`winit-0.30`, `platform_impl/macos/app_state.rs`).
const WINIT_DELEGATE_CLASS: &CStr = c"WinitApplicationDelegate";
/// Type encoding of `-application:openURLs:`: no return value, then self, the selector, and two objects.
const OPEN_URLS_TYPE_ENCODING: &CStr = c"v@:@@";

/// Wakes the event loop, which sleeps until an event arrives, so a request that comes in while the app is idle is handled right away.
static WAKE_PROXY: OnceLock<EventLoopProxy<WinitUserEvent>> = OnceLock::new();

pub(crate) fn install(app: &App) {
    if let Some(proxy) = app.world().get_resource::<EventLoopProxyWrapper>() {
        let _ = WAKE_PROXY.set(EventLoopProxy::clone(proxy));
    } else {
        warn!(
            "No winit event loop found, add MacOsOpenFilesPlugin after the DefaultPlugins. Requests will be handled late."
        );
    }
    install_open_urls_method();
}

/// Adds the method to winit's application delegate class, and sets the delegate again so `NSApplication` notices.
fn install_open_urls_method() {
    let Some(main_thread) = MainThreadMarker::new() else {
        warn!("Not on the main thread, cannot receive files from macOS.");
        return;
    };
    let Some(delegate_class) = AnyClass::get(WINIT_DELEGATE_CLASS) else {
        warn!(
            "The winit application delegate class was not found, cannot receive files from macOS."
        );
        return;
    };
    if !add_open_urls_method(delegate_class) {
        warn!("The application delegate already has application:openURLs:, not replacing it.");
        return;
    }

    // NSApplication looks up which delegate methods exist when the delegate is set, so set winit's delegate again.
    let application = NSApplication::sharedApplication(main_thread);
    let delegate = application.delegate();
    application.setDelegate(delegate.as_deref());
    info!("Receiving files and URLs from macOS.");
}

/// Adds `application:openURLs:` to `class`. Returns false if the class already has the method.
fn add_open_urls_method(class: &AnyClass) -> bool {
    let handler: extern "C-unwind" fn(*mut AnyObject, Sel, *mut AnyObject, *mut NSArray<NSURL>) =
        application_open_urls;
    // SAFETY: `Imp` is an untyped function pointer. The method is called with the arguments that `OPEN_URLS_TYPE_ENCODING` describes,
    // which match the signature of `application_open_urls`. `class` is a valid class.
    unsafe {
        let handler: Imp = std::mem::transmute(handler);
        class_addMethod(
            std::ptr::from_ref(class).cast_mut(),
            sel!(application:openURLs:),
            handler,
            OPEN_URLS_TYPE_ENCODING.as_ptr(),
        )
        .as_bool()
    }
}

/// Splits the URLs of a request in files, and URLs that are not files.
fn request_from_urls(urls: &NSArray<NSURL>) -> Request {
    let mut request = Request::default();
    for url in urls {
        if url.isFileURL() {
            if let Some(path) = url.path() {
                request.files.push(PathBuf::from(path.to_string()));
            }
        } else if let Some(url) = url.absoluteString() {
            request.urls.push(url.to_string());
        }
    }
    request
}

/// `-[WinitApplicationDelegate application:openURLs:]`, called by `AppKit` with the files and URLs to open.
extern "C-unwind" fn application_open_urls(
    _this: *mut AnyObject,
    _selector: Sel,
    _application: *mut AnyObject,
    urls: *mut NSArray<NSURL>,
) {
    // SAFETY: AppKit passes an `NSArray` of `NSURL` that stays valid for the duration of the call.
    let Some(urls) = (unsafe { urls.as_ref() }) else {
        return;
    };
    let request = request_from_urls(urls);
    if request.is_empty() {
        return;
    }
    queue::push(request);
    if let Some(proxy) = WAKE_PROXY.get() {
        let _ = proxy.send_event(WinitUserEvent::WakeUp);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::queue::{TEST_LOCK, take_all};
    use crate::{OpenFiles, OpenUrls};
    use objc2::rc::Retained;
    use objc2::runtime::{ClassBuilder, NSObject};
    use objc2::{ClassType, msg_send};
    use objc2_foundation::NSString;
    use std::sync::PoisonError;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn file_url(path: &str) -> Retained<NSURL> {
        NSURL::fileURLWithPath(&NSString::from_str(path))
    }

    fn url(url: &str) -> Retained<NSURL> {
        NSURL::URLWithString(&NSString::from_str(url)).expect("a valid url")
    }

    /// A class like winit's delegate, without `application:openURLs:`. Every test needs its own, a class name can only be registered once.
    fn fake_delegate_class() -> &'static AnyClass {
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        let name = std::ffi::CString::new(format!(
            "TestWinitApplicationDelegate{}",
            COUNTER.fetch_add(1, Ordering::SeqCst)
        ))
        .expect("no nul in the name");
        ClassBuilder::new(&name, NSObject::class())
            .expect("the class name is not used yet")
            .register()
    }

    /// Calls `application:openURLs:` on `delegate`, as `AppKit` does.
    fn call_open_urls(delegate: &AnyObject, urls: &NSArray<NSURL>) {
        let application: *const AnyObject = std::ptr::null();
        let () = unsafe { msg_send![delegate, application: application, openURLs: urls] };
    }

    fn new_instance(class: &AnyClass) -> Retained<AnyObject> {
        unsafe { msg_send![class, new] }
    }

    #[test]
    fn file_urls_become_paths_and_other_urls_stay_urls() {
        let urls = NSArray::from_retained_slice(&[
            file_url("/tmp/model.gcode"),
            url("myapp://open?layer=3"),
            file_url("/tmp/second.gcode"),
            url("https://example.com/a?b=c"),
        ]);

        let request = request_from_urls(&urls);

        assert_eq!(
            request.files,
            vec![
                PathBuf::from("/tmp/model.gcode"),
                PathBuf::from("/tmp/second.gcode")
            ]
        );
        assert_eq!(
            request.urls,
            vec!["myapp://open?layer=3", "https://example.com/a?b=c"]
        );
    }

    #[test]
    fn a_path_with_spaces_and_accents_still_opens_the_file() {
        // macOS gives the path in decomposed unicode ("e" and a combining accent instead of "é"), so the text differs from what was
        // put in, but it must still be the same file.
        let dir =
            std::env::temp_dir().join(format!("bevy_macos_open_files_test_{}", std::process::id()));
        std::fs::create_dir_all(dir.join("my folder")).expect("make the test folder");
        let file = dir.join("my folder").join("modèl één.gcode");
        std::fs::write(&file, "G1 X1").expect("write the test file");
        let urls = NSArray::from_retained_slice(&[file_url(file.to_str().expect("utf8 path"))]);

        let request = request_from_urls(&urls);

        assert_eq!(request.files.len(), 1);
        assert_eq!(
            std::fs::read_to_string(&request.files[0]).expect("the path opens the file"),
            "G1 X1"
        );
        assert!(request.files[0].starts_with(dir.join("my folder")));
        std::fs::remove_dir_all(&dir).expect("remove the test folder");
    }

    #[test]
    fn a_percent_encoded_file_url_is_decoded() {
        let urls = NSArray::from_retained_slice(&[url("file:///tmp/a%20b.gcode")]);

        let request = request_from_urls(&urls);

        assert_eq!(request.files, vec![PathBuf::from("/tmp/a b.gcode")]);
    }

    #[test]
    fn the_method_is_added_once_and_not_replaced() {
        let class = fake_delegate_class();

        assert!(add_open_urls_method(class));
        assert!(!add_open_urls_method(class));
    }

    #[test]
    fn a_delegate_with_the_method_receives_what_appkit_sends() {
        let _guard = TEST_LOCK.lock().unwrap_or_else(PoisonError::into_inner);
        let _ = take_all();
        let class = fake_delegate_class();
        assert!(add_open_urls_method(class));
        let delegate = new_instance(class);

        call_open_urls(
            &delegate,
            &NSArray::from_retained_slice(&[file_url("/tmp/a.gcode"), url("myapp://x")]),
        );

        assert_eq!(
            take_all(),
            vec![Request {
                files: vec![PathBuf::from("/tmp/a.gcode")],
                urls: vec!["myapp://x".to_string()],
            }]
        );
    }

    #[test]
    fn a_delegate_without_the_method_does_not_respond_to_it() {
        // What winit's own delegate does today, and why nothing arrives without this crate.
        let class = fake_delegate_class();
        let delegate = new_instance(class);
        let responds: bool =
            unsafe { msg_send![&*delegate, respondsToSelector: sel!(application:openURLs:)] };
        assert!(!responds);

        assert!(add_open_urls_method(class));
        let responds: bool =
            unsafe { msg_send![&*delegate, respondsToSelector: sel!(application:openURLs:)] };
        assert!(responds);
    }

    #[test]
    fn an_empty_array_queues_nothing() {
        let _guard = TEST_LOCK.lock().unwrap_or_else(PoisonError::into_inner);
        let _ = take_all();
        let class = fake_delegate_class();
        assert!(add_open_urls_method(class));
        let delegate = new_instance(class);

        call_open_urls(&delegate, &NSArray::from_retained_slice(&[]));

        assert_eq!(take_all(), vec![]);
    }

    #[test]
    fn what_appkit_sends_arrives_as_messages_in_the_next_frame() {
        // The whole path except winit and a window: AppKit calls the method, the request is queued, the next frame sends messages.
        let _guard = TEST_LOCK.lock().unwrap_or_else(PoisonError::into_inner);
        let _ = take_all();
        let class = fake_delegate_class();
        assert!(add_open_urls_method(class));
        let delegate = new_instance(class);
        let mut app = crate::queue::tests_support::test_app();

        call_open_urls(
            &delegate,
            &NSArray::from_retained_slice(&[
                file_url("/tmp/one.gcode"),
                file_url("/tmp/two.gcode"),
                url("myapp://open?file=/tmp/one.gcode"),
            ]),
        );
        app.update();

        let received = app
            .world()
            .resource::<crate::queue::tests_support::Received>();
        assert_eq!(
            received.files,
            vec![OpenFiles(vec![
                "/tmp/one.gcode".into(),
                "/tmp/two.gcode".into()
            ])]
        );
        assert_eq!(
            received.urls,
            vec![OpenUrls(vec!["myapp://open?file=/tmp/one.gcode".into()])]
        );
    }
}
