use std::sync::mpsc::Sender;
use bitflags::bitflags;
use windows_sys::Win32::UI::Shell::*;

use windows_sys::Win32::{
    Foundation::{HMODULE, HWND},
    UI::WindowsAndMessaging::HMENU,
};

#[derive(Clone)]
pub(crate) struct WindowInfo {
    pub hwnd: HWND,
    pub hmodule: HMODULE,
    pub hmenu: HMENU,
}

unsafe impl Send for WindowInfo {}
unsafe impl Sync for WindowInfo {}

#[derive(Clone)]
pub(crate) struct WindowsLoopData {
    pub info: WindowInfo,
    pub tx: Sender<WindowsTrayEvent>,
}

pub(crate) struct WindowsTrayEvent(pub(crate) u32);

pub const NOT_BOUND: &'static str = "TrayNotification is not yet bound to a winapi object";
pub const BAD_HANDLE: &'static str = "INTERNAL ERROR: TrayNotification handle is not HWND!";

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
