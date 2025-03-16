use crate::shared::APP_NAME;
use anyhow::{anyhow, Context, Result};
use std::path::PathBuf;

#[allow(dead_code)] // todo: remove once we use all fields
pub struct AppConfig {
    pub top_level_domain: String,
    pub port: u16,
    pub log_level: Option<String>,
    pub logging_dir: PathBuf,
    pub records_file: PathBuf,
    pub config_dir: PathBuf,
    pub start_at_login: Option<bool>,
    pub config_revision: ConfigRevision,
}

#[allow(dead_code)] // todo: remove once we use it
pub struct ConfigRevision {
    revision: u8,
}

impl AppConfig {
    pub fn load() -> Result<Self> {
        let config_dir = app_config_dir()?;
        let mut records_file =
            dirs::home_dir().ok_or_else(|| anyhow!("Failed to get home directory"))?;
        records_file.push(".dot-local-records");
        let mut logging_dir = config_dir.clone();
        logging_dir.push("logs");
        std::fs::create_dir_all(&logging_dir)?;
        Ok(AppConfig {
            top_level_domain: ".local".to_string(),
            port: 53,
            log_level: None,
            logging_dir,
            records_file,
            config_dir,
            start_at_login: None,
            config_revision: ConfigRevision { revision: 0 },
        })
    }
}

pub fn app_config_dir() -> Result<PathBuf> {
    let mut path = dirs::config_dir().with_context(|| "Could not find config directory")?;
    path.push(APP_NAME);
    Ok(path)
}
