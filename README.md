# bevy_macos_open_files

Receive the files and URLs that macOS asks a [Bevy](https://bevy.org) app to open, as Bevy messages.

On macOS, double clicking a document in Finder, choosing **Open With**, dropping a file on the Dock icon, or opening a link with a custom
URL scheme (`myapp://…`) does **not** start your app with command line arguments. macOS sends the request to the *running* app, through
the `application:openURLs:` method of the app delegate. winit 0.30 (which Bevy 0.18 uses) makes its own app delegate without that method,
so the request is silently dropped. This crate adds the method, and gives you messages:

| Message | When |
|---|---|
| `OpenFiles(Vec<PathBuf>)` | macOS asks to open files. All files of one request are in one message. |
| `OpenUrls(Vec<String>)` | macOS asks to open URLs that are not files (your custom scheme). Treat them as untrusted input. |

The messages are sent in the first frame after they arrive. That includes the request that launched the app, and requests that arrive while
the app is idle (the event loop is woken up for them).

```toml
[dependencies]
bevy_macos_open_files = "0.18"   # the first two numbers match the Bevy version
```

```rust
use bevy::prelude::*;
use bevy_macos_open_files::{MacOsOpenFilesPlugin, OpenFiles, OpenUrls};

fn main() {
    App::new()
        // After the DefaultPlugins: they create the event loop and the app delegate that this adds a method to.
        .add_plugins((DefaultPlugins, MacOsOpenFilesPlugin))
        .add_systems(Update, (open_files, open_urls))
        .run();
}

fn open_files(mut opened: MessageReader<OpenFiles>) {
    for OpenFiles(paths) in opened.read() {
        info!("Finder asked to open {paths:?}");
    }
}

fn open_urls(mut opened: MessageReader<OpenUrls>) {
    for OpenUrls(urls) in opened.read() {
        info!("Asked to open {urls:?}");
    }
}
```

On other platforms the plugin only registers the (never sent) messages, so you do not need `#[cfg(target_os = "macos")]` around it. There,
files arrive as command line arguments as usual.

## What your app needs: a bundle and an `Info.plist`

macOS only offers an app for a file type or a URL scheme if the app is a **bundle** (`MyApp.app`) with an `Info.plist` that says so. A bare
binary, such as the one `cargo run` starts, is never asked to open anything. You can still test the code path with `cargo run` by passing a
file as an argument yourself, but not the Finder part.

A bundle is this layout:

```
MyApp.app/
  Contents/
    Info.plist
    MacOS/MyApp            <- your binary (the name must match CFBundleExecutable)
    Resources/AppIcon.icns <- optional
```

### Info.plist

This is the minimum for an app that opens `.xyz` files and `myapp://` links. Replace the placeholders.

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<!-- Identity: a reverse-DNS id that is unique to your app, and the name of the binary in Contents/MacOS. -->
	<key>CFBundleIdentifier</key>
	<string>com.example.myapp</string>
	<key>CFBundleName</key>
	<string>MyApp</string>
	<key>CFBundleExecutable</key>
	<string>MyApp</string>
	<key>CFBundlePackageType</key>
	<string>APPL</string>
	<key>CFBundleShortVersionString</key>
	<string>1.0.0</string>

	<!-- application:openURLs: exists since macOS 10.13. -->
	<key>LSMinimumSystemVersion</key>
	<string>10.13</string>
	<key>NSHighResolutionCapable</key>
	<true/>

	<!-- Files: which documents the app can open. This is what puts the app in "Open With". -->
	<key>CFBundleDocumentTypes</key>
	<array>
		<dict>
			<key>CFBundleTypeName</key>
			<string>XYZ Document</string>
			<!-- Viewer: opens, but does not save. Editor: opens and saves. -->
			<key>CFBundleTypeRole</key>
			<string>Viewer</string>
			<!-- Alternate: shown in "Open With", but not the default app. Default: becomes the default app. -->
			<key>LSHandlerRank</key>
			<string>Alternate</string>
			<!-- Either file extensions... -->
			<key>CFBundleTypeExtensions</key>
			<array>
				<string>xyz</string>
			</array>
			<!-- ...or Uniform Type Identifiers (see below). Both can be used. Use public.item for any file. -->
			<!--
			<key>LSItemContentTypes</key>
			<array>
				<string>com.example.myapp.xyz</string>
			</array>
			-->
		</dict>
	</array>

	<!-- Only needed with LSItemContentTypes for a type that macOS does not know: defines the type. -->
	<!--
	<key>UTImportedTypeDeclarations</key>
	<array>
		<dict>
			<key>UTTypeIdentifier</key>
			<string>com.example.myapp.xyz</string>
			<key>UTTypeDescription</key>
			<string>XYZ Document</string>
			<key>UTTypeConformsTo</key>
			<array>
				<string>public.data</string>
			</array>
			<key>UTTypeTagSpecification</key>
			<dict>
				<key>public.filename-extension</key>
				<array>
					<string>xyz</string>
				</array>
			</dict>
		</dict>
	</array>
	-->

	<!-- URLs: the custom scheme. Opening myapp://anything starts the app, or wakes it up, and sends OpenUrls. -->
	<key>CFBundleURLTypes</key>
	<array>
		<dict>
			<key>CFBundleURLName</key>
			<string>com.example.myapp</string>
			<key>CFBundleURLSchemes</key>
			<array>
				<string>myapp</string>
			</array>
		</dict>
	</array>
</dict>
</plist>
```

Leave out `CFBundleDocumentTypes` if you only want URLs, and `CFBundleURLTypes` if you only want files.

After changing the `Info.plist`, macOS may need a nudge to notice it. Launching the app from its bundle once usually does it, or run
`/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister -f MyApp.app`.

### Tools that make the .app bundle for you

There are tools for creating .app bundles, check their documentation for how they take the `Info.plist` settings
(file associations and URL schemes) and which Bevy-specific things (an `assets` folder next to the binary) they copy:

- [`cargo-bundle`](https://github.com/burtonageo/cargo-bundle) – builds `.app` bundles from `Cargo.toml` metadata.
- [`cargo-packager`](https://github.com/crabnebula-dev/cargo-packager) – builds `.app` bundles and `.dmg` installers, and supports file associations in its configuration.
- [`create-dmg`](https://github.com/create-dmg/create-dmg) – makes the `.dmg` disk image from a finished `.app`.
- The Bevy cheat book has a section on macOS: <https://bevy-cheatbook.github.io/platforms/macos.html>.

A manual build is also short: build for the targets you want, combine them with `lipo -create … -output MyApp.app/Contents/MacOS/MyApp`
for a universal binary, copy `assets` next to it, and add the `Info.plist` and icon.

## Trying it

`examples/bundle_example.sh` builds `examples/print_opened.rs` as `OpenFilesExample.app` (it accepts any file, and the scheme
`bevy-open-files-example://`), then:

```sh
open -a target/example-bundle/OpenFilesExample.app some_file.txt
open bevy-open-files-example://hello/world
```

Start the binary inside the bundle from a terminal to see what it prints, or look in the Console app.

## Limitations

- **Will be deprecated with winit 0.31 release.** This adds a method to winit's private delegate class `WinitApplicationDelegate`, as it is in winit 0.30. If the class is not found, a warning is logged and the app works as before.
  winit 0.31 lets an app register its own app delegate, which is the cleaner way, and this crate would then change to that.
- **Needs a bundle,** see above, this only works on MacOs .app bundles.
- **Paths are as macOS gives them.** macOS may hand a file name over in decomposed unicode (`é` as `e` plus a combining accent). It is the
  same file and opens fine, but compare paths as text only after normalizing them yourself.
- **Only `application:openURLs:`.** Files and URLs are both delivered through it on macOS 10.13 and later. The older
  `application:openFile:` and `application:openFiles:` methods are not implemented.
- It adds `unsafe` code (objc runtime calls), in `src/macos.rs` only.

## License

MIT
