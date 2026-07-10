#!/usr/bin/env bash
set -euo pipefail

version="${1:?usage: package-appimage.sh VERSION}"
linuxdeploy="${LINUXDEPLOY:-./linuxdeploy-x86_64.AppImage}"
appdir="${APPDIR:-AppDir}"

rm -rf "$appdir"
mkdir -p "$appdir/usr/lib/suture" "$appdir/usr/share/doc/suture"

APPIMAGE_EXTRACT_AND_RUN=1 "$linuxdeploy" \
  --appdir "$appdir" \
  --executable target/release/suture \
  --executable "$(command -v ffmpeg)" \
  --executable "$(command -v ffprobe)" \
  --executable "$(command -v cdparanoia)" \
  --desktop-file packaging/io.github.erik0318.Suture.desktop \
  --icon-file assets/suture.svg

for tool in ffmpeg ffprobe cdparanoia; do
  if [[ -x "$appdir/usr/bin/$tool" ]]; then
    mv "$appdir/usr/bin/$tool" "$appdir/usr/lib/suture/$tool"
  fi
done

cp LICENSE THIRD_PARTY_NOTICES.md "$appdir/usr/share/doc/suture/"
cp /usr/share/doc/ffmpeg/copyright "$appdir/usr/share/doc/suture/FFMPEG_COPYRIGHT" || true
cp /usr/share/doc/cdparanoia/copyright "$appdir/usr/share/doc/suture/CDPARANOIA_COPYRIGHT" || true

export OUTPUT="Suture-${version}-x86_64.AppImage"
APPIMAGE_EXTRACT_AND_RUN=1 "$linuxdeploy" --appdir "$appdir" --output appimage
sha256sum "$OUTPUT" > "$OUTPUT.sha256"

