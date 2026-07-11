#!/usr/bin/env bash
set -euo pipefail

version="${1:?usage: package-macos.sh VERSION}"
arch="$(uname -m)"
root="$(cd "$(dirname "$0")/.." && pwd)"
dist="$root/dist-macos"
app="$dist/Suture.app"
macos="$app/Contents/MacOS"
libs="$app/Contents/libs"
resources="$app/Contents/Resources"

rm -rf "$dist"
mkdir -p "$macos" "$libs" "$resources"
cp "$root/target/release/suture" "$macos/Suture"
cp "$root/packaging/macos/Info.plist" "$app/Contents/Info.plist"
cp "$root/LICENSE" "$root/THIRD_PARTY_NOTICES.md" "$resources/"

cp "$(command -v ffmpeg)" "$macos/ffmpeg"
cp "$(command -v ffprobe)" "$macos/ffprobe"
cp "$(command -v cd-paranoia)" "$macos/cd-paranoia"
cp "$(brew --prefix curl)/bin/curl" "$macos/curl"
cp "$(brew --prefix ca-certificates)/share/ca-certificates/cacert.pem" "$macos/ca-certificates.crt"

for executable in "$macos"/*; do
  if file "$executable" | grep -q 'Mach-O'; then
    dylibbundler -od -b -x "$executable" -d "$libs" -p '@executable_path/../libs/'
  fi
done

if find "$macos" "$libs" -type f -exec otool -L {} \; 2>/dev/null \
    | grep -E '/opt/homebrew|/usr/local/(Cellar|opt)'; then
  echo "A Homebrew-only library path remains in the macOS application" >&2
  exit 1
fi

chmod +x "$macos"/*
codesign --force --deep --sign - "$app"
plutil -lint "$app/Contents/Info.plist"

output="$root/Suture${version}-macOS-${arch}.dmg"
hdiutil create -volname "Suture ${version}" -srcfolder "$app" -ov -format UDZO "$output"
shasum -a 256 "$output" > "$output.sha256"
