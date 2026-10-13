#!/bin/sh
set -eu
cd "$(dirname "$0")/.."
cargo build --release --locked
app_dir="$PWD/dist/Orbweaver.app"
mkdir -p "$app_dir/Contents/MacOS" "$app_dir/Contents/Resources"
cp target/release/orbweaver "$app_dir/Contents/MacOS/orbweaver"
cp assets/Orbweaver.icns "$app_dir/Contents/Resources/Orbweaver.icns"
cat > "$app_dir/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleName</key><string>Orbweaver</string>
<key>CFBundleDisplayName</key><string>Orbweaver</string>
<key>CFBundleIdentifier</key><string>local.orbweaver.app</string>
<key>CFBundleExecutable</key><string>orbweaver</string>
<key>CFBundleIconFile</key><string>Orbweaver</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>CFBundleShortVersionString</key><string>0.1.0</string>
<key>CFBundleVersion</key><string>1</string>
<key>NSHighResolutionCapable</key><true/>
<key>LSMinimumSystemVersion</key><string>13.0</string>
<key>NSInputMonitoringUsageDescription</key><string>Orbweaver reads only your Razer keypad to apply your key mappings.</string>
</dict></plist>
PLIST
identity="${ORBWEAVER_SIGN_IDENTITY:-Orbweaver Local Signing}"
if security find-identity -v -p codesigning | grep -q "\"$identity\""; then
    # A stable local identity keeps macOS privacy grants valid across rebuilds.
    codesign --force --sign "$identity" --identifier local.orbweaver.app "$app_dir"
    printf 'Signed with "%s".\n' "$identity"
else
    codesign --force --sign - "$app_dir"
    printf 'Signed ad hoc; run scripts/make-signing-identity.sh once to keep privacy grants across rebuilds.\n'
fi
printf 'Built %s\n' "$app_dir"
