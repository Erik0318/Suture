use std::{env, path::PathBuf, process::Command};

pub fn sidecar(name: &str) -> PathBuf {
    #[cfg(target_os = "windows")]
    let name = if name.to_ascii_lowercase().ends_with(".exe") {
        name.to_owned()
    } else {
        format!("{name}.exe")
    };
    #[cfg(not(target_os = "windows"))]
    let name = name.to_owned();

    if let Some(dir) = env::var_os("SUTURE_MEDIA_DIR") {
        return PathBuf::from(dir).join(&name);
    }

    if let Ok(exe) = env::current_exe() {
        if let Some(bin_dir) = exe.parent() {
            let same_dir = bin_dir.join(&name);
            if same_dir.is_file() {
                return same_dir;
            }
            if let Some(usr_dir) = bin_dir.parent() {
                let appimage_sidecar = usr_dir.join("lib").join("suture").join(&name);
                if appimage_sidecar.is_file() {
                    return appimage_sidecar;
                }
            }
        }
    }

    PathBuf::from(name)
}

pub fn verify_media_tools() -> Vec<String> {
    ["ffmpeg", "ffprobe"]
        .into_iter()
        .filter_map(|name| {
            let result = Command::new(sidecar(name)).arg("-version").output();
            match result {
                Ok(output) if output.status.success() => None,
                Ok(output) => Some(format!(
                    "{name} failed its startup check (exit {:?})",
                    output.status.code()
                )),
                Err(error) => Some(format!("Could not start {name}: {error}")),
            }
        })
        .collect()
}
