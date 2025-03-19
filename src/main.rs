#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![warn(clippy::pedantic)]
#![allow(clippy::enum_glob_use)]

use crate::app_config::AppConfig;
use crate::dns::DnsServer;
use crate::logging::configure_logging;
use crate::shared::error_message;
use crate::tray_app::{Application, UserEvent};
use anyhow::Result;
use log::error;
use winit::event_loop::EventLoop;

mod app_config;
mod dns;
mod logging;
mod shared;
mod tray_app;

#[tokio::main]
async fn main() {
    if let Err(e) = run().await {
        error!("DNS server error: {}", e);
        error_message(format!("{e}"));
    }
}

async fn run() -> Result<()> {
    let app_config = AppConfig::new()?;
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
    tokio::spawn(async move {
        dns_server.run().await.unwrap_or_else(|e| {
            error!("DNS server error: {}", e);
            error_message(format!("{e}"));
            _ = shutdown_proxy.send_event(UserEvent::Shutdown);
        });
    });
    let mut app = Application::new(&event_loop, notify_tx, &app_config);
    event_loop.run_app(&mut app)?;
    Ok(())
}
