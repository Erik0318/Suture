# Building Suture

## Fedora development setup

```bash
sudo dnf install rust cargo gcc gcc-c++ pkgconf-pkg-config systemd-devel \
  gtk3-devel libxkbcommon-devel libxkbcommon-x11-devel wayland-devel ffmpeg-free cdparanoia
cargo run
```

If Fedora's FFmpeg build lacks a codec needed for testing, point Suture at another local development build:

```bash
SUTURE_MEDIA_DIR=/path/to/bin cargo run
```

That directory must contain `ffmpeg`, `ffprobe`, and `cdparanoia`. Release builds ignore the host PATH because the tools live inside the AppImage.

## Checks

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
```

## AppImage

Release CI builds on Ubuntu 22.04, stages the Rust executable and media sidecars with linuxdeploy, and creates `Suture-<version>-x86_64.AppImage` plus its SHA-256 checksum. A version tag such as `v0.1.0` triggers the workflow after the CI job succeeds.
