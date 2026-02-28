use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize)]
pub struct AppConfig {
    pub default_model: String,
    pub model_dir: PathBuf,
    pub whisper_bin: String,
    pub ffmpeg_bin: String,
    pub xdotool_bin: String,
    pub threads: usize,
    pub language: String,
    pub convert: bool,
    pub mic_socket: PathBuf,
    pub mic_source: String,
    pub mic_min_seconds: f32,
    pub timeout_secs: u64,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct FileConfig {
    default_model: Option<String>,
    model_dir: Option<PathBuf>,
    whisper_bin: Option<String>,
    ffmpeg_bin: Option<String>,
    xdotool_bin: Option<String>,
    threads: Option<usize>,
    language: Option<String>,
    convert: Option<bool>,
    #[serde(rename = "mic_hotkey")]
    _deprecated_mic_hotkey: Option<String>,
    mic_socket: Option<PathBuf>,
    mic_source: Option<String>,
    mic_min_seconds: Option<f32>,
    timeout_secs: Option<u64>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            default_model: "base.en".to_string(),
            model_dir: default_model_dir(),
            whisper_bin: "auto".to_string(),
            ffmpeg_bin: "ffmpeg".to_string(),
            xdotool_bin: "xdotool".to_string(),
            threads: 4,
            language: "en".to_string(),
            convert: true,
            mic_socket: default_mic_socket_path(),
            mic_source: "default".to_string(),
            mic_min_seconds: 0.2,
            timeout_secs: 3600,
        }
    }
}

impl AppConfig {
    fn apply_file_config(&mut self, file: FileConfig) {
        if let Some(default_model) = file.default_model {
            self.default_model = default_model;
        }
        if let Some(model_dir) = file.model_dir {
            self.model_dir = model_dir;
        }
        if let Some(whisper_bin) = file.whisper_bin {
            self.whisper_bin = whisper_bin;
        }
        if let Some(ffmpeg_bin) = file.ffmpeg_bin {
            self.ffmpeg_bin = ffmpeg_bin;
        }
        if let Some(xdotool_bin) = file.xdotool_bin {
            self.xdotool_bin = xdotool_bin;
        }
        if let Some(threads) = file.threads {
            self.threads = threads;
        }
        if let Some(language) = file.language {
            self.language = language;
        }
        if let Some(convert) = file.convert {
            self.convert = convert;
        }
        if let Some(mic_socket) = file.mic_socket {
            self.mic_socket = mic_socket;
        }
        if let Some(mic_source) = file.mic_source {
            self.mic_source = mic_source;
        }
        if let Some(mic_min_seconds) = file.mic_min_seconds {
            self.mic_min_seconds = mic_min_seconds;
        }
        if let Some(timeout_secs) = file.timeout_secs {
            self.timeout_secs = timeout_secs;
        }
    }
}

pub fn load(config_path_override: Option<&PathBuf>) -> Result<(AppConfig, PathBuf)> {
    let path = resolve_config_path(config_path_override)?;
    let mut config = AppConfig::default();

    if path.exists() {
        let raw = fs::read_to_string(&path)
            .with_context(|| format!("failed reading config at {}", path.display()))?;
        let file_cfg: FileConfig = toml::from_str(&raw)
            .with_context(|| format!("failed parsing TOML config at {}", path.display()))?;
        config.apply_file_config(file_cfg);
    }

    config.model_dir = expand_tilde(&config.model_dir);
    config.mic_socket = expand_tilde(&config.mic_socket);

    Ok((config, path))
}

pub fn init(config_path_override: Option<&PathBuf>, force: bool) -> Result<PathBuf> {
    let path = resolve_config_path(config_path_override)?;

    if path.exists() && !force {
        bail!(
            "config already exists at {} (use --force to overwrite)",
            path.display()
        );
    }

    let default = AppConfig::default();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create config directory {}", parent.display()))?;
    }

    let serialized = toml::to_string_pretty(&default)?;
    fs::write(&path, serialized)
        .with_context(|| format!("failed writing config to {}", path.display()))?;

    Ok(path)
}

pub fn resolve_config_path(config_path_override: Option<&PathBuf>) -> Result<PathBuf> {
    if let Some(path) = config_path_override {
        return Ok(expand_tilde(path));
    }

    let base = dirs::config_dir().context("unable to determine OS config directory")?;
    Ok(base.join("whisperx").join("config.toml"))
}

fn default_model_dir() -> PathBuf {
    dirs::cache_dir()
        .unwrap_or_else(|| PathBuf::from(".cache"))
        .join("whisperx")
        .join("models")
}

fn default_mic_socket_path() -> PathBuf {
    if let Some(runtime) = dirs::runtime_dir() {
        return runtime.join("whisperx").join("mic.sock");
    }

    let user = std::env::var("USER").unwrap_or_else(|_| "user".to_string());
    PathBuf::from(format!("/tmp/whisperx-{user}/mic.sock"))
}

pub fn expand_tilde(path: &Path) -> PathBuf {
    let raw = path.to_string_lossy();
    if raw == "~" {
        return dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    }

    if let Some(suffix) = raw.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(suffix);
        }
    }

    path.to_path_buf()
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use pretty_assertions::assert_eq;

    use super::{AppConfig, FileConfig, expand_tilde};

    #[test]
    fn applies_file_overrides() {
        let mut config = AppConfig::default();
        let file = FileConfig {
            default_model: Some("small.en".to_string()),
            model_dir: Some(PathBuf::from("~/models")),
            whisper_bin: Some("./bin/whisper-cli".to_string()),
            ffmpeg_bin: Some("/usr/bin/ffmpeg".to_string()),
            xdotool_bin: Some("/usr/bin/xdotool".to_string()),
            threads: Some(8),
            language: Some("auto".to_string()),
            convert: Some(false),
            _deprecated_mic_hotkey: None,
            mic_socket: Some(PathBuf::from("~/mic.sock")),
            mic_source: Some("alsa_input".to_string()),
            mic_min_seconds: Some(0.35),
            timeout_secs: Some(123),
        };

        config.apply_file_config(file);

        assert_eq!(config.default_model, "small.en");
        assert_eq!(config.model_dir, PathBuf::from("~/models"));
        assert_eq!(config.whisper_bin, "./bin/whisper-cli");
        assert_eq!(config.ffmpeg_bin, "/usr/bin/ffmpeg");
        assert_eq!(config.xdotool_bin, "/usr/bin/xdotool");
        assert_eq!(config.threads, 8);
        assert_eq!(config.language, "auto");
        assert!(!config.convert);
        assert_eq!(config.mic_socket, PathBuf::from("~/mic.sock"));
        assert_eq!(config.mic_source, "alsa_input");
        assert_eq!(config.mic_min_seconds, 0.35);
        assert_eq!(config.timeout_secs, 123);
    }

    #[test]
    fn expands_tilde_prefix() {
        let home = dirs::home_dir().expect("home dir expected in test env");
        assert_eq!(expand_tilde(&PathBuf::from("~/tmp")), home.join("tmp"));
    }
}
