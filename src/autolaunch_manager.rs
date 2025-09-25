use crate::prelude::*;
use auto_launch::{AutoLaunch, AutoLaunchBuilder};

pub trait AutoLaunchManager {
    fn enable(&self) -> Result<()>;
    fn disable(&self) -> Result<()>;
    fn is_enabled(&self) -> Result<bool>;
}

impl AutoLaunchManager for AutoLaunch {
    #[cfg(not(debug_assertions))]
    fn enable(&self) -> Result<()> {
        self.enable().map_err(Error::from)
    }

    #[cfg(debug_assertions)]
    fn enable(&self) -> Result<()> {
        Err(anyhow!("Not supported in DEBUG mode!"))
    }

    #[cfg(not(debug_assertions))]
    fn disable(&self) -> Result<()> {
        self.disable().map_err(Error::from)
    }

    #[cfg(debug_assertions)]
    fn disable(&self) -> Result<()> {
        Err(anyhow!("Not supported in DEBUG mode!"))
    }

    fn is_enabled(&self) -> Result<bool> {
        self.is_enabled().map_err(Error::from)
    }
}

pub fn mk_auto_launch() -> Result<AutoLaunch> {
    let exe_path =
        std::env::current_exe().map_err(|e| anyhow!("Error identifying application path: {e}"))?;
    let exe_str = exe_path.to_str().ok_or(anyhow!("Invalid exe path"))?;
    AutoLaunchBuilder::new()
        .set_app_name(APP_NAME)
        .set_app_path(exe_str)
        .build()
        .map_err(Error::from)
}
