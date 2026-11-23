#!/usr/bin/env bash
# Builds a release Glide.app (binary, Info.plist, AppIcon.icns) under <target>/release.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

cargo build --release
target="${CARGO_TARGET_DIR:-$root/target}"
version="$(cargo metadata --no-deps --format-version 1 |
  python3 -c 'import json,sys; print(next(p["version"] for p in json.load(sys.stdin)["packages"] if p["name"] == "glide"))')"

app="$target/release/Glide.app"
rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cp "$target/release/glide" "$app/Contents/MacOS/glide"
sed "s/@VERSION@/$version/g" assets/macos/Info.plist.in >"$app/Contents/Info.plist"

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
iconset="$work/AppIcon.iconset"
mkdir "$iconset"
src=assets/icon/glide-1024.png
for size in 16 32 128 256 512; do
  sips -z "$size" "$size" "$src" --out "$iconset/icon_${size}x${size}.png" >/dev/null
  double=$((size * 2))
  sips -z "$double" "$double" "$src" --out "$iconset/icon_${size}x${size}@2x.png" >/dev/null
done
iconutil -c icns "$iconset" -o "$app/Contents/Resources/AppIcon.icns"

echo "$app"
