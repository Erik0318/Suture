# Third-party notices

Suture is MIT-licensed and invokes media tools as separate processes. Release artifacts include additional software under its own license.

## FFmpeg and ffprobe

The Linux AppImage is assembled from the Ubuntu 22.04 FFmpeg package. That build enables GPL components, including x264. FFmpeg is distributed under GPL-2.0-or-later for this configuration. The release bundle includes the package copyright file, and the release notes must identify the exact Ubuntu package version and source-package URL.

- Project: <https://ffmpeg.org/>
- Source: <https://packages.ubuntu.com/jammy/ffmpeg>
- License information: <https://ffmpeg.org/legal.html>

## x264

The FFmpeg build used by release CI may include x264. x264 is GPL-2.0-or-later.

- Project and source: <https://code.videolan.org/videolan/x264>

## cdparanoia and libcdio

The Linux AppImage includes the Ubuntu 22.04 cdparanoia executable and its discovered runtime libraries for audio-CD extraction. Their copyright files are copied into the release bundle. cdparanoia is GPL-2.0-or-later; linked library licenses are documented by their respective Ubuntu source packages.

- cdparanoia source package: <https://packages.ubuntu.com/source/jammy/cdparanoia>
- GNU libcdio: <https://www.gnu.org/software/libcdio/>

## Rust dependencies

Rust crate licenses are recorded by `cargo-about` during release preparation. A final release must include the generated dependency report. The repository must not publish a release if that report is missing.

