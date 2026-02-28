use std::path::{Path, PathBuf};

use crate::config::AppConfig;

pub fn resolve_whisper_bin(config: &AppConfig) -> PathBuf {
    if config.whisper_bin.eq_ignore_ascii_case("auto") {
        if let Some(path) = bundled_whisper_cli_path() {
            return path;
        }
        return PathBuf::from("whisper-cli");
    }

    if config.whisper_bin == "whisper-cli"
        && let Some(path) = bundled_whisper_cli_path()
    {
        return path;
    }

    PathBuf::from(&config.whisper_bin)
}

pub fn bundled_whisper_cli_path() -> Option<PathBuf> {
    let current_exe = std::env::current_exe().ok()?;
    let parent = current_exe.parent()?;
    let candidate = parent.join("whisper-cli");
    if candidate.is_file() {
        return Some(candidate);
    }
    None
}

pub fn binary_available(path: &Path) -> bool {
    if path.components().count() > 1 || path.is_absolute() {
        path.is_file()
    } else {
        which::which(path).is_ok()
    }
}
