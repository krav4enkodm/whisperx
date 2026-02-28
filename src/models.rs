use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use indicatif::{ProgressBar, ProgressStyle};
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::cache;
use crate::config::AppConfig;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelRecord {
    pub name: String,
    pub file: String,
    pub url: String,
    pub sha256: Option<String>,
    pub size: Option<u64>,
    pub description: Option<String>,
}

pub fn list(config: &AppConfig) -> Result<()> {
    let registry = load_registry()?;

    println!(
        "{:<12} {:<10} {:<40} Description",
        "Model", "Installed", "File"
    );
    for model in registry {
        let path = model_path(config, &model);
        let installed = if path.exists() { "yes" } else { "no" };
        let description = model.description.unwrap_or_default();
        println!(
            "{:<12} {:<10} {:<40} {}",
            model.name, installed, model.file, description
        );
    }

    Ok(())
}

pub fn ensure_model(config: &AppConfig, name: &str) -> Result<PathBuf> {
    let registry = load_registry()?;
    let model = registry
        .iter()
        .find(|m| m.name == name)
        .ok_or_else(|| anyhow::anyhow!("unknown model '{name}'"))?
        .clone();

    cache::ensure_dir(&config.model_dir)?;
    let destination = model_path(config, &model);

    if destination.exists() {
        if let Some(expected) = model.sha256.as_deref() {
            let actual = sha256_file(&destination)?;
            if !hashes_match(&actual, expected) {
                eprintln!(
                    "warning: checksum mismatch for existing model '{}', re-downloading",
                    destination.display()
                );
                fs::remove_file(&destination).with_context(|| {
                    format!("failed to remove stale model at {}", destination.display())
                })?;
            } else {
                return Ok(destination);
            }
        } else {
            return Ok(destination);
        }
    }

    download_model(&model, &destination)?;
    Ok(destination)
}

fn load_registry() -> Result<Vec<ModelRecord>> {
    let models: Vec<ModelRecord> = serde_json::from_str(include_str!("../assets/models.json"))
        .context("failed to parse assets/models.json")?;

    if models.is_empty() {
        bail!("model registry is empty");
    }

    Ok(models)
}

fn model_path(config: &AppConfig, model: &ModelRecord) -> PathBuf {
    config.model_dir.join(&model.file)
}

fn download_model(model: &ModelRecord, destination: &Path) -> Result<()> {
    let client = Client::new();
    let mut response = client
        .get(&model.url)
        .send()
        .with_context(|| format!("failed downloading model from {}", model.url))?;

    if !response.status().is_success() {
        bail!(
            "model download failed with status {} from {}",
            response.status(),
            model.url
        );
    }

    let total_size = response.content_length().or(model.size).unwrap_or(0);

    let progress = if total_size > 0 {
        let pb = ProgressBar::new(total_size);
        pb.set_style(
            ProgressStyle::with_template("{bar:40.cyan/blue} {bytes}/{total_bytes} {msg}")
                .unwrap_or_else(|_| ProgressStyle::default_bar()),
        );
        pb.set_message(format!("downloading {}", model.name));
        pb
    } else {
        ProgressBar::hidden()
    };

    let temp_path = destination.with_extension("download");
    let mut file = File::create(&temp_path)
        .with_context(|| format!("failed creating temporary file {}", temp_path.display()))?;

    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 8192];

    loop {
        let bytes_read = response
            .read(&mut buffer)
            .context("failed while reading model download stream")?;
        if bytes_read == 0 {
            break;
        }

        file.write_all(&buffer[..bytes_read])
            .context("failed while writing model download")?;
        hasher.update(&buffer[..bytes_read]);
        progress.inc(bytes_read as u64);
    }

    progress.finish_and_clear();

    let actual_sha = hex::encode(hasher.finalize());
    if let Some(expected_sha) = model.sha256.as_deref() {
        if !hashes_match(&actual_sha, expected_sha) {
            let _ = fs::remove_file(&temp_path);
            bail!(
                "checksum mismatch for model '{}': expected {}, got {}",
                model.name,
                expected_sha,
                actual_sha
            );
        }
    }

    fs::rename(&temp_path, destination).with_context(|| {
        format!(
            "failed moving downloaded model from {} to {}",
            temp_path.display(),
            destination.display()
        )
    })?;

    Ok(())
}

fn hashes_match(actual: &str, expected: &str) -> bool {
    actual.trim().eq_ignore_ascii_case(expected.trim())
}

fn sha256_file(path: &Path) -> Result<String> {
    let mut file = File::open(path)
        .with_context(|| format!("failed opening model file for checksum: {}", path.display()))?;

    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 8192];
    loop {
        let bytes_read = file
            .read(&mut buffer)
            .with_context(|| format!("failed reading {}", path.display()))?;
        if bytes_read == 0 {
            break;
        }
        hasher.update(&buffer[..bytes_read]);
    }

    Ok(hex::encode(hasher.finalize()))
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::{ModelRecord, load_registry};

    #[test]
    fn parses_registry() {
        let models = load_registry().expect("registry should parse");
        assert!(!models.is_empty());
        assert!(models.iter().any(|m| m.name == "base.en"));
    }

    #[test]
    fn parses_valid_model_record_json() {
        let record: ModelRecord = serde_json::from_str(
            r#"{
                "name": "base.en",
                "file": "ggml-base.en.bin",
                "url": "https://example.test/model.bin",
                "sha256": "abc123",
                "size": 42,
                "description": "Test"
            }"#,
        )
        .expect("record should parse");

        assert_eq!(record.name, "base.en");
        assert_eq!(record.file, "ggml-base.en.bin");
        assert_eq!(record.sha256.as_deref(), Some("abc123"));
        assert_eq!(record.size, Some(42));
    }
}
