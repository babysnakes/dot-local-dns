use crate::constants::app_config_dir;
use anyhow::Result;
use flexi_logger::{Cleanup, Criterion, FileSpec, Logger, Naming};

pub fn configure_logging() -> Result<()> {
    if cfg!(debug_assertions) {
        Logger::try_with_str("debug")?.start()?;
    } else {
        Logger::try_with_str("info")?
            .log_to_file(
                FileSpec::default()
                    .directory(app_config_dir()?)
                    .basename("application"),
            )
            .rotate(
                Criterion::Size(10_000_000),
                Naming::Numbers,
                Cleanup::KeepLogFiles(7),
            )
            .start()?;
    }
    Ok(())
}
