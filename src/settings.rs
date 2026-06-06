use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    /// Whisper language code: "en", "es", "fr", "de", "zh", etc.
    /// Use "auto" to let Whisper detect the language automatically (slower).
    pub language: String,

    /// Recording auto-stops after this many seconds. Prevents accidentally
    /// holding the hotkey and generating a huge audio file.
    pub max_recording_secs: u32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            language: "en".to_string(),
            max_recording_secs: 60,
        }
    }
}

fn config_path() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".bol").join("config.toml"))
}

impl Settings {
    /// Load settings from ~/.bol/config.toml. Returns defaults if the file
    /// doesn't exist or can't be parsed.
    pub fn load() -> Self {
        let path = match config_path() {
            Some(p) => p,
            None => return Self::default(),
        };
        match fs::read_to_string(&path) {
            Ok(content) => toml::from_str(&content).unwrap_or_else(|e| {
                eprintln!("Warning: could not parse config ({}), using defaults", e);
                Self::default()
            }),
            Err(_) => {
                // First run — write a default config so the user can find and edit it.
                let s = Self::default();
                let _ = s.save();
                s
            }
        }
    }

    /// Save settings to ~/.bol/config.toml.
    pub fn save(&self) -> Result<()> {
        let path = config_path()
            .ok_or_else(|| anyhow::anyhow!("Cannot determine home directory"))?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let content = toml::to_string_pretty(self)?;
        fs::write(&path, content)?;
        Ok(())
    }
}
