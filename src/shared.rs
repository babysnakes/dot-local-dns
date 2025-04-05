use anyhow::Result;
use log::error;
use notify_rust::Notification;
use std::path::PathBuf;

pub const APP_NAME: &str = "DotLocal-DNS";
pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const DEFAULT_TOP_LEVEL_DOMAIN: &str = ".local";
pub const LOGS_DIR_NAME: &str = "logs";
pub const DEFAULT_RECORDS_FILE_NAME: &str = "records.txt";

macro_rules! notify_error {
    ($($arg:tt)+) => {
        // todo: Can we do it without alocating strings?
        let msg = format!($($arg)+);
        let summary = format!("{APP_NAME} Error");
        error!($($arg)+);
        send_notification(&summary, &msg);
    };
}

macro_rules! panic_with_error {
    ($($arg:tt)+) => {
        error!($($arg)+);
        error_message(format!($($arg)+));
        panic!("{}", format_args!($($arg)+));
    };
}

pub(crate) use notify_error;
pub(crate) use panic_with_error;

#[cfg(target_os = "windows")]
pub fn send_notification(summary: &str, body: &str) {
    Notification::new()
        .summary(summary)
        .body(body)
        .show()
        .unwrap_or_else(|e| error!("{}", e));
}

pub fn error_message(body: String) {
    use windows_strings::HSTRING;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        MessageBoxW, MB_ICONERROR, MB_OK, MB_SYSTEMMODAL, MB_TOPMOST,
    };

    let title = format!("{APP_NAME} Error");
    tokio::task::spawn_blocking(move || unsafe {
        MessageBoxW(
            0 as _,
            HSTRING::from(body).as_ptr(),
            HSTRING::from(title).as_ptr(),
            MB_OK | MB_ICONERROR | MB_TOPMOST | MB_SYSTEMMODAL,
        );
    });
}

pub fn open_path(path: &PathBuf) -> Result<()> {
    open::that(path)?;
    Ok(())
}
