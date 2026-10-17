#!/usr/bin/env bash
# Bundle a universal advplay binary and the sample package into LastCall.app.
# Usage: bundle-macos-app.sh <advplay-binary> <sample.advpak> <dest-dir>
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
binary="${1:?advplay binary}"
sample="${2:?sample.advpak}"
dest="${3:?destination directory}"
app="$dest/LastCall.app"
rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cp "$root/packaging/Info.plist" "$app/Contents/Info.plist"
cp "$binary" "$app/Contents/MacOS/advplay"
cp "$sample" "$app/Contents/Resources/sample.advpak"
cp "$root/packaging/README-macos.txt" "$dest/README.txt"
chmod +x "$app/Contents/MacOS/advplay"
echo "bundled $app"
