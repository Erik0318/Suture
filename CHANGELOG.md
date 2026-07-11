# Changelog

All notable changes to Suture are documented here.

## 0.1.0-alpha.2 — 2026-07-10

- Fixed the AppImage FFmpeg/ffprobe runtime library path that blocked file and folder imports
- Added visible audio-CD detection and table-of-contents read feedback with elapsed time
- Made the cover target react to drag hover and accept images by content, not only extension
- Added a responsive single-column layout and reduced window constraints for stable edge resizing
- Capped repeated probe errors and strengthened isolated AppImage sidecar validation in CI

## 0.1.0-alpha.1 — 2026-07-10

- Native Rust/egui Linux application shell
- Local file and folder discovery with bounded ffprobe concurrency
- Automatic and manual track ordering
- Cover discovery, preview, and embedded-art extraction
- Continuous audio and static-cover video export through FFmpeg
- Chapters, optional CUE sheets, cancellation, validation, and safe replacement
- Real percentage, speed, ETA, and track-thread progress visualization
- Linux optical-drive detection and audio-CD import through cdparanoia
- GitHub Actions checks and AppImage packaging

Hardware optical-drive testing and launch testing outside Ubuntu CI remain release blockers.
