use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AppConfig {
    pub last_tv_ip: Option<String>,
    pub tv_name: Option<String>,
    pub paired: bool,
}

pub fn get_config_dir() -> PathBuf {
    dirs::config_dir()
        .map(|p| p.join("atv-remote"))
        .unwrap_or_else(|| PathBuf::from(".atv-remote"))
}

impl AppConfig {
    pub fn load_from(path: &Path) -> Result<Self, Box<dyn std::error::Error>> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let data = fs::read_to_string(path)?;
        let cfg = serde_json::from_str(&data)?;
        Ok(cfg)
    }

    pub fn save_to(&self, path: &Path) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let data = serde_json::to_string_pretty(self)?;
        fs::write(path, data)?;
        Ok(())
    }

    pub fn load() -> Self {
        let path = get_config_dir().join("config.json");
        Self::load_from(&path).unwrap_or_default()
    }

    pub fn save(&self) -> Result<(), Box<dyn std::error::Error>> {
        let path = get_config_dir().join("config.json");
        self.save_to(&path)
    }
}
