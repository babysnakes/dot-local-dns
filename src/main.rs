#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // Don't show console on Windows
#![warn(clippy::pedantic)]
#![allow(clippy::enum_glob_use)]

mod app_config;
mod autolaunch_manager;
mod dns;
mod logging;
mod shared;
mod tray_app;

mod prelude {
    pub(crate) use crate::app_config::AppConfig;
    pub(crate) use crate::autolaunch_manager::{mk_auto_launch, AutoLaunchManager};
    pub(crate) use crate::dns::safe_open_records_file;
    pub(crate) use crate::dns::DnsServer;
    pub(crate) use crate::dns::Notification::{self, ARecordQuery, MergeRecords, Reload, Shutdown};
    pub(crate) use crate::logging::configure_logging;
    pub(crate) use crate::shared::*;
    pub(crate) use crate::tray_app::{Application, UserEvent};
    pub(crate) use anyhow::{anyhow, Context, Error, Result};
    pub(crate) use log::{debug, error, info, trace, warn};
    pub(crate) use std::collections::HashMap;
    pub(crate) use std::fs::{self, File};
    pub(crate) use std::io::Write;
    pub(crate) use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr};
    pub(crate) use std::path::{Path, PathBuf};
    pub(crate) use tokio::sync::mpsc::{self, Receiver, Sender};
    pub(crate) use tokio::sync::oneshot;
}

use prelude::*;
use winit::event_loop::EventLoop;

#[tokio::main]
#[cfg(target_os = "windows")]
async fn main() {
    if let Err(e) = run().await {
        error!("DNS server error: {e}");
        error_message(format!("{e}"));
    }
}

async fn run() -> Result<()> {
    let mut app_config = AppConfig::new()?;
    configure_logging(&app_config.log_level, &app_config.logging_dir)?;
    let mut dns_server = DnsServer::new(
        app_config.port,
        &app_config.records_file,
        &app_config.top_level_domain,
    )
    .await?;
    let event_loop = EventLoop::<UserEvent>::with_user_event().build()?;
    let notify_tx = dns_server.notify_tx.clone();
    let shutdown_proxy = event_loop.create_proxy();
    let auto = mk_auto_launch()?;
    tokio::spawn(async move {
        dns_server.run().await.unwrap_or_else(|e| {
            error!("DNS server error: {e}");
            error_message(format!("{e}"));
            _ = shutdown_proxy.send_event(UserEvent::Shutdown);
        });
    });
    let mut app = Application::new(&event_loop, notify_tx, &mut app_config, &auto)
        .context("Creating system tray application")?;
    event_loop.run_app(&mut app)?;
    Ok(())
}
