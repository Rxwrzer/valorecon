use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    #[serde(default)]
    pub region_override: String,
    #[serde(default = "default_poll")]
    pub poll_seconds: u64,
    #[serde(default)]
    pub henrik_key: String,
    #[serde(default = "default_pull_target")]
    pub profile_pull_target: i32,
    #[serde(default = "default_pull_source")]
    pub pull_source: String,
    #[serde(default)]
    pub always_on_top: bool,
    #[serde(default = "default_henrik_rate_limit")]
    pub henrik_rate_limit: u32,
    #[serde(default)]
    pub live_use_henrik: bool,
}

fn default_poll() -> u64 { 7 }
fn default_pull_target() -> i32 { 50 }
fn default_pull_source() -> String { "riot".into() }
fn default_henrik_rate_limit() -> u32 { 30 }

impl Default for Settings {
    fn default() -> Self {
        Self {
            region_override: String::new(),
            poll_seconds: 7,
            henrik_key: String::new(),
            profile_pull_target: 50,
            pull_source: "riot".into(),
            always_on_top: false,
            henrik_rate_limit: 30,
            live_use_henrik: false,
        }
    }
}

impl Settings {
    pub fn config_path() -> PathBuf {
        let base = std::env::var("APPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|_| dirs_next().unwrap_or_else(|| PathBuf::from(".")));
        base.join("ValoRecon").join("config.json")
    }

    pub fn load() -> Self {
        let path = Self::config_path();
        if let Ok(data) = std::fs::read_to_string(&path) {
            serde_json::from_str(&data).unwrap_or_default()
        } else {
            Self::default()
        }
    }

    pub fn save(&self) {
        let path = Self::config_path();
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(data) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(path, data);
        }
    }
}

fn dirs_next() -> Option<PathBuf> {
    // Fallback: use LOCALAPPDATA or home dir
    std::env::var("LOCALAPPDATA").ok().map(PathBuf::from)
        .or_else(|| std::env::var("USERPROFILE").ok().map(|h| PathBuf::from(h).join("AppData").join("Roaming")))
}
