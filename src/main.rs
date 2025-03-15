#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![warn(clippy::pedantic)]
#![allow(clippy::enum_glob_use)]

use crate::dns::DnsServer;
use crate::logging::configure_logging;
use crate::shared::error_message;
use crate::tray_app::{Application, UserEvent};
use anyhow::Result;
use log::error;
use winit::event_loop::EventLoop;

mod dns;
mod logging;
mod shared;
mod tray_app;

#[tokio::main]
async fn main() -> Result<()> {
    configure_logging()?;
    let mut dns_server = DnsServer::new(53, None).await?;
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
    let mut app = Application::new(&event_loop, notify_tx);
    if let Err(e) = event_loop.run_app(&mut app) {
        error_message(format!("{e}"));
        error!("{e}");
    }
    Ok(())
}
