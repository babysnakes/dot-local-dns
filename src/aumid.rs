use crate::prelude::*;
use windows_registry::CURRENT_USER;

const AUMID_REG_PARENT_PATH: &str = r"Software\Classes\AppUserModelId";

/// Registers the app's AUMID in the Windows registry so toast notifications
/// work even without the NSIS installer (e.g. zip extraction or dev builds).
pub fn ensure_registered() {
    if let Err(e) = register() {
        warn!("Failed to register AUMID in registry: {e}");
    }
}

fn register() -> Result<()> {
    let reg_path = format!(r"{AUMID_REG_PARENT_PATH}\{APP_IDENTIFIER}");
    let key = CURRENT_USER
        .create(&reg_path)
        .context("Creating AUMID registry key")?;

    key.set_expand_string("DisplayName", APP_NAME)
        .context("Setting DisplayName")?;

    key.set_string("IconBackgroundColor", "0")
        .context("Setting IconBackgroundColor")?;

    if let Some(icon_path) = find_icon() {
        key.set_expand_string("IconUri", &icon_path.to_string_lossy())
            .context("Setting IconUri")?;
        debug!("AUMID registered with icon: {}", icon_path.display());
    } else {
        debug!("AUMID registered without icon (icon not found)");
    }

    Ok(())
}

fn find_icon() -> Option<PathBuf> {
    let exe_dir = std::env::current_exe().ok()?.parent()?.to_path_buf();
    let icon = exe_dir.join(r"resources\Icon.png");
    if icon.exists() {
        return Some(icon);
    }
    // Dev builds: exe is in target\debug, icon is in project root resources/
    let project_icon = exe_dir
        .parent()?
        .parent()?
        .join(r"resources\Icon.png");
    if project_icon.exists() {
        return Some(project_icon);
    }
    None
}
