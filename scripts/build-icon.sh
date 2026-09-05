#!/bin/sh
set -eu
cd "$(dirname "$0")/.."
icon_dir=$(mktemp -d)/Orbweaver.iconset
mkdir -p "$icon_dir"
trap 'rm -rf "${icon_dir%/*}"' EXIT
for size in 16 32 128 256 512; do
    sips -z "$size" "$size" assets/orbweaver-icon.png --out "$icon_dir/icon_${size}x${size}.png" >/dev/null
    double=$((size * 2))
    sips -z "$double" "$double" assets/orbweaver-icon.png --out "$icon_dir/icon_${size}x${size}@2x.png" >/dev/null
done
iconutil -c icns "$icon_dir" -o assets/Orbweaver.icns
printf 'Built assets/Orbweaver.icns\n'
