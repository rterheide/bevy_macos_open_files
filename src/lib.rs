//! Receive the files and URLs that macOS asks a Bevy app to open, as Bevy messages.
//!
//! When a user double clicks a document in Finder, chooses "Open With", drops a file on the app's Dock icon, or opens a link with a custom
//! URL scheme, macOS does not start the app with command line arguments. It sends the request to the running app, through the
//! `application:openURLs:` method of the app delegate. winit 0.30, which Bevy 0.18 uses, makes its own app delegate without that method, so
//! the request is silently dropped. This crate adds the method to winit's delegate, and turns the requests into [`OpenFiles`] and
//! [`OpenUrls`] messages.
//!
//! ```no_run
//! use bevy::prelude::*;
//! use bevy_macos_open_files::{MacOsOpenFilesPlugin, OpenFiles};
//!
//! fn main() {
//!     App::new()
//!         // After the `DefaultPlugins`: they create the event loop and the delegate that this adds to.
//!         .add_plugins((DefaultPlugins, MacOsOpenFilesPlugin))
//!         .add_systems(Update, load_opened_files)
//!         .run();
//! }
//!
//! fn load_opened_files(mut opened: MessageReader<OpenFiles>) {
//!     for OpenFiles(paths) in opened.read() {
//!         println!("Finder asked to open {paths:?}");
//!     }
//! }
//! ```
//!
//! The app must be launched as an app bundle (`MyApp.app`) with the right `Info.plist` for macOS to offer it for files or URLs,
//! see the README. On other platforms the plugin does nothing, so the same code compiles everywhere without `cfg`s.
//!
//! This relies on the name of winit's private delegate class, `WinitApplicationDelegate`, as of winit 0.30. When Bevy uses a winit that
//! lets the app register its own delegate (winit 0.31), this approach is replaced. If the class is not found, a warning is logged and
//! the app works as before, without these messages.

use bevy_app::{App, First, Plugin};
use bevy_ecs::prelude::*;
use std::path::PathBuf;

#[cfg(target_os = "macos")]
mod macos;
mod queue;

/// The files that macOS asked the app to open. All the files of one request are in one message, in the order macOS gave them.
///
/// Sent in the first frame after they arrive, also when they are what launched the app.
#[derive(Message, Debug, Clone, PartialEq, Eq)]
pub struct OpenFiles(pub Vec<PathBuf>);

/// The URLs that are not files, that macOS asked the app to open. This is how an app with a custom URL scheme (`myapp://…`) receives
/// its links. The URLs are not checked, treat them as untrusted input.
#[derive(Message, Debug, Clone, PartialEq, Eq)]
pub struct OpenUrls(pub Vec<String>);

/// Adds the [`OpenFiles`] and [`OpenUrls`] messages, and on macOS the app delegate method that receives them.
///
/// Add it after the `DefaultPlugins` (or at least the `WinitPlugin`). On other platforms it only registers the messages.
#[derive(Debug, Default)]
pub struct MacOsOpenFilesPlugin;

impl Plugin for MacOsOpenFilesPlugin {
    fn build(&self, app: &mut App) {
        let _ = app
            .add_message::<OpenFiles>()
            .add_message::<OpenUrls>()
            .add_systems(First, queue::deliver);
        #[cfg(target_os = "macos")]
        macos::install(app);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy_ecs::message::Messages;

    #[test]
    fn the_plugin_registers_both_messages_without_a_window() {
        // The plugin delivers from the shared queue, which other tests fill.
        let _guard = crate::queue::TEST_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        // No winit here: on macOS the plugin warns that it found no event loop, and the messages still exist.
        let mut app = App::new();
        let _ = app.add_plugins(MacOsOpenFilesPlugin);
        app.update();

        assert!(app.world().contains_resource::<Messages<OpenFiles>>());
        assert!(app.world().contains_resource::<Messages<OpenUrls>>());
    }
}
