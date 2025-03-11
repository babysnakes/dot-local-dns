use anyhow::{Context, Result};
use std::path::PathBuf;

pub const TOP_LEVEL_DOMAIN: &str = ".local";
pub const CONFIG_FILE_NAME: &str = ".local.ns-entries";
pub const APP_NAME: &str = "DotLocal-DNS";

pub fn app_config_dir() -> Result<PathBuf> {
    let mut path = dirs::config_dir().with_context(|| "Could not find config directory")?;
    path.push(APP_NAME);
    Ok(path)
}
