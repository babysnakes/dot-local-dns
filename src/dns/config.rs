use super::super::constants::*;
use anyhow::{anyhow, Context, Result};
use std::collections::HashMap;
use std::net::Ipv4Addr;
use std::path::{Path, PathBuf};
use tokio::fs;

pub struct DotLocalDNSConfig {
    pub records: HashMap<String, Ipv4Addr>,
}

impl DotLocalDNSConfig {
    /// Creates a .local domain config, either from config file or with only localhost resolving.
    pub async fn new() -> Result<Self> {
        let config_file = get_config_path()?;
        let exists = fs::try_exists(&config_file).await?;
        let config = if exists {
            Self::from_file(config_file).await?
        } else {
            Self {
                records: HashMap::new(),
            }
        };
        Ok(config)
    }

    /// Load the config from the supplied file path. The format of the file is lines of name to IPv4.
    /// Name must end with .local.
    ///
    /// e.g.:
    ///
    /// zero.local:0.0.0.0
    pub async fn from_file(file: impl AsRef<Path>) -> Result<Self> {
        let contents = fs::read_to_string(&file).await?;
        let mut records = HashMap::new();
        for line in contents.lines() {
            match line {
                "" => continue,
                s if s.starts_with("#") => continue,
                s => {
                    let (name, ip) = parse_line(s).context(format!("trying to parse '{s}'"))?;
                    if records.contains_key(&name) {
                        return Err(anyhow!("Duplicate hostname: {name}"));
                    }
                    records.insert(name, ip);
                }
            }
        }
        Ok(Self { records })
    }
}

fn parse_line(line: &str) -> Result<(String, Ipv4Addr)> {
    let mut parts = line.splitn(2, ":");
    let name = parts.next().ok_or(anyhow!("Missing hostname"))?;
    let ip: Ipv4Addr = parts.next().ok_or(anyhow!("Missing IP"))?.parse()?;
    Ok((name.to_owned(), ip))
}

fn get_config_path() -> Result<PathBuf> {
    use simple_home_dir::*;
    match home_dir() {
        Some(mut home) => {
            home.push(CONFIG_FILE_NAME);
            Ok(home)
        }
        None => Err(anyhow!("FATAL! Couldn't extract home directory")),
    }
}
