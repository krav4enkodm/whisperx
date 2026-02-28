use std::fs;
use std::path::Path;

use anyhow::{Context, Result};

pub fn ensure_dir(path: &Path) -> Result<()> {
    fs::create_dir_all(path)
        .with_context(|| format!("failed to create directory {}", path.display()))
}
