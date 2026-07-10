# Compatibility status

Suture's first target is a Linux x86_64 AppImage built on Ubuntu 22.04.

| Environment | Status | Evidence needed before stable release |
|---|---|---|
| Ubuntu 22.04 CI | Automated | Build, FFmpeg export test, AppImage extraction, bundled-tool checks, headless launch |
| Current Ubuntu LTS | Pending manual test | Launch, file dialogs, audio and video export |
| Current Fedora stable | Pending manual test | Launch, Wayland/X11, file dialogs, optical drive |
| Current Debian stable | Pending manual test | Launch and export |
| Current Arch Linux | Pending manual test | Launch and export |
| Current openSUSE | Pending manual test | Launch and export |

Audio-CD behavior also requires at least two physical optical drives and clean, scratched, mixed-mode, and CD-TEXT discs. CI has no optical hardware, so parser/state tests cannot replace those checks.

Suture should not be described as stable across these distributions until the pending rows are backed by a recorded test result.

