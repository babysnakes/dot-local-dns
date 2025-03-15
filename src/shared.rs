use anyhow::{Context, Result};
use log::error;
use notify_rust::Notification;
use std::path::PathBuf;
use tray_icon::menu::AboutMetadata;

pub const TOP_LEVEL_DOMAIN: &str = ".local";
pub const RECORDS_FILE_NAME: &str = ".dot-local-records";
pub const APP_NAME: &str = "DotLocal-DNS";
pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

pub fn app_config_dir() -> Result<PathBuf> {
    let mut path = dirs::config_dir().with_context(|| "Could not find config directory")?;
    path.push(APP_NAME);
    Ok(path)
}

pub fn about_manifest() -> AboutMetadata {
    AboutMetadata {
        name: Some(APP_NAME.to_owned()),
        version: Some(APP_VERSION.into()),
        short_version: None,
        authors: None,
        comments: None,
        copyright: None,
        license: None,
        website: None,
        website_label: None,
        credits: None,
        icon: None,
    }
}

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
