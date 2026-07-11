# Building Suture

## Fedora development setup

```bash
sudo dnf install rust cargo gcc gcc-c++ pkgconf-pkg-config systemd-devel \
  gtk3-devel libxkbcommon-devel libxkbcommon-x11-devel wayland-devel ffmpeg-free \
  cdparanoia libdiscid-devel curl ca-certificates
cargo run
```

If Fedora's FFmpeg build lacks a codec needed for testing, point Suture at another local development build:

```bash
SUTURE_MEDIA_DIR=/path/to/bin cargo run
```

That directory must contain `ffmpeg`, `ffprobe`, `cdparanoia`, and `curl`. Release builds ignore the host PATH because the tools live inside the AppImage.

## Checks

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
```

## Release packages

Release CI builds on Ubuntu 22.04, Windows Server 2022, and Apple Silicon macOS. It creates an AppImage, a per-user Windows installer, and a macOS DMG, each with its SHA-256 checksum. The `v1.0.0` release is replaced only after all native tests, sidecar checks, packaging checks, and launch smoke tests succeed.
