use crate::prelude::*;
use tokio::fs;

pub type RecordsDB = HashMap<String, Ipv4Addr>;

/// Load the records from the supplied file path. The format of the file is lines of name to IPv4.
/// Name must end with .loc. Returns empty [`RecordsDB`] if file does not exist.
///
/// e.g.:
///
/// zero.loc:0.0.0.0
pub async fn load(file: impl AsRef<Path>, tld: &str) -> Result<RecordsDB> {
    if fs::try_exists(&file).await? {
        load_from_file(file, tld).await
    } else {
        debug!("Using empty records");
        Ok(HashMap::new())
    }
}

/// Load the records from the supplied file path. The format of the file is lines of name to IPv4.
/// Name must end with .loc. Returns error if file does not exist.
///
/// e.g.:
///
/// zero.loc:0.0.0.0
pub async fn load_from_file(file: impl AsRef<Path>, tld: &str) -> Result<RecordsDB> {
    debug!("Loading records from file: {}", file.as_ref().display());
    let contents = fs::read_to_string(&file).await?;
    let mut records = HashMap::new();
    for line in contents.lines() {
        match line {
            "" => (),
            s if s.starts_with('#') => (),
            s => {
                let (name, ip) = parse_line(s).context(format!("trying to parse '{s}'"))?;
                if records.contains_key(&name) {
                    return Err(anyhow!("Duplicate hostname: {name}"));
                }
                if !name.ends_with(tld) {
                    send_notification(
                        "Invalid record in records file",
                        &format!("Invalid TopLevelDomain in: {name}"),
                    );
                    continue;
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

pub fn safe_open_records_file(f: &PathBuf) -> Result<()> {
    if !f.exists() {
        create_records_file(f)?;
    }
    open_path(f)
}

fn create_records_file(f: impl AsRef<Path>) -> Result<()> {
    let msg = include_bytes!("../../resources/records.txt");
    let mut file = File::create(f)?;
    file.write_all(msg)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[tokio::test]
    async fn ignore_invalid_top_level_domains() {
        let records_contents = "hello.loc:127.0.0.1\nhello.com:127.0.0.1\n";
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(records_contents.as_bytes()).unwrap();
        let records = load_from_file(file.path(), "loc").await.unwrap();
        assert!(
            !records.contains_key("hello.com"),
            "hello.com should not be in records"
        );
    }
}
