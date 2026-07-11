# Compatibility status

Suture ships self-contained applications for Linux x86_64, Windows x86_64, and Apple Silicon macOS.

| Environment | Status | Evidence needed before stable release |
|---|---|---|
| Ubuntu 22.04 CI | Automated | Build, FFmpeg export test, AppImage extraction, bundled-tool checks, headless launch |
| Windows Server 2022 CI | Automated | Native build/tests, bundled sidecars and DLL closure, installer creation, launch smoke test |
| Apple Silicon macOS 14 CI | Automated | Native build/tests, bundled dylib closure, ad-hoc signature, DMG creation, launch smoke test |
| Current Ubuntu LTS | Pending manual test | Launch, file dialogs, audio and video export |
| Current Fedora stable | Pending manual test | Launch, Wayland/X11, file dialogs, optical drive |
| Current Debian stable | Pending manual test | Launch and export |
| Current Arch Linux | Pending manual test | Launch and export |
| Current openSUSE | Pending manual test | Launch and export |

Audio-CD behavior also requires physical-hardware testing with clean, scratched, mixed-mode, and CD-TEXT discs on all three operating systems. CI has no optical hardware, so parser/state tests cannot replace those checks.

Suture should not be described as stable across these distributions until the pending rows are backed by a recorded test result.
