#!/usr/bin/env bash
# Builds a release Glide.app (binary, Info.plist, AppIcon.icns) under <target>/release.
# The bundle keeps the linker's ad-hoc signature; it is not signed or notarized.
set -euo pipefail

for tool in cargo python3 sips iconutil; do
  command -v "$tool" >/dev/null || {
    echo "bundle-macos.sh: '$tool' is required (macOS with Xcode command line tools)" >&2
    exit 1
  }
done
if [[ -n "${CARGO_BUILD_TARGET:-}" ]]; then
  echo "bundle-macos.sh: unset CARGO_BUILD_TARGET; the bundle uses the host target" >&2
  exit 1
fi

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

cargo build --release
{
  IFS= read -r target
  IFS= read -r version
} < <(cargo metadata --no-deps --format-version 1 |
  python3 -c 'import json,sys; m=json.load(sys.stdin); print(m["target_directory"]); print(next(p["version"] for p in m["packages"] if p["name"] == "glide"))')

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
staged="$work/Glide.app"
mkdir -p "$staged/Contents/MacOS" "$staged/Contents/Resources"
cp "$target/release/glide" "$staged/Contents/MacOS/glide"
sed "s/@VERSION@/$version/g" assets/macos/Info.plist.in >"$staged/Contents/Info.plist"

iconset="$work/AppIcon.iconset"
mkdir "$iconset"
src=assets/icon/glide-1024.png
for size in 16 32 128 256 512; do
  sips -z "$size" "$size" "$src" --out "$iconset/icon_${size}x${size}.png" >/dev/null
  double=$((size * 2))
  sips -z "$double" "$double" "$src" --out "$iconset/icon_${size}x${size}@2x.png" >/dev/null
done
iconutil -c icns "$iconset" -o "$staged/Contents/Resources/AppIcon.icns"

app="$target/release/Glide.app"
rm -rf "$app"
mv "$staged" "$app"
echo "$app"
