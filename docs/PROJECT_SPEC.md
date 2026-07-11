You are building a production-quality lightweight desktop application named “Suture”.

The application turns an ordered set of audio files into either:

1. One continuous audio file
2. One continuous video containing a static album cover and the combined audio

The first release target is Linux x86_64 as a directly downloadable AppImage. Development may happen on Fedora, but release builds must be produced in a reproducible CI environment and tested on multiple mainstream Linux distributions.

Do not create a toy prototype. Build a clean, maintainable MVP that can later support Windows, macOS, Flatpak, DEB, RPM, Snap, and ARM64.

────────────────────────────────────
1. REQUIRED TECHNOLOGY
────────────────────────────────────

Use:

- Rust stable
- egui + eframe for the native GUI
- rfd or an equivalent native Rust file-dialog library
- serde and serde_json for data models and ffprobe JSON
- std::process::Command or tokio::process::Command for child processes
- FFmpeg and ffprobe as bundled external executables
- linuxdeploy and/or appimagetool for AppImage packaging
- GitHub Actions for reproducible release builds

Do not use:

- Electron
- Python
- Java
- a browser server
- shell scripts as the main application logic
- shell command strings passed through `sh -c`
- any runtime dependency that the end user must install manually

The GUI must be native, minimal, responsive, and usable on both GNOME and KDE.

All FFmpeg and ffprobe invocations must use structured argument arrays through Rust’s process API. Never concatenate user paths into a shell command.

────────────────────────────────────
2. CORE USER WORKFLOW
────────────────────────────────────

The main window should have three clear areas:

A. Input
B. Track order
C. Export

The user must be able to:

- Select a folder
- Select one or more individual audio files
- Import tracks directly from an inserted audio CD without first choosing a local folder
- Add more files after the first selection
- Remove individual tracks
- Clear the entire project
- Select an album-cover image manually
- Replace or remove the selected cover
- Choose an output location
- Export one continuous audio file
- Export one continuous video as MKV or MP4
- Optionally write a UTF-8 timestamp list using cumulative track starts and titles
- Export separate verified WAV tracks to a chosen folder when an audio CD is active
- Cancel an active export
- View export progress and a readable error message
- Open the exported file’s containing folder

A video export must be disabled when no usable cover image is available.

Audio export must never require a cover.

Do not upload any file to the internet. Everything must happen locally.

────────────────────────────────────
2A. AUDIO CD DISCOVERY AND IMPORT
────────────────────────────────────

The Linux application must detect attached optical drives and inserted audio CDs automatically. A user with an audio CD should not need to browse to a mount point or understand Linux device paths.

Do not use abcde as the production runtime. It is a shell orchestrator that expects several separately installed tools and distro-specific configuration. Use Rust to orchestrate the workflow and bundle a dedicated, auditable CD-reading sidecar such as cdparanoia/libcdio. All child-process calls must use structured argument arrays and must never pass paths or device names through a shell.

On Linux:

- Enumerate optical drives with libudev through a Rust udev binding
- Monitor udev media-inserted, media-changed, drive-added, drive-removed, and eject events without blocking the GUI thread
- Prefer udev properties such as ID_CDROM and ID_CDROM_MEDIA_AUDIO
- Use sysfs only as a documented fallback
- Confirm that media is an audio CD by reading its table of contents rather than trusting a device label
- Use a bundled cdparanoia-compatible reader or a reproducibly built libcdio-paranoia sidecar for secure digital audio extraction
- Do not require abcde, cdparanoia, FFmpeg, or libcdio to be preinstalled by the user

When one audio CD is detected:

- Show a non-blocking “Audio CD detected” card in the Input area
- Show the drive name, track count, and total duration as soon as the table of contents is available
- Provide one clear “Import CD” action
- Never begin ripping automatically
- Never eject the disc automatically

When multiple optical drives contain audio CDs, show a small drive chooser. When a disc is removed during import, stop safely, preserve a useful diagnostic, remove incomplete temporary tracks, and tell the user to reinsert the disc.

Calculate a MusicBrainz disc ID from the verified table of contents and make one HTTPS metadata request for track names. Show the lookup state without blocking the GUI. If the disc is unknown or the request is unavailable, fall back to “Track 01”, “Track 02”, and so on without inventing metadata. Never upload audio or local file paths.

Import the CD into a private temporary working directory as sequential 44.1 kHz, 16-bit stereo PCM WAV tracks. Write separate tracks into a user folder only after the user invokes the CD-only action and chooses that folder. Refuse to overwrite existing track files. Once imported, probe the temporary tracks and place them in the normal track list so ordering, removal, cover selection, and all export modes work exactly like local files.

CD track order must initially follow the disc table of contents. Preserve pregap information where it is available. Detect data-only and mixed-mode discs and never attempt to decode data tracks as audio. If the drive or disc reports read errors, retry through the paranoia reader, show the affected track, and surface a warning rather than silently producing damaged output.

Provide readable errors for:

- No optical drive found
- Empty drive
- Data disc instead of audio CD
- Drive permission denied
- Disc removed during import
- Unreadable or damaged sector
- Unsupported drive or sidecar failure

Keep all detection and ripping work off the GUI thread. Cancellation must stop the reader cleanly, then force-kill it only after a short timeout. Clean incomplete temporary tracks after cancellation, failure, disc removal, and normal exit when safe.

────────────────────────────────────
3. AUDIO FILE DISCOVERY
────────────────────────────────────

When a folder is selected:

- Scan only the selected folder by default
- Ignore hidden files
- Do not scan subfolders unless the user enables an “Include subfolders” option
- Use ffprobe rather than trusting only the file extension
- Accept any audio file that the bundled FFmpeg build can decode
- Skip unsupported or corrupted files and show them in a warning panel
- Do not silently fail

Expected common formats include:

- FLAC
- MP3
- WAV
- AIFF/AIF
- M4A
- AAC
- ALAC
- OGG Vorbis
- Opus
- WMA
- APE
- WavPack
- TTA

The application should store, for every track:

- Original absolute path
- Display filename
- Metadata title
- Metadata artist
- Album
- Track number
- Disc number
- Codec
- Container
- Duration
- Sample rate
- Channel count
- Channel layout
- Bit depth when available
- Bitrate when available
- Whether the source is lossless or lossy
- Whether ffprobe reported an error

Run probing work off the GUI thread. The interface must remain responsive while files are being scanned.

Limit concurrent ffprobe processes to a reasonable number, such as four.

────────────────────────────────────
4. AUTOMATIC TRACK ORDER
────────────────────────────────────

Automatically sort tracks using the following priority:

1. Leading number in the filename
2. Embedded disc-number and track-number metadata
3. Natural filename ordering
4. Original selection order as the final tie-breaker

Recognize numeric filename prefixes such as:

- 1 Song.flac
- 01 Song.flac
- 01 - Song.flac
- 01. Song.flac
- 01_Song.flac
- 1-01 Song.flac
- 1.01 Song.flac

Use natural number sorting, so:

1, 2, 3, 10

must not become:

1, 10, 2, 3

For filenames that clearly contain disc and track numbers, sort by disc first and track second.

If only some files have numeric prefixes, show a small non-blocking notice saying that partial numbering was detected.

The user must be able to manually change the order with:

- Drag-and-drop rows
- Move-up button
- Move-down button
- Move-to-top button
- Move-to-bottom button
- Keyboard shortcuts when a row is selected

Include a “Restore automatic order” action.

The order shown in the UI must always be the exact export order.

────────────────────────────────────
5. TRACK LIST UI
────────────────────────────────────

Each row should show:

- Drag handle
- Final track position
- Track title or filename
- Duration
- Codec
- Sample rate
- Lossless/lossy indicator
- Remove button

Do not overload the main table with every technical property.

Show complete technical information in a collapsible details area or tooltip.

Support multi-selection so several tracks can be removed or moved together.

Show total track count and total combined duration above or below the list.

────────────────────────────────────
6. COVER DISCOVERY
────────────────────────────────────

When a folder is selected, automatically look for a cover.

Priority:

1. cover.*
2. folder.*
3. front.*
4. album.*
5. Embedded artwork in the first audio track
6. The largest decodable image in the selected folder

Filename matching must be case-insensitive.

Expected image formats include:

- JPG
- JPEG
- PNG
- WebP
- BMP
- TIFF
- GIF
- Other formats FFmpeg can decode

Do not assume that a `.pic` extension describes a specific image codec. Probe the actual file contents.

For animated formats, use the first frame unless animated-cover support is added later.

Show a cover preview while preserving the image aspect ratio.

Manual cover selection must always override automatic detection.

────────────────────────────────────
7. EXPORT MODES
────────────────────────────────────

The Export panel must begin with two large choices:

- Audio
- Video

A. AUDIO EXPORT

Initial supported output choices:

Lossless:
- FLAC
- WAV PCM
- ALAC in M4A
- Matroska Audio/MKA with FLAC

Lossy:
- MP3
- AAC in M4A
- Opus in OGG
- Opus in MKA

B. VIDEO EXPORT

Supported containers:

- MKV
- MP4

Recommended combinations:

MKV lossless:
- H.264 video
- FLAC audio

MKV lossy:
- H.264 video
- Opus, AAC, or MP3 audio

MP4 lossless:
- H.264 video
- ALAC audio

MP4 lossy:
- H.264 video
- AAC audio

Do not offer incompatible codec/container combinations.

The codec-selection UI should update automatically when the container changes.

────────────────────────────────────
8. HONEST AUDIO QUALITY MODES
────────────────────────────────────

Clearly separate these concepts in the UI:

1. Copy original audio streams
2. Encode to a lossless codec
3. Encode to a lossy codec

Use these user-facing descriptions:

“Original stream copy”
Copies compressed audio without re-encoding. Available only when every selected track has compatible codec and stream parameters.

“Lossless encoding”
Decodes the sources and stores the result in FLAC, ALAC, or PCM without additional lossy compression.

“Lossy encoding”
Encodes to MP3, AAC, or Opus at the selected quality.

Never claim that converting MP3, AAC, Opus, or another lossy source to FLAC restores lost quality.

If lossy input is exported to FLAC, display:

“This prevents further lossy compression, but it cannot restore information already removed from the source.”

Do not normalize volume, apply ReplayGain, resample, alter pitch, or add fades by default.

────────────────────────────────────
9. ORIGINAL STREAM COPY
────────────────────────────────────

Enable stream-copy mode only after checking that all tracks have compatible:

- Audio codec
- Sample rate
- Channel count
- Channel layout
- Relevant codec parameters
- Target-container support

If compatibility is uncertain, disable stream copy and explain why.

Do not attempt unsafe concatenation merely because all files share the same extension.

For compatible tracks, use FFmpeg’s concat demuxer and `-c:a copy`.

For incompatible or mixed inputs, decode and encode to the selected output codec.

────────────────────────────────────
10. AUDIO CONCATENATION
────────────────────────────────────

Tracks must play continuously in the exact displayed order.

Do not add silence between tracks.

Preserve gapless transitions as closely as FFmpeg and the formats permit.

Avoid clipping audio.

When mixed sample rates or channel layouts require conversion:

- Display the chosen output sample rate and layout
- Default to preserving the common source format when all tracks match
- For mixed music sources, use 44.1 kHz stereo as the default unless the user explicitly selects another value
- Never downmix multichannel audio silently
- Require confirmation before changing channel count

Calculate total expected duration from ffprobe before starting.

After export, probe the result and compare its duration against the sum of the input durations using a reasonable tolerance.

────────────────────────────────────
11. STATIC-COVER VIDEO
────────────────────────────────────

The video must contain only the album cover.

Default video behavior:

- Preserve image aspect ratio
- Center the image
- Use a black background for unused space
- Do not stretch the image
- Scale down images larger than the selected maximum
- Do not upscale small covers by default
- Ensure even output dimensions
- Use yuv420p for compatibility
- Use H.264
- Use a still-image-oriented encoder configuration
- Default to 2 fps
- Offer 1, 2, and 5 fps
- Insert regular keyframes so seeking does not produce a long black screen
- Default maximum canvas size: 1920×1080
- Offer square 1080×1080 and “Use cover dimensions” modes

Cover modes:

- Fit: show the whole image with padding
- Fill: crop to fill the canvas
- Original: use the original image dimensions where possible

Default to Fit.

For H.264, use settings similar in intent to:

- libx264
- tune stillimage
- slow or medium preset
- CRF around 28–32
- forced keyframes at short regular intervals
- yuv420p

Do not optimize file size by creating a video that common players cannot seek through properly.

Audio must remain the full length of the combined tracks. The image stream must loop until the audio ends.

────────────────────────────────────
12. CHAPTERS AND TRACK INFORMATION
────────────────────────────────────

Generate chapter markers using the individual track boundaries.

Chapter titles should use:

1. Metadata title
2. Filename without extension
3. “Track N” as fallback

Embed chapters when the selected container supports them.

For plain FLAC or WAV exports, offer an optional accompanying CUE sheet.

For MKV/MKA exports, embed Matroska chapters.

For MP4/M4A exports, add compatible chapter metadata when technically reliable.

Chapter-generation failure must not destroy an otherwise valid export. Report it as a warning.

────────────────────────────────────
13. FILE-PATH SAFETY
────────────────────────────────────

Support:

- Spaces
- Apostrophes
- Parentheses
- Unicode
- Chinese
- Cyrillic
- Emoji where supported by the filesystem

Never pass paths through shell interpolation.

FFmpeg concat files can be fragile with unusual filenames. Create a private temporary working directory containing safely generated sequential filenames or symlinks such as:

000001.flac
000002.flac
000003.mp3

Generate the concat manifest from these controlled temporary paths.

Clean temporary data after:

- Successful completion
- Failure
- User cancellation
- Normal application exit when safe

Do not delete or modify the user’s original files.

────────────────────────────────────
14. EXPORT PROGRESS
────────────────────────────────────

Every operation that can take noticeable time must expose progress, including folder scanning, ffprobe work, cover decoding, CD table-of-contents reading, CD import, export preparation, encoding, and final validation. Never leave the user looking at a frozen window or an unexplained spinner.

Percentages must represent real measurable work:

- Folder scan: discovered entries processed when the total is known
- Probing: completed files divided by files queued
- CD import: audio sectors read divided by the total sectors in the selected tracks
- Export: encoded timestamp divided by expected total duration
- Validation: completed validation checks divided by checks scheduled

For the brief period before a total can be known, show an indeterminate motion indicator plus the exact current step, then switch to a percentage immediately after the table of contents, file list, or duration becomes available. Do not fabricate a steadily increasing percentage.

Use FFmpeg progress output, preferably:

-progress pipe:1

Parse:

- out_time
- speed
- progress
- total_size when available

Display:

- Percentage
- Current encoded time
- Total duration
- Encoding speed
- Elapsed time
- Estimated remaining time
- Current operation
- Cancel button

Make the waiting state feel specific to Suture while keeping the minimal visual design. Use a thin animated “thread” that travels through small track nodes. Completed nodes become solid, the active node pulses subtly, and pending nodes remain outlined. During CD import, each node represents a disc track; during export, it represents a track boundary in the combined timeline.

Beside the thread, show a large exact percentage and one factual live status line, for example:

- “Reading track 4 of 11 — 18:42 remaining”
- “Probing 7 of 12 files”
- “Stitching 31:18 of 48:09 at 3.2×”
- “Retrying a difficult sector on track 6”
- “Checking chapters and duration”

Also show per-track progress during CD import, overall progress, elapsed time, estimated remaining time once stable, and the current read/encode speed. Update smoothly without repainting the entire UI unnecessarily. Respect reduced-motion accessibility settings; in reduced-motion mode, replace pulsing and traveling motion with simple state changes. Do not use fake waveforms, random quotes, distracting confetti, or meaningless animations.

Cancellation must terminate FFmpeg cleanly, then force-kill it only if it does not exit after a short timeout.

A cancelled export must not leave a seemingly valid partial output file. Use a temporary output filename and rename it only after successful validation.

────────────────────────────────────
15. ERROR HANDLING
────────────────────────────────────

Provide readable errors for:

- No audio selected
- Missing cover during video export
- Unsupported input
- Corrupted file
- FFmpeg failure
- ffprobe failure
- Permission denied
- Output already exists
- Insufficient disk space
- Invalid output path
- Codec unavailable
- Incompatible stream-copy request
- User cancellation

Include a collapsible technical log with:

- Executable version
- FFmpeg version
- Sanitized command arguments
- FFmpeg stderr
- Exit code

Do not expose an enormous raw log as the primary error message.

Never silently overwrite an output file. Ask the user whether to replace it or choose another name.

────────────────────────────────────
16. SETTINGS
────────────────────────────────────

Persist a small settings file in the normal per-user configuration directory.

Remember:

- Last input folder
- Last output folder
- Last export mode
- Last selected container
- Last selected codecs
- Video canvas mode
- FPS
- Cover fit mode
- Theme
- Whether subfolder scanning is enabled

Do not store the full recent-file history unless the user enables it.

Support system theme, light theme, and dark theme.

────────────────────────────────────
17. VISUAL DESIGN
────────────────────────────────────

The application must look minimal rather than unfinished.

Design requirements:

- Single main window
- No permanent sidebar
- No gradients
- No oversized branding
- Limited use of icons
- Clear spacing
- Rounded controls only where useful
- Neutral colors
- System theme support
- Strong focus states for keyboard users
- Minimum usable window size around 760×560
- Responsive layout for larger windows

Suggested layout:

Top:
- Application name
- Select Folder
- Add Files
- Add Cover

Center:
- Cover preview on the left
- Reorderable track list on the right

Bottom:
- Audio/Video selector
- Container and codec options
- Output path
- Export button
- Progress area

The cover preview may collapse on narrow windows.

Use real icons or simple text labels. Do not use random emoji as interface icons.

────────────────────────────────────
18. APPLICATION ARCHITECTURE
────────────────────────────────────

Separate the project into modules such as:

src/
  main.rs
  app.rs
  model/
    project.rs
    track.rs
    export.rs
    settings.rs
  media/
    cd_detect.rs
    cd_toc.rs
    cd_rip.rs
    ffmpeg.rs
    ffprobe.rs
    scan.rs
    sort.rs
    cover.rs
    concat.rs
    chapters.rs
    validation.rs
  ui/
    input_panel.rs
    track_list.rs
    cover_panel.rs
    export_panel.rs
    progress_panel.rs
    dialogs.rs
  platform/
    paths.rs
    process.rs
    open_folder.rs

Use a state-machine-like export model:

Idle
DetectingDisc
ReadingDiscToc
RippingDisc
Scanning
Ready
Preparing
Exporting
Validating
Completed
Failed
Cancelled

Do not allow two exports at the same time.

Keep FFmpeg command construction separate from process execution so it can be unit-tested.

────────────────────────────────────
19. BUNDLED FFMPEG
────────────────────────────────────

Bundle ffmpeg and ffprobe inside the AppImage.

Do not download them on first launch.

Do not depend on `/usr/bin/ffmpeg`.

During development, optionally allow a clearly marked environment variable to override the bundled binaries.

At startup:

- Resolve the internal sidecar path
- Run `ffmpeg -version`
- Run `ffprobe -version`
- Verify that expected encoders and muxers are available
- Disable unavailable export options instead of crashing

Strip unnecessary symbols from release binaries when permitted.

Document:

- Exact FFmpeg version
- Exact build configuration
- Enabled external libraries
- Licenses
- Source location
- Build scripts
- Checksums

Avoid FFmpeg’s nonfree configuration.

Because H.264 through libx264 may change the applicable FFmpeg license, do not treat licensing as an afterthought. Add:

- LICENSE
- THIRD_PARTY_NOTICES.md
- FFmpeg license text
- x264 license text when used
- Reproducible FFmpeg build configuration
- Any source-distribution information required by the selected licenses

Keep the Rust application under the repository’s MIT license. Bundled FFmpeg, x264, cdparanoia/libcdio, and other sidecars retain their own licenses. Prefer invoking GPL tools as separate executables rather than linking GPL libraries into the MIT application binary. The AppImage distribution must still include all required license texts, notices, corresponding-source information or offers, build recipes, and exact component versions.

Do not claim legal compliance without documenting exactly what is distributed.

────────────────────────────────────
20. APPIMAGE PACKAGING
────────────────────────────────────

First supported artifact:

Suture1.0.0.AppImage

The AppImage must include:

- Main application binary
- ffmpeg
- ffprobe
- Desktop file
- Application icon
- Required shared libraries that should be bundled
- License files
- Third-party notices

The AppImage must:

- Launch by double-clicking after executable permission is enabled
- Launch from a terminal
- Work when stored in a path containing spaces
- Find its bundled FFmpeg tools regardless of the current directory
- Not write configuration inside the mounted AppImage
- Store settings in the user config directory
- Store temporary export files in the appropriate temporary directory

Build on an old-enough supported Linux baseline rather than the developer’s newest Fedora installation.

Use a reproducible container or GitHub Actions runner. Begin with an Ubuntu 22.04-compatible build baseline unless dependency constraints require a documented change.

Do not claim support for every Linux distribution.

Publish a tested compatibility list.

Initial test targets:

- Current Fedora stable
- Current Ubuntu LTS
- Current Debian stable
- Current Arch Linux
- Current openSUSE release

Build x86_64 first.

Prepare the project structure for an ARM64 AppImage later.

Generate a SHA-256 checksum for every release artifact.

────────────────────────────────────
21. TESTING
────────────────────────────────────

Write unit tests for:

- Numeric-prefix parsing
- Disc/track parsing
- Natural sorting
- Partial-numbering behavior
- Codec/container compatibility
- Stream-copy eligibility
- FFmpeg argument generation
- Chapter timestamp calculation
- Output filename generation
- Settings serialization
- Audio-CD table-of-contents parsing
- Sector-based CD import progress calculation
- Mixed-mode disc filtering
- Drive insertion/removal state transitions

Write integration tests that generate small test media with FFmpeg.

Test cases must include:

1. Three matching FLAC tracks
2. Three MP3 tracks
3. Mixed FLAC, WAV, and MP3
4. Different sample rates
5. Different channel counts
6. Unicode paths
7. Apostrophes and spaces
8. Numeric names: 1, 2, 10
9. Disc and track names: 1-01, 1-02, 2-01
10. One corrupted input
11. No cover
12. JPG cover
13. PNG cover with alpha
14. Very large cover
15. Odd image dimensions
16. MKV with FLAC
17. MKV with Opus
18. MP4 with AAC
19. MP4 with ALAC
20. User cancellation
21. Audio CD with several tracks
22. Mixed-mode disc with audio and data tracks
23. Disc removal during import
24. CD import cancellation and temporary-file cleanup

Validate outputs using ffprobe.

Tests must verify:

- Output exists
- Output duration approximately equals the sum of inputs
- Selected audio codec is present
- Video exports contain exactly one video stream and one audio stream
- Static image persists through the full video duration
- Chapter count and timestamps are correct where supported
- Original files are unchanged
- Failed exports do not leave final-named partial files

────────────────────────────────────
22. CI AND RELEASES
────────────────────────────────────

Create GitHub Actions workflows for:

- cargo fmt
- cargo clippy
- cargo test
- Linux release build
- AppImage packaging
- Artifact checksum generation

The release workflow should run on version tags such as:

v0.1.0

A release should include:

- AppImage
- SHA-256 checksum
- Changelog
- License files
- Third-party notices
- Basic run instructions

Do not automatically publish a release until tests pass.

────────────────────────────────────
23. DEVELOPMENT PHASES
────────────────────────────────────

Implement in phases.

Phase 1:
- Repository setup
- Basic egui window
- File/folder selection
- Optical-drive enumeration and audio-CD detection
- Audio-CD table-of-contents display
- ffprobe detection
- Track list
- Automatic sorting
- Manual reordering
- Cover preview

Phase 2:
- Audio-CD import with cancellation and sector-based progress
- Audio export to FLAC and WAV
- MKV video with H.264 and FLAC
- Progress and cancellation
- Output validation

Phase 3:
- MP3, AAC, Opus, and ALAC options
- MP4 export
- Stream-copy mode
- Chapter generation
- CUE-sheet generation

Phase 4:
- AppImage packaging
- Bundled FFmpeg
- Release CI
- Cross-distribution testing

Phase 5:
- UI refinement
- Accessibility
- Performance improvements
- Documentation
- Windows/macOS preparation

After every phase:

- Run formatting
- Run clippy with warnings treated seriously
- Run tests
- Summarize what was implemented
- List known limitations
- Provide exact commands to build and run
- Commit changes with a meaningful commit message if repository access permits

Do not move to a later phase while the current phase is broken.

────────────────────────────────────
24. MVP ACCEPTANCE CRITERIA
────────────────────────────────────

The Linux MVP is complete only when a user can:

1. Download one AppImage
2. Mark it executable
3. Launch it without installing FFmpeg
4. Select a folder containing audio files or import an inserted audio CD without browsing to a device path
5. See the files automatically detected and ordered
6. Reorder tracks manually
7. Select or auto-detect a cover
8. Export one FLAC file without requiring a cover
9. Export one MKV video with the cover visible for the full duration
10. Choose lossless or lossy audio where implemented
11. Cancel an export
12. Receive a clear error when something fails
13. Play the result in mpv and VLC
14. Seek throughout the video without the cover disappearing
15. Confirm with ffprobe that the expected codecs were used
16. See accurate percentage progress and the Suture track-thread visualization during long operations
17. Cancel a CD import without leaving incomplete temporary tracks

Start by creating:

- A concise architecture document
- A repository tree
- Cargo.toml
- The Phase 1 implementation
- Build and run instructions for Fedora

Do not respond only with an implementation plan. Create the actual project files and run the available tests.
