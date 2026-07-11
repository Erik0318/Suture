# Suture

Suture stitches ordered tracks into one continuous audio file or a static-cover video. It is a local-first native desktop app for albums, mixes, live recordings, and audio CDs.

## Download

Download the package for your computer from [GitHub Releases](https://github.com/Erik0318/Suture/releases):

- Linux x86_64: `Suture1.0.0.AppImage`
- Windows x86_64: `Suture1.0.0-Windows-x86_64-Setup.exe`
- macOS Apple Silicon: `Suture1.0.0-macOS-arm64.dmg`

Every package includes its media and audio-CD tools; users do not need to install FFmpeg, curl, libcdio-paranoia, or abcde.

```bash
chmod +x Suture1.0.0.AppImage
./Suture1.0.0.AppImage
```

Windows and macOS may show a first-launch security confirmation because those new packages are not yet signed with commercial platform certificates.

## Implemented on `main`

- Add a folder or individual audio files
- Probe media off the UI thread with at most four ffprobe workers
- Natural filename, disc/track metadata, and manual ordering
- Cover discovery, embedded-art extraction, validation, preview, replacement, and removal
- FLAC, WAV, ALAC, MP3, AAC, Opus, MKA, MKV, and MP4 choices
- Compatible video codec/container choices only
- Static-cover H.264 video with Fit, Fill, or Original sizing
- Chapter metadata and optional CUE sheets
- Real FFmpeg percentage, speed, elapsed time, ETA, cancellation, and output validation
- Suture's track-thread waiting visualization with reduced-motion behavior
- Native Linux, Windows, and macOS optical-drive discovery
- Cross-platform audio-CD table-of-contents reading and sector-based libcdio-paranoia import progress
- CD-only export of verified, separate PCM WAV tracks into a chosen folder
- Optional UTF-8 timestamp lists using cumulative track starts and titles
- Safe temporary filenames and final-name replacement only after validation
- Settings persistence and readable warning/error surfaces

## CD workflow

Suture notices optical drives automatically through libudev on Linux, Windows optical-drive information, or macOS `drutil`. When an audio disc is inserted, it reads the table of contents and shows the track count and duration. It calculates the MusicBrainz disc ID directly and performs a TOC-aware lookup for real track names; unknown discs and offline sessions retain numbered fallback names. **Import CD** adds the tracks to the same reorder/export workflow as local audio. **Export separate WAV tracks** appears only for a recognized audio CD and saves one verified track per file into a chosen folder without overwriting existing files.

The shipped applications use a bundled cdparanoia/libcdio-paranoia sidecar controlled directly from Rust. They do not depend on abcde or a system FFmpeg installation.

## Development

Suture uses Rust stable, egui/eframe, FFmpeg/ffprobe sidecars, platform optical-drive discovery, and libcdio-paranoia. Build and packaging details are in [docs/BUILDING.md](docs/BUILDING.md). The architecture is documented in [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md), and verified platform status is tracked in [docs/COMPATIBILITY.md](docs/COMPATIBILITY.md).

```bash
cargo run
```

The full acceptance specification is in [docs/PROJECT_SPEC.md](docs/PROJECT_SPEC.md).

## Release status

CI checks formatting, strict Clippy, tests, bundled media tools, and native launch smoke tests before publishing the Linux, Windows, and macOS packages together.

## License

Suture is available under the [MIT License](LICENSE). Bundled media tools keep their own licenses; see [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).
