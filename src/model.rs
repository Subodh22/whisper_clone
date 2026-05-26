use anyhow::{anyhow, Result};
use std::fs;
use std::io::{Read, Write};
use std::path::PathBuf;

use indicatif::{ProgressBar, ProgressStyle};

const HUGGINGFACE_BASE_URL: &str =
    "https://huggingface.co/ggerganov/whisper.cpp/resolve/main";

/// Returns the path to the VoxType data directory (~/.voxtype/models/).
fn models_dir() -> Result<PathBuf> {
    let home = dirs::home_dir().ok_or_else(|| anyhow!("Cannot determine home directory"))?;
    let dir = home.join(".voxtype").join("models");
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// Returns the local path for a given model name (e.g. "tiny.en" → ~/.voxtype/models/ggml-tiny.en.bin).
fn model_path(model_name: &str) -> Result<PathBuf> {
    Ok(models_dir()?.join(format!("ggml-{}.bin", model_name)))
}

/// Ensures the requested Whisper model is available locally.
/// Downloads it from Hugging Face if it doesn't exist yet.
/// Returns the path to the model file.
pub fn ensure_model(model_name: &str) -> Result<PathBuf> {
    let path = model_path(model_name)?;

    if path.exists() {
        let size = fs::metadata(&path)?.len();
        eprintln!(
            "✅ Model '{}' found ({:.1} MB)",
            model_name,
            size as f64 / 1_048_576.0
        );
        return Ok(path);
    }

    eprintln!("📦 Model '{}' not found locally. Downloading...", model_name);
    download_model(model_name, &path)?;

    Ok(path)
}

/// Downloads a Whisper GGML model from Hugging Face with a progress bar.
fn download_model(model_name: &str, dest: &PathBuf) -> Result<()> {
    let url = format!("{}/ggml-{}.bin", HUGGINGFACE_BASE_URL, model_name);
    eprintln!("   URL: {}", url);

    let mut response = reqwest::blocking::get(&url)?;

    if !response.status().is_success() {
        return Err(anyhow!(
            "Download failed with status {}. Check that model '{}' exists.",
            response.status(),
            model_name
        ));
    }

    let total_size = response.content_length().unwrap_or(0);

    let pb = ProgressBar::new(total_size);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("   {spinner:.green} [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta})")
            .unwrap()
            .progress_chars("█▓░"),
    );

    // Download to a temp file first, then rename on success
    let temp_path = dest.with_extension("part");
    let mut file = fs::File::create(&temp_path)?;

    let mut buffer = [0u8; 8192];
    loop {
        let bytes_read = response.read(&mut buffer)?;
        if bytes_read == 0 {
            break;
        }
        file.write_all(&buffer[..bytes_read])?;
        pb.inc(bytes_read as u64);
    }

    file.flush()?;
    drop(file);

    // Rename temp file to final path
    fs::rename(&temp_path, dest)?;
    pb.finish_with_message("Download complete");
    eprintln!("   ✅ Saved to {}\n", dest.display());

    Ok(())
}

/// Lists available model names and their approximate sizes.
pub fn available_models() -> Vec<(&'static str, &'static str)> {
    vec![
        ("tiny.en", "~75 MB — fastest, English only"),
        ("tiny", "~75 MB — fastest, multilingual"),
        ("base.en", "~142 MB — balanced, English only"),
        ("base", "~142 MB — balanced, multilingual"),
        ("small.en", "~466 MB — most accurate, English only"),
        ("small", "~466 MB — most accurate, multilingual"),
    ]
}
