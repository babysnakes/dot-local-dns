use crate::prelude::*;
use flexi_logger::{detailed_format, Cleanup, Criterion, FileSpec, Logger, Naming};

pub fn configure_logging(log_level: &str, logging_dir: &PathBuf) -> Result<()> {
    if cfg!(debug_assertions) {
        Logger::try_with_str(log_level)?.start()?;
    } else {
        Logger::try_with_str(log_level)?
            .log_to_file(
                FileSpec::default()
                    .directory(logging_dir)
                    .basename("application"),
            )
            .rotate(
                Criterion::Size(10_000_000),
                Naming::Numbers,
                Cleanup::KeepLogFiles(7),
            )
            .format(detailed_format)
            .start()?;
    }
    Ok(())
}
