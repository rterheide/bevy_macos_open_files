//! The requests that macOS made, waiting to be sent as messages. Not specific to macOS, so it is tested everywhere.

use crate::{OpenFiles, OpenUrls};
use bevy_ecs::prelude::*;
use bevy_log::info;
use std::path::PathBuf;
use std::sync::{Mutex, PoisonError};

/// One call of `application:openURLs:`: the files and the other URLs that were in it.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct Request {
    pub(crate) files: Vec<PathBuf>,
    pub(crate) urls: Vec<String>,
}

impl Request {
    pub(crate) fn is_empty(&self) -> bool {
        self.files.is_empty() && self.urls.is_empty()
    }
}

/// Requests that were made and not sent yet. One entry per request.
static PENDING: Mutex<Vec<Request>> = Mutex::new(Vec::new());

/// Queues a request, to be sent by [`deliver`] in the next frame.
#[cfg_attr(
    not(target_os = "macos"),
    allow(dead_code, reason = "only macOS makes requests")
)]
pub(crate) fn push(request: Request) {
    if request.is_empty() {
        return;
    }
    PENDING
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .push(request);
}

/// Takes all the queued requests, oldest first.
pub(crate) fn take_all() -> Vec<Request> {
    std::mem::take(&mut *PENDING.lock().unwrap_or_else(PoisonError::into_inner))
}

/// Sends the requests that were made since the last frame as messages.
pub(crate) fn deliver(mut files: MessageWriter<OpenFiles>, mut urls: MessageWriter<OpenUrls>) {
    for request in take_all() {
        if !request.files.is_empty() {
            info!("macOS asked to open {} file(s)", request.files.len());
            let _ = files.write(OpenFiles(request.files));
        }
        if !request.urls.is_empty() {
            info!("macOS asked to open {} URL(s)", request.urls.len());
            let _ = urls.write(OpenUrls(request.urls));
        }
    }
}

/// Tests share the queue, so they run one at a time.
#[cfg(test)]
pub(crate) static TEST_LOCK: Mutex<()> = Mutex::new(());

#[cfg(test)]
pub(crate) mod tests_support {
    use super::*;
    use bevy_app::{App, First};

    #[derive(Resource, Default)]
    pub(crate) struct Received {
        pub(crate) files: Vec<OpenFiles>,
        pub(crate) urls: Vec<OpenUrls>,
    }

    fn collect(
        mut files: MessageReader<OpenFiles>,
        mut urls: MessageReader<OpenUrls>,
        mut received: ResMut<Received>,
    ) {
        received.files.extend(files.read().cloned());
        received.urls.extend(urls.read().cloned());
    }

    /// An app with the messages and the delivery, and a system that collects what is sent.
    pub(crate) fn test_app() -> App {
        let mut app = App::new();
        let _ = app
            .add_message::<OpenFiles>()
            .add_message::<OpenUrls>()
            .init_resource::<Received>()
            .add_systems(First, deliver)
            .add_systems(bevy_app::Update, collect);
        app
    }
}

#[cfg(test)]
mod tests {
    use super::tests_support::{Received, test_app};
    use super::*;

    fn lock_and_clear() -> std::sync::MutexGuard<'static, ()> {
        let guard = TEST_LOCK.lock().unwrap_or_else(PoisonError::into_inner);
        let _ = take_all();
        guard
    }

    fn request(files: &[&str], urls: &[&str]) -> Request {
        Request {
            files: files.iter().map(PathBuf::from).collect(),
            urls: urls.iter().map(ToString::to_string).collect(),
        }
    }

    #[test]
    fn files_and_urls_are_sent_as_separate_messages() {
        let _guard = lock_and_clear();
        let mut app = test_app();

        push(request(&["/a.txt", "/b.txt"], &["myapp://open"]));
        app.update();

        let received = app.world().resource::<Received>();
        assert_eq!(
            received.files,
            vec![OpenFiles(vec!["/a.txt".into(), "/b.txt".into()])]
        );
        assert_eq!(received.urls, vec![OpenUrls(vec!["myapp://open".into()])]);
    }

    #[test]
    fn all_files_of_a_request_are_in_one_message_in_order() {
        let _guard = lock_and_clear();
        let mut app = test_app();

        push(request(&["/3", "/1", "/2"], &[]));
        app.update();

        let received = app.world().resource::<Received>();
        assert_eq!(received.files.len(), 1);
        assert_eq!(
            received.files[0].0,
            vec![
                PathBuf::from("/3"),
                PathBuf::from("/1"),
                PathBuf::from("/2")
            ]
        );
        assert_eq!(received.urls, vec![]);
    }

    #[test]
    fn requests_are_sent_oldest_first_and_only_once() {
        let _guard = lock_and_clear();
        let mut app = test_app();

        push(request(&["/first"], &[]));
        push(request(&["/second"], &[]));
        app.update();
        app.update();

        let received = app.world().resource::<Received>();
        assert_eq!(
            received.files,
            vec![
                OpenFiles(vec!["/first".into()]),
                OpenFiles(vec!["/second".into()])
            ]
        );
    }

    #[test]
    fn an_empty_request_sends_nothing() {
        let _guard = lock_and_clear();
        let mut app = test_app();

        push(Request::default());
        app.update();

        let received = app.world().resource::<Received>();
        assert_eq!(received.files, vec![]);
        assert_eq!(received.urls, vec![]);
    }

    #[test]
    fn requests_made_before_the_first_frame_are_sent_in_it() {
        // This is how the file that launched the app arrives: macOS asks before the app has run a frame.
        let _guard = lock_and_clear();
        push(request(&["/launched_with.txt"], &[]));
        let mut app = test_app();

        app.update();

        let received = app.world().resource::<Received>();
        assert_eq!(
            received.files,
            vec![OpenFiles(vec!["/launched_with.txt".into()])]
        );
    }
}
