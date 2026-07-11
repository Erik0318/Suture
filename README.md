# Suture

Suture stitches ordered tracks into one continuous audio file or a static-cover video. It is a local-first native desktop app for albums, mixes, live recordings, and audio CDs.

## Download

Download `Suture1.0.0.AppImage` and its SHA-256 checksum from [GitHub Releases](https://github.com/Erik0318/Suture/releases).

```bash
chmod +x Suture1.0.0.AppImage
./Suture1.0.0.AppImage
```

The AppImage bundles FFmpeg, ffprobe, and the audio-CD reader.

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
- Linux optical-drive discovery through libudev
- Audio-CD table-of-contents reading and sector-based cdparanoia import progress
- CD-only export of verified, separate PCM WAV tracks into a chosen folder
- Optional UTF-8 timestamp lists using cumulative track starts and titles
- Safe temporary filenames and final-name replacement only after validation
- Settings persistence and readable warning/error surfaces

## CD workflow

Suture notices optical drives automatically. When an audio disc is inserted, it reads the table of contents and shows the track count and duration. It calculates the disc ID with libdiscid and performs one MusicBrainz lookup for real track names; unknown discs and offline sessions retain numbered fallback names. **Import CD** adds the tracks to the same reorder/export workflow as local audio. **Export separate WAV tracks** appears only for a recognized audio CD and saves one verified track per file into a chosen folder without overwriting existing files.

The shipped application uses a bundled cdparanoia sidecar controlled directly from Rust. It does not depend on abcde or a system FFmpeg installation.

## Development

Suture uses Rust stable, egui/eframe, FFmpeg/ffprobe sidecars, libudev, and cdparanoia. Fedora setup, test commands, and packaging details are in [docs/BUILDING.md](docs/BUILDING.md). The architecture is documented in [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md), and verified platform status is tracked in [docs/COMPATIBILITY.md](docs/COMPATIBILITY.md).

```bash
cargo run
```

The full acceptance specification is in [docs/PROJECT_SPEC.md](docs/PROJECT_SPEC.md).

## Release status

CI checks formatting, strict Clippy, tests, bundled media tools, and a headless AppImage launch before publishing `Suture1.0.0.AppImage`.

## License

Suture is available under the [MIT License](LICENSE). Bundled media tools keep their own licenses; see [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).
