#!/bin/sh
set -eu
cd "$(dirname "$0")/.."
cargo build --release --locked
app_dir="$PWD/dist/Orbweaver.app"
mkdir -p "$app_dir/Contents/MacOS" "$app_dir/Contents/Resources"
cp target/release/orbweaver "$app_dir/Contents/MacOS/orbweaver"
cat > "$app_dir/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleName</key><string>Orbweaver</string>
<key>CFBundleDisplayName</key><string>Orbweaver</string>
<key>CFBundleIdentifier</key><string>local.orbweaver.app</string>
<key>CFBundleExecutable</key><string>orbweaver</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>CFBundleShortVersionString</key><string>0.1.0</string>
<key>CFBundleVersion</key><string>1</string>
<key>NSHighResolutionCapable</key><true/>
<key>LSMinimumSystemVersion</key><string>13.0</string>
<key>NSInputMonitoringUsageDescription</key><string>Orbweaver reads only your Razer keypad to apply your key mappings.</string>
</dict></plist>
PLIST
codesign --force --sign - "$app_dir"
printf 'Built %s\n' "$app_dir"
