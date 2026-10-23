//! Prints the files and URLs that macOS asks the app to open.
//!
//! Run `examples/bundle_example.sh` to build this as an app bundle, a bare binary is never asked to open anything.

use bevy::prelude::*;
use bevy_macos_open_files::{MacOsOpenFilesPlugin, OpenFiles, OpenUrls};

fn main() {
    let _ = App::new()
        .add_plugins((DefaultPlugins, MacOsOpenFilesPlugin))
        .add_systems(Update, print_opened)
        .run();
}

fn print_opened(mut files: MessageReader<OpenFiles>, mut urls: MessageReader<OpenUrls>) {
    for OpenFiles(paths) in files.read() {
        println!("OPEN FILES: {paths:?}");
    }
    for OpenUrls(urls) in urls.read() {
        println!("OPEN URLS: {urls:?}");
    }
}
