# Third-party notices

Suture is MIT-licensed and invokes media tools as separate processes. Release artifacts include additional software under its own license.

## FFmpeg and ffprobe

The release packages include FFmpeg and ffprobe from Ubuntu, MSYS2, or Homebrew. Some configurations enable GPL components, including x264, and are therefore distributed under GPL-2.0-or-later. Each platform package keeps the applicable upstream license.

- Project: <https://ffmpeg.org/>
- Source: <https://packages.ubuntu.com/jammy/ffmpeg>
- License information: <https://ffmpeg.org/legal.html>

## x264

The FFmpeg build used by release CI may include x264. x264 is GPL-2.0-or-later.

- Project and source: <https://code.videolan.org/videolan/x264>

## cdparanoia and libcdio

The Linux AppImage includes Ubuntu cdparanoia. Windows and macOS include the libcdio-paranoia port, which provides the same direct digital audio extraction and error-correction workflow across platforms. cdparanoia and libcdio-paranoia are GPL-2.0-or-later/GPL-3.0-or-later according to the packaged version; linked libraries keep their respective licenses.

- cdparanoia source package: <https://packages.ubuntu.com/source/jammy/cdparanoia>
- GNU libcdio: <https://www.gnu.org/software/libcdio/>

## MusicBrainz metadata

Suture calculates MusicBrainz-compatible disc IDs from the CD table of contents in Rust. When an audio CD is recognized, it makes one HTTPS request to the MusicBrainz web service to retrieve track names. MusicBrainz core data is CC0.

- Disc ID specification: <https://musicbrainz.org/doc/Disc_ID_Calculation>
- MusicBrainz data licensing: <https://musicbrainz.org/doc/About/Data_License>

## curl and CA certificates

Every platform package includes curl, its required runtime libraries, and a CA certificate bundle for the MusicBrainz HTTPS request.

- curl: <https://curl.se/>
- Ubuntu curl package: <https://packages.ubuntu.com/jammy/curl>
- MSYS2 curl package: <https://packages.msys2.org/packages/mingw-w64-ucrt-x86_64-curl>
- Homebrew curl formula: <https://formulae.brew.sh/formula/curl>

## Rust dependencies

Rust dependencies and their exact resolved versions are recorded in `Cargo.lock`; each dependency retains its own license.
