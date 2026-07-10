# Suture architecture

Suture keeps user interface state, media inspection, export construction, process execution, and platform integration separate. Long-running work runs on worker threads and reports typed events to the egui application.

## Runtime flow

1. `app` accepts local paths, dropped files, or an optical-drive event.
2. `scan` discovers files and limits concurrent `ffprobe` work to four workers.
3. `probe` converts ffprobe JSON into the shared `Track` model.
4. `sort` establishes automatic order; the UI owns all subsequent manual order changes.
5. `cover` validates manual and discovered images and extracts embedded artwork when necessary.
6. `export` creates a private workspace with controlled filenames, constructs FFmpeg arguments without a shell, parses `-progress pipe:1`, validates the result, and only then moves it to the final name.
7. `cd` enumerates Linux optical drives through libudev, verifies the disc table of contents, and imports audio sectors through the bundled cdparanoia sidecar.

## State and cancellation

Only one scan, CD import, or export runs at a time. A shared atomic cancellation token lets the UI request cancellation without blocking. Workers terminate the child process, remove partial outputs, and report a final typed event.

## Trust boundaries

- User paths are never interpolated into shell commands.
- FFmpeg and cdparanoia receive structured argument arrays.
- Concat manifests reference sequential filenames inside a private temporary directory.
- Existing output files remain untouched until a replacement export has passed duration validation.
- Release tools are resolved next to the application inside the AppImage. `SUTURE_MEDIA_DIR` is a development-only override.

