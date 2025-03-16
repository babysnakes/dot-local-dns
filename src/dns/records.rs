use anyhow::{anyhow, Context, Result};
use log::debug;
use std::collections::HashMap;
use std::net::Ipv4Addr;
use std::path::Path;
use tokio::fs;

pub type RecordsDB = HashMap<String, Ipv4Addr>;

/// Load the records from the supplied file path. The format of the file is lines of name to IPv4.
/// Name must end with .local. Returns empty [`RecordsDB`] if file does not exist.
///
/// e.g.:
///
/// zero.local:0.0.0.0
pub async fn try_from_file(file: impl AsRef<Path>) -> Result<RecordsDB> {
    if fs::try_exists(&file).await? {
        load_from_file(file).await
    } else {
        debug!("Using empty records");
        Ok(HashMap::new())
    }
}

/// Load the records from the supplied file path. The format of the file is lines of name to IPv4.
/// Name must end with .local. Returns error if file does not exist.
///
/// e.g.:
///
/// zero.local:0.0.0.0
pub async fn load_from_file(file: impl AsRef<Path>) -> Result<RecordsDB> {
    debug!("Loading records from file: {:?}", file.as_ref());
    let contents = fs::read_to_string(&file).await?;
    let mut records = HashMap::new();
    for line in contents.lines() {
        match line {
            "" => continue,
            s if s.starts_with('#') => continue,
            s => {
                let (name, ip) = parse_line(s).context(format!("trying to parse '{s}'"))?;
                if records.contains_key(&name) {
                    return Err(anyhow!("Duplicate hostname: {name}"));
                }
                records.insert(name, ip);
            }
        }
    }
    Ok(records)
}

fn parse_line(line: &str) -> Result<(String, Ipv4Addr)> {
    debug!("parsing line: {line}");
    let mut parts = line.splitn(2, ':');
    let name = parts.next().ok_or(anyhow!("Missing hostname"))?;
    let ip: Ipv4Addr = parts.next().ok_or(anyhow!("Missing IP"))?.parse()?;
    Ok((name.to_owned(), ip))
}
