#!/usr/bin/env bash
# Builds the `print_opened` example as OpenFilesExample.app in target/example-bundle, to try the crate by hand:
#   examples/bundle_example.sh
#   open -a target/example-bundle/OpenFilesExample.app some_file.txt       (or "Open With" in Finder)
#   open bevy-open-files-example://hello/world
# The output of the app is in the Console app, or start the binary inside the bundle from a terminal.
set -euo pipefail
cd "$(dirname "$0")/.."

cargo build --example print_opened
APP=target/example-bundle/OpenFilesExample.app
mkdir -p "$APP/Contents/MacOS"
cp target/debug/examples/print_opened "$APP/Contents/MacOS/OpenFilesExample"
cat > "$APP/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>CFBundleIdentifier</key>
	<string>com.example.openfilesexample</string>
	<key>CFBundleName</key>
	<string>OpenFilesExample</string>
	<key>CFBundleExecutable</key>
	<string>OpenFilesExample</string>
	<key>CFBundlePackageType</key>
	<string>APPL</string>
	<key>CFBundleShortVersionString</key>
	<string>0.18.0</string>
	<key>LSMinimumSystemVersion</key>
	<string>10.13</string>
	<key>NSHighResolutionCapable</key>
	<true/>
	<key>CFBundleDocumentTypes</key>
	<array>
		<dict>
			<key>CFBundleTypeName</key>
			<string>Any file</string>
			<key>CFBundleTypeRole</key>
			<string>Viewer</string>
			<key>LSHandlerRank</key>
			<string>Alternate</string>
			<key>LSItemContentTypes</key>
			<array>
				<string>public.item</string>
			</array>
		</dict>
	</array>
	<key>CFBundleURLTypes</key>
	<array>
		<dict>
			<key>CFBundleURLName</key>
			<string>com.example.openfilesexample</string>
			<key>CFBundleURLSchemes</key>
			<array>
				<string>bevy-open-files-example</string>
			</array>
		</dict>
	</array>
</dict>
</plist>
PLIST
# Make macOS notice the bundle and its document types.
/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister -f "$APP"
echo "Built $APP"
