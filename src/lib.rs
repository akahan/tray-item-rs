mod api;
mod error;

use bitflags::bitflags;
pub use error::TIError;
use windows_sys::Win32::UI::Shell::*;

pub struct TrayItem(api::TrayItemImpl);

#[derive(Clone)]
pub enum IconSource {
    Resource(&'static str),
    #[cfg(all(target_os = "linux", feature = "ksni"))]
    Data {
        height: i32,
        width: i32,
        data: Vec<u8>,
    },
}

impl IconSource {
    pub fn as_str(&self) -> &str {
        match self {
            IconSource::Resource(res) => res,
            #[allow(unreachable_patterns)]
            _ => unimplemented!(),
        }
    }
}

impl TrayItem {
    pub fn new(title: &str, icon: IconSource) -> Result<Self, TIError> {
        Ok(Self(api::TrayItemImpl::new(title, icon)?))
    }

    pub fn set_icon(&mut self, icon: IconSource) -> Result<(), TIError> {
        self.0.set_icon(icon)
    }

    pub fn add_label(&mut self, label: &str) -> Result<(), TIError> {
        self.0.add_label(label)
    }

    pub fn add_menu_item<F>(&mut self, label: &str, cb: F) -> Result<(), TIError>
    where
        F: Fn() + Send + Sync + 'static,
    {
        self.0.add_menu_item(label, cb)
    }

    pub fn inner_mut(&mut self) -> &mut api::TrayItemImpl {
        &mut self.0
    }

    pub fn show_toast(
        &mut self,
        text: &str,
        icon: IconSource,
        title: Option<&str>,
        flags: Option<TrayNotificationFlags>,
    ) -> Result<(), TIError> {
        self.0.show_toast(text, icon, title, flags)
    }
}

bitflags! {
    pub struct TrayNotificationFlags: u32 {
        const NO_ICON = NIIF_NONE;
        const INFO_ICON = NIIF_INFO;
        const WARNING_ICON = NIIF_WARNING;
        const ERROR_ICON = NIIF_ERROR;
        const USER_ICON = NIIF_USER;
        const SILENT = NIIF_NOSOUND;
        const LARGE_ICON = NIIF_LARGE_ICON;
        const QUIET = NIIF_RESPECT_QUIET_TIME;
    }
}
