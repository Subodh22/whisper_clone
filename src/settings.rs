use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

fn default_language() -> String { "en".to_string() }
fn default_max_recording_secs() -> u32 { 60 }
fn default_true() -> bool { true }
fn default_hotkey() -> String { "ctrl+shift+space".to_string() }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    /// Whisper language: "en", "es", "fr", etc. Use "auto" for detection.
    #[serde(default = "default_language")]
    pub language: String,

    /// Auto-stop recording after this many seconds.
    #[serde(default = "default_max_recording_secs")]
    pub max_recording_secs: u32,

    /// Play audio cues on start/stop.
    #[serde(default = "default_true")]
    pub feedback_sounds: bool,

    /// Show the recording overlay window while dictating.
    #[serde(default = "default_true")]
    pub show_overlay: bool,

    /// Hotkey combination. Modifiers: ctrl, shift, alt, cmd.
    /// Keys: space, a-z, 0-9, enter, tab.
    /// Examples: "ctrl+shift+space", "cmd+shift+d", "ctrl+alt+r"
    #[serde(default = "default_hotkey")]
    pub hotkey: String,

    /// Launch Bol automatically at login.
    #[serde(default)]
    pub launch_at_login: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            language: default_language(),
            max_recording_secs: default_max_recording_secs(),
            feedback_sounds: true,
            show_overlay: true,
            hotkey: default_hotkey(),
            launch_at_login: false,
        }
    }
}

fn config_path() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".bol").join("config.toml"))
}

impl Settings {
    /// Load settings from ~/.bol/config.toml. Returns defaults if missing or unparseable.
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
                // First run — write defaults so the user can discover and edit them.
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
