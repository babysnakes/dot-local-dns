use crate::app_config::AppConfig;
use crate::autolaunch_manager::AutoLaunchManager;
use crate::dns::Notification;
use crate::dns::Notification::{Reload, Shutdown};
use crate::shared::{
    error_message, notify_error, open_path, panic_with_error, send_notification, APP_NAME,
    APP_VERSION,
};
use anyhow::{Context, Error, Result};
use log::{debug, error, info};
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
use tokio::sync::mpsc::Sender;
use tray_icon::menu::{AboutMetadata, AboutMetadataBuilder, CheckMenuItem};
use tray_icon::{
    menu::{Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem},
    TrayIcon, TrayIconBuilder,
};
use winit::{
    application::ApplicationHandler,
    event::{StartCause, WindowEvent},
    event_loop::ActiveEventLoop,
    event_loop::EventLoop,
    window::WindowId,
};

const QUIT_ID: &str = "quit";
const RELOAD_ID: &str = "reload";
const LOGS_ID: &str = "log_dir";
const STARTUP_ID: &str = "startup";
const RECORDS_ID: &str = "edit_records";

pub struct Application<'a> {
    tray_app: Option<TrayIcon>,
    notification_tx: Sender<Notification>,
    app_config: &'a mut AppConfig,
    startup_menu: CheckMenuItem,
    auto_launch_manager: &'a dyn AutoLaunchManager,
}

#[derive(Debug)]
pub(crate) enum UserEvent {
    MenuEvent(MenuEvent),
    Shutdown,
}

impl<'a> Application<'a> {
    pub fn new(
        event_loop: &EventLoop<UserEvent>,
        notification_tx: Sender<Notification>,
        app_config: &'a mut AppConfig,
        auto_launch_manager: &'a dyn AutoLaunchManager,
    ) -> Result<Self> {
        let proxy = event_loop.create_proxy();
        MenuEvent::set_event_handler(Some(move |event| {
            proxy
                .send_event(UserEvent::MenuEvent(event))
                .unwrap_or_else(|e| {
                    notify_error!("Failed forwarding event: {e}");
                });
        }));
        let start_flag = app_config.start_at_login;
        let app = Self {
            tray_app: None,
            notification_tx,
            app_config,
            startup_menu: CheckMenuItem::with_id(
                STARTUP_ID,
                "Startup at Login",
                true,
                start_flag,
                None,
            ),
            auto_launch_manager,
        };
        if start_flag != app.auto_launch_manager.is_enabled()? {
            notify_user_about_mismatch_auto_launch(start_flag, !start_flag);
            app.app_config.set_start_at_login(!start_flag)?;
            app.startup_menu.set_checked(!start_flag);
        }
        Ok(app)
    }

    fn create_tray(&self) -> TrayIcon {
        let icon_data = include_bytes!("../resources/Icon.png");
        let icon = load_icon(icon_data);
        let menu = self.create_menu();

        TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_menu_on_left_click(true)
            .with_tooltip("DotLocal DNS")
            .with_icon(icon)
            .build()
            .unwrap_or_else(|e| {
                panic_with_error!("Error creating tray icon: {e}");
            })
    }

    fn create_menu(&self) -> Menu {
        let quit_i = MenuItem::with_id(QUIT_ID, "Quit", true, None);
        let reload_i = MenuItem::with_id(RELOAD_ID, "Reload Records", true, None);
        let logs_i = MenuItem::with_id(LOGS_ID, "Open Logs Directory", true, None);
        let records_i = MenuItem::with_id(RECORDS_ID, "Edit Records File", true, None);
        Menu::with_items(&[
            &PredefinedMenuItem::about("About".into(), Some(about_manifest())),
            &reload_i,
            &records_i,
            &logs_i,
            &self.startup_menu,
            &PredefinedMenuItem::separator(),
            &quit_i,
        ])
        .unwrap_or_else(|e| {
            panic_with_error!("Error creating menu: {e}");
        })
    }

    fn set_auto_launch(&mut self, launch: bool) -> Result<()> {
        self.app_config.set_start_at_login(launch)?;
        if launch {
            self.auto_launch_manager.enable()
        } else {
            self.auto_launch_manager.disable()
        }
    }
}

impl ApplicationHandler<UserEvent> for Application<'_> {
    fn new_events(&mut self, _event_loop: &ActiveEventLoop, cause: StartCause) {
        if StartCause::Init == cause {
            self.tray_app = Some(self.create_tray());
        }
    }

    fn resumed(&mut self, _event_loop: &ActiveEventLoop) {}

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: UserEvent) {
        match event {
            UserEvent::MenuEvent(MenuEvent { id: MenuId(id) }) if id == QUIT_ID => {
                info!("Shutting down");
                let tx = self.notification_tx.clone();
                tokio::spawn(async move {
                    tx.send(Shutdown).await.unwrap_or_else(|e| {
                        notify_error!("Error sending shutdown message to application: {e}");
                    });
                });
                event_loop.exit();
            }
            UserEvent::MenuEvent(MenuEvent { id: MenuId(id) }) if id == RELOAD_ID => {
                debug!("Reloading Records");
                let tx = self.notification_tx.clone();
                tokio::spawn(async move {
                    tx.send(Reload).await.unwrap_or_else(|e| {
                        notify_error!("Error sending reload records message: {e}");
                    });
                });
            }
            UserEvent::MenuEvent(MenuEvent { id: MenuId(id) }) if id == LOGS_ID => {
                debug!("Open logs directory");
                if let Err(e) = open_path(&self.app_config.logging_dir) {
                    notify_error!("Error opening logs directory: {e}");
                }
            }
            UserEvent::MenuEvent(MenuEvent { id: MenuId(id) }) if id == STARTUP_ID => {
                let enabled = self.startup_menu.is_checked();
                let verb = if enabled { "setting" } else { "disabling" };
                self.set_auto_launch(enabled).unwrap_or_else(|e| {
                    error!("Error {verb} start at login: {e}");
                    error_message(format!("Error {verb} start at login: {e}"));
                });
            }
            UserEvent::MenuEvent(MenuEvent { id: MenuId(id) }) if id == RECORDS_ID => {
                debug!("Edit records file");
                if let Err(e) =
                    safe_open_file(&self.app_config.records_file).context("opening records file")
                {
                    error!("Error: {e:#}");
                    error_message(format!("Error: {e:#}"));
                }
            }
            UserEvent::MenuEvent(_) => {}
            UserEvent::Shutdown => {
                event_loop.exit();
            }
        }
    }

    fn window_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        _event: WindowEvent,
    ) {
    }
}

fn load_icon(resource: &[u8]) -> tray_icon::Icon {
    load_rgba(resource)
        .and_then(|(rgba, width, height)| {
            tray_icon::Icon::from_rgba(rgba, width, height).map_err(Error::from)
        })
        .unwrap_or_else(|e| {
            panic_with_error!("Error loading icon: {e}");
        })
}

fn load_about_icon(resource: &[u8]) -> Option<tray_icon::menu::Icon> {
    load_rgba(resource)
        .and_then(|(rgba, width, height)| {
            tray_icon::menu::Icon::from_rgba(rgba, width, height).map_err(Error::from)
        })
        .inspect_err(|e| {
            notify_error!("Error loading icon: {e}");
        })
        .ok()
}

fn load_rgba(resource: &[u8]) -> Result<(Vec<u8>, u32, u32)> {
    let img = image::load_from_memory(resource)?;
    let rgb = img.into_rgba8();
    let (width, height) = rgb.dimensions();
    Ok((rgb.into_raw(), width, height))
}

fn about_manifest() -> AboutMetadata {
    let icon_data = include_bytes!("../resources/Icon.png");
    let icon = load_about_icon(icon_data);
    AboutMetadataBuilder::new()
        .name(Some(APP_NAME))
        .icon(icon)
        .version(Some(APP_VERSION))
        .build()
}

fn notify_user_about_mismatch_auto_launch(app: bool, system: bool) {
    let tr = |b: bool| {
        if b {
            "enabled"
        } else {
            "disabled"
        }
    };
    let in_app = tr(app);
    let in_system = tr(system);
    let msg = format!(
        concat!(
            "There is a mismatch in configured starting at login between the application ",
            r#"({}) and the system ({})!"#,
            "\n\nWe've set the application to match the system settings ({}). You can set it to",
            "your liking using the menu in the system tray."
        ),
        in_app, in_system, in_system
    );

    error_message(msg);
}

fn safe_open_file(f: &PathBuf) -> Result<()> {
    if !f.exists() {
        create_records_file(f)?;
    }
    open_path(f)
}

fn create_records_file(f: &PathBuf) -> Result<()> {
    let msg = include_bytes!("../resources/records.txt");
    let mut file = File::create(f)?;
    file.write_all(msg)?;
    Ok(())
}
