use crate::dns::Notification;
use crate::dns::Notification::{Reload, Shutdown};
use crate::shared::{
    about_manifest, error_message, notify_error, panic_with_error, send_notification, APP_NAME,
};
use anyhow::Error;
use log::{debug, error, info};
use tokio::sync::mpsc::Sender;
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

pub struct Application {
    tray_app: Option<TrayIcon>,
    notification_tx: Sender<Notification>,
}

pub(crate) enum UserEvent {
    MenuEvent(MenuEvent),
    Shutdown,
}

impl Application {
    pub fn new(event_loop: &EventLoop<UserEvent>, notification_tx: Sender<Notification>) -> Self {
        let proxy = event_loop.create_proxy();
        MenuEvent::set_event_handler(Some(move |event| {
            proxy
                .send_event(UserEvent::MenuEvent(event))
                .unwrap_or_else(|e| {
                    notify_error!("Failed forwarding event: {e}");
                });
        }));
        Self {
            tray_app: None,
            notification_tx,
        }
    }

    fn create_tray() -> TrayIcon {
        let icon_data = include_bytes!("../resources/Icon.png");
        let icon = load_icon(icon_data);
        let menu = Self::create_menu();

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

    fn create_menu() -> Menu {
        let quit_i = MenuItem::with_id(QUIT_ID, "Quit", true, None);
        let reload_i = MenuItem::with_id(RELOAD_ID, "Reload Records", true, None);
        Menu::with_items(&[
            &PredefinedMenuItem::about("About".into(), Some(about_manifest())),
            &reload_i,
            &PredefinedMenuItem::separator(),
            &quit_i,
        ])
        .unwrap_or_else(|e| {
            panic_with_error!("Error creating menu: {e}");
        })
    }
}

impl ApplicationHandler<UserEvent> for Application {
    fn new_events(&mut self, _event_loop: &ActiveEventLoop, cause: StartCause) {
        if StartCause::Init == cause {
            self.tray_app = Some(Self::create_tray());
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
    image::load_from_memory(resource)
        .map(image::DynamicImage::into_rgba8)
        .map_err(Error::from)
        .map(|img| {
            let (width, height) = img.dimensions();
            let rgba = img.into_raw();
            (rgba, width, height)
        })
        .and_then(|(rgba, width, height)| {
            tray_icon::Icon::from_rgba(rgba, width, height).map_err(Error::from)
        })
        .unwrap_or_else(|err| {
            panic_with_error!("Error loading icon: {err}");
        })
}
