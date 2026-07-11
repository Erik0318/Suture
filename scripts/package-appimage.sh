#!/usr/bin/env bash
set -euo pipefail

version="${1:?usage: package-appimage.sh VERSION}"
linuxdeploy="${LINUXDEPLOY:-./linuxdeploy-x86_64.AppImage}"
appdir="${APPDIR:-AppDir}"

rm -rf "$appdir"
mkdir -p "$appdir/usr/share/doc/suture" "$appdir/usr/share/suture"
xkbcommon_x11="$(ldconfig -p | awk '/libxkbcommon-x11.so.0/{print $NF; exit}')"
if [[ -z "$xkbcommon_x11" ]]; then
  echo "libxkbcommon-x11.so.0 is required for X11 support" >&2
  exit 1
fi

APPIMAGE_EXTRACT_AND_RUN=1 "$linuxdeploy" \
  --appdir "$appdir" \
  --executable target/release/suture \
  --executable "$(command -v ffmpeg)" \
  --executable "$(command -v ffprobe)" \
  --executable "$(command -v cdparanoia)" \
  --executable "$(command -v curl)" \
  --library "$xkbcommon_x11" \
  --desktop-file packaging/io.github.erik0318.Suture.desktop \
  --icon-file assets/io.github.erik0318.Suture.svg

bundled_xkbcommon_x11="$(find "$appdir/usr/lib" -maxdepth 1 -name 'libxkbcommon-x11.so.0*' -print -quit)"
if [[ -z "$bundled_xkbcommon_x11" ]]; then
  echo "linuxdeploy did not bundle libxkbcommon-x11" >&2
  exit 1
fi
ln -sfn "$(basename "$bundled_xkbcommon_x11")" "$appdir/usr/lib/libxkbcommon-x11.so"

cp LICENSE THIRD_PARTY_NOTICES.md "$appdir/usr/share/doc/suture/"
cp /etc/ssl/certs/ca-certificates.crt "$appdir/usr/share/suture/"
cp /usr/share/doc/ffmpeg/copyright "$appdir/usr/share/doc/suture/FFMPEG_COPYRIGHT" || true
cp /usr/share/doc/cdparanoia/copyright "$appdir/usr/share/doc/suture/CDPARANOIA_COPYRIGHT" || true
cp /usr/share/doc/libdiscid0/copyright "$appdir/usr/share/doc/suture/LIBDISCID_COPYRIGHT" || true
cp /usr/share/doc/curl/copyright "$appdir/usr/share/doc/suture/CURL_COPYRIGHT" || true

export OUTPUT="Suture${version}.AppImage"
APPIMAGE_EXTRACT_AND_RUN=1 "$linuxdeploy" --appdir "$appdir" --output appimage
sha256sum "$OUTPUT" > "$OUTPUT.sha256"
