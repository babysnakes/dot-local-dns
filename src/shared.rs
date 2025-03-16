use anyhow::Result;
use log::error;
use notify_rust::Notification;
use std::path::PathBuf;

pub const APP_NAME: &str = "DotLocal-DNS";
pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

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
    use windows_sys::Win32::UI::WindowsAndMessaging::{MessageBoxA, MB_ICONERROR, MB_OK};

    let title = format!("{APP_NAME} Error");
    tokio::task::spawn_blocking(move || unsafe {
        MessageBoxA(
            0 as _,
            body.as_ptr().cast(),
            title.as_ptr().cast(),
            MB_OK | MB_ICONERROR,
        );
    });
}

pub fn open_path(path: &PathBuf) -> Result<()> {
    open::that(path)?;
    Ok(())
}
