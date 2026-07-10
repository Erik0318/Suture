# Suture

Suture stitches an ordered set of tracks into one continuous audio file or a static-cover video. It is a lightweight, local-first desktop app built for albums, live recordings, mixes, and audio CDs.

> Suture is currently in early development. There is no official AppImage yet.

## Download

When the first stable build is ready, download the x86_64 AppImage from [GitHub Releases](https://github.com/Erik0318/Suture/releases). Release builds will include a SHA-256 checksum and will not require a separate FFmpeg installation.

## Planned Linux MVP

- Add individual audio files, a folder, or drag files into the window
- Detect an inserted audio CD automatically and import its tracks without browsing to a device path
- Detect formats with ffprobe and order tracks from filenames and metadata
- Reorder or remove tracks manually
- Detect or select album artwork
- Export continuous audio in lossless or lossy formats
- Export MKV or MP4 video with a static cover and chapter markers
- Show honest percentage progress, speed, elapsed time, and ETA
- Use Suture's track-thread progress view during scanning, CD import, and export
- Cancel safely without leaving a fake-complete output file
- Keep media local

## CD support

On Linux, Suture will monitor optical-drive events through udev, verify the audio-CD table of contents, and rip through a bundled cdparanoia/libcdio-based sidecar. The user will not need to install abcde, cdparanoia, FFmpeg, or ffprobe.

Suture will never rip or eject a disc automatically. If several drives are connected, the user can choose one. CD-TEXT will be read locally when available, with numbered track names as the offline fallback.

## Technology

- Rust stable
- egui + eframe
- Bundled FFmpeg and ffprobe
- Bundled audio-CD reader
- AppImage for the first Linux release
- GitHub Actions for tests and reproducible release builds

The complete implementation requirements live in [docs/PROJECT_SPEC.md](docs/PROJECT_SPEC.md).

## Target platforms

The first artifact will be `Suture-<version>-x86_64.AppImage`. Windows, macOS, Flatpak, DEB, RPM, Snap, and ARM64 packaging can follow after the Linux MVP is stable.

## License

The Suture source code is available under the [MIT License](LICENSE). Bundled media tools retain their respective licenses and will be documented in the release notices.
