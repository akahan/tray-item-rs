// Most of this code is taken from https://github.com/qdot/systray-rs/blob/master/src/api/win32/mod.rs

mod funcs;
mod structs;

use std::{
    cell::RefCell,
    mem,
    sync::{
        mpsc::{channel, Sender},
        Arc, Mutex,
    },
    thread,
};

use windows_sys::Win32::{
    Foundation::{LPARAM, WPARAM},
    UI::{
        Shell::{
            Shell_NotifyIconW, NIF_ICON, NIF_INFO, NIF_TIP, NIM_DELETE, NIM_MODIFY, NOTIFYICONDATAW,
        },
        WindowsAndMessaging::{
            InsertMenuItemW, LoadImageW, PostMessageW, SetMenuItemInfoW, HICON, IMAGE_ICON,
            LOADIMAGE_HANDLE, LR_DEFAULTCOLOR, MENUITEMINFOW, MFS_DISABLED, MFS_UNHILITE,
            MFT_SEPARATOR, MFT_STRING, MIIM_FTYPE, MIIM_ID, MIIM_STATE, MIIM_STRING, WM_DESTROY,
        },
    },
};

use crate::{IconSource, TIError, TrayNotificationFlags};

use funcs::*;
use structs::*;

thread_local!(static WININFO_STASH: RefCell<Option<WindowsLoopData>> = RefCell::new(None));

type CallBackEntry = Option<Box<dyn Fn() + Send + 'static>>;

pub struct TrayItemWindows {
    entries: Arc<Mutex<Vec<CallBackEntry>>>,
    info: WindowInfo,
    windows_loop: Option<thread::JoinHandle<()>>,
    event_loop: Option<thread::JoinHandle<()>>,
    event_tx: Sender<WindowsTrayEvent>,
}

impl TrayItemWindows {
    pub fn new(title: &str, icon: IconSource) -> Result<Self, TIError> {
        let entries = Arc::new(Mutex::new(Vec::new()));
        let (event_tx, event_rx) = channel::<WindowsTrayEvent>();

        let entries_clone = Arc::clone(&entries);
        let event_loop = thread::spawn(move || loop {
            if let Ok(v) = event_rx.recv() {
                if v.0 == u32::MAX {
                    break;
                }

                padlock::mutex_lock(&entries_clone, |ents: &mut Vec<CallBackEntry>| match &ents
                    [v.0 as usize]
                {
                    Some(f) => f(),
                    None => (),
                })
            }
        });

        let (tx, rx) = channel();

        let event_tx_clone = event_tx.clone();
        let windows_loop = thread::spawn(move || unsafe {
            let info = match init_window() {
                Ok(info) => {
                    tx.send(Ok(info.clone())).ok();
                    info
                }

                Err(e) => {
                    tx.send(Err(e)).ok();
                    return;
                }
            };

            WININFO_STASH.with(|stash| {
                let data = WindowsLoopData {
                    info,
                    tx: event_tx_clone,
                };

                (*stash.borrow_mut()) = Some(data);
            });

            run_loop();
        });

        let info = match rx.recv().unwrap() {
            Ok(i) => i,
            Err(e) => return Err(e),
        };

        let w = Self {
            entries,
            info,
            windows_loop: Some(windows_loop),
            event_loop: Some(event_loop),
            event_tx,
        };

        w.set_tooltip(title)?;
        w.set_icon(icon)?;

        Ok(w)
    }

    pub fn set_icon(&self, icon: IconSource) -> Result<(), TIError> {
        self.set_icon_from_resource(icon.as_str())
    }

    pub fn add_label(&mut self, label: &str) -> Result<(), TIError> {
        self.add_label_with_id(label)?;
        Ok(())
    }

    pub fn add_label_with_id(&mut self, label: &str) -> Result<u32, TIError> {
        let item_idx = padlock::mutex_lock(&self.entries, |entries| {
            let len = entries.len();
            entries.push(None);
            len
        }) as u32;

        let mut st = to_wstring(label);
        let mut item = unsafe { mem::zeroed::<MENUITEMINFOW>() };
        item.cbSize = mem::size_of::<MENUITEMINFOW>() as u32;
        item.fMask = MIIM_FTYPE | MIIM_STRING | MIIM_ID | MIIM_STATE;
        item.fType = MFT_STRING;
        item.fState = MFS_DISABLED | MFS_UNHILITE;
        item.wID = item_idx;
        item.dwTypeData = st.as_mut_ptr();
        item.cch = (label.len() * 2) as u32;

        unsafe {
            if InsertMenuItemW(self.info.hmenu, item_idx, 1, &item) == 0 {
                return Err(get_win_os_error("Error inserting menu item"));
            }
        }
        Ok(item_idx)
    }

    pub fn set_label(&mut self, label: &str, id: u32) -> Result<(), TIError> {
        let mut st = to_wstring(label);
        let mut item = unsafe { mem::zeroed::<MENUITEMINFOW>() };
        item.cbSize = mem::size_of::<MENUITEMINFOW>() as u32;
        item.fMask = MIIM_FTYPE | MIIM_STRING | MIIM_ID | MIIM_STATE;
        item.fType = MFT_STRING;
        item.fState = MFS_DISABLED | MFS_UNHILITE;
        item.wID = id;
        item.dwTypeData = st.as_mut_ptr();
        item.cch = (label.len() * 2) as u32;

        unsafe {
            if SetMenuItemInfoW(self.info.hmenu, id, 1, &item) == 0 {
                return Err(get_win_os_error("Error inserting menu item"));
            }
        }
        Ok(())
    }

    pub fn add_menu_item<F>(&mut self, label: &str, cb: F) -> Result<(), TIError>
    where
        F: Fn() + Send + 'static,
    {
        self.add_menu_item_with_id(label, cb)?;
        Ok(())
    }

    pub fn add_menu_item_with_id<F>(&mut self, label: &str, cb: F) -> Result<u32, TIError>
    where
        F: Fn() + Send + 'static,
    {
        let item_idx = padlock::mutex_lock(&self.entries, |entries| {
            let len = entries.len();
            entries.push(Some(Box::new(cb)));
            len
        }) as u32;

        let mut st = to_wstring(label);
        let mut item = unsafe { mem::zeroed::<MENUITEMINFOW>() };
        item.cbSize = mem::size_of::<MENUITEMINFOW>() as u32;
        item.fMask = MIIM_FTYPE | MIIM_STRING | MIIM_ID | MIIM_STATE;
        item.fType = MFT_STRING;
        item.wID = item_idx;
        item.dwTypeData = st.as_mut_ptr();
        item.cch = (label.len() * 2) as u32;

        unsafe {
            if InsertMenuItemW(self.info.hmenu, item_idx, 1, &item) == 0 {
                return Err(get_win_os_error("Error inserting menu item"));
            }
        }
        Ok(item_idx)
    }

    pub fn set_menu_item_label(&mut self, label: &str, id: u32) -> Result<(), TIError> {
        let mut st = to_wstring(label);
        let mut item = unsafe { mem::zeroed::<MENUITEMINFOW>() };
        item.cbSize = mem::size_of::<MENUITEMINFOW>() as u32;
        item.fMask = MIIM_FTYPE | MIIM_STRING | MIIM_ID | MIIM_STATE;
        item.fType = MFT_STRING;
        item.wID = id;
        item.dwTypeData = st.as_mut_ptr();
        item.cch = (label.len() * 2) as u32;

        unsafe {
            if SetMenuItemInfoW(self.info.hmenu, id, 1, &item) == 0 {
                return Err(get_win_os_error("Error setting menu item"));
            }
        }
        Ok(())
    }

    pub fn add_separator(&mut self) -> Result<(), TIError> {
        self.add_separator_with_id()?;
        Ok(())
    }

    pub fn add_separator_with_id(&mut self) -> Result<u32, TIError> {
        let item_idx = padlock::mutex_lock(&self.entries, |entries| {
            let len = entries.len();
            entries.push(None);
            len
        }) as u32;

        let mut item = unsafe { mem::zeroed::<MENUITEMINFOW>() };
        item.cbSize = mem::size_of::<MENUITEMINFOW>() as u32;
        item.fMask = MIIM_FTYPE | MIIM_ID | MIIM_STATE;
        item.fType = MFT_SEPARATOR;
        item.wID = item_idx;

        unsafe {
            if InsertMenuItemW(self.info.hmenu, item_idx, 1, &item) == 0 {
                return Err(get_win_os_error("Error inserting menu separator"));
            }
        }
        Ok(item_idx)
    }

    pub fn set_tooltip(&self, tooltip: &str) -> Result<(), TIError> {
        let mut wide_tooltip = to_wstring(tooltip);
        if wide_tooltip.len() > 128 {
            return Err(TIError::new("The tooltip may not exceed 127 wide bytes"));
        }

        let mut nid = unsafe { mem::zeroed::<NOTIFYICONDATAW>() };
        nid.cbSize = mem::size_of::<NOTIFYICONDATAW>() as u32;
        nid.hWnd = self.info.hwnd;
        nid.uID = 1;
        nid.uFlags = NIF_TIP;

        wide_tooltip.extend(vec![0; 128 - wide_tooltip.len()]);
        let mut tooltip_array = [0u16; 128];
        tooltip_array.copy_from_slice(&wide_tooltip[..]);
        nid.szTip = tooltip_array;

        unsafe {
            if Shell_NotifyIconW(NIM_MODIFY, &nid) == 0 {
                return Err(get_win_os_error("Error setting tooltip"));
            }
        }
        Ok(())
    }

    pub fn show_toast(
        &self,
        text: &str,
        icon: IconSource,
        title: Option<&str>,
        flags: Option<TrayNotificationFlags>,
    ) -> Result<(), TIError> {
        // if self.handle.blank() { panic!("{}", NOT_BOUND); }
        // self.handle.tray().expect(BAD_HANDLE);

        let default_flags = TrayNotificationFlags::NO_ICON | TrayNotificationFlags::SILENT;

        unsafe {
            let mut data = self.notify_default();
            data.uFlags = NIF_INFO;
            data.dwInfoFlags = flags.unwrap_or(default_flags).bits();
            let icon = self._icon_from_resource(icon.as_str())?;
            data.hBalloonIcon = icon;

            let info_v = to_wstring(text);
            let length = if info_v.len() >= 256 {
                255
            } else {
                info_v.len()
            };
            for i in 0..length {
                data.szInfo[i] = info_v[i];
            }

            let info_title_v = match title {
                Some(t) => to_wstring(t),
                None => vec![],
            };

            let length = if info_title_v.len() >= 256 {
                255
            } else {
                info_title_v.len()
            };
            for i in 0..length {
                data.szInfoTitle[i] = info_title_v[i];
            }

            Shell_NotifyIconW(NIM_MODIFY, &mut data);
        }

        Ok(())
    }

    fn notify_default(&self) -> NOTIFYICONDATAW {
        unsafe {
            let parent = self.info.hwnd;
            NOTIFYICONDATAW {
                cbSize: mem::size_of::<NOTIFYICONDATAW>() as u32,
                hWnd: parent,
                uID: 0,
                uFlags: 0,
                uCallbackMessage: 0,
                hIcon: 0,
                szTip: mem::zeroed(),
                dwState: 0,
                dwStateMask: 0,
                szInfo: mem::zeroed(),
                szInfoTitle: mem::zeroed(),
                dwInfoFlags: 0,
                guidItem: mem::zeroed(),
                hBalloonIcon: 0,
                Anonymous: mem::zeroed(),
            }
        }
    }

    fn set_icon_from_resource(&self, resource_name: &str) -> Result<(), TIError> {
        let icon = self._icon_from_resource(resource_name)?;
        self._set_icon(icon)
    }

    fn _icon_from_resource(&self, resource_name: &str) -> Result<LOADIMAGE_HANDLE, TIError> {
        unsafe {
            let handle = LoadImageW(
                self.info.hmodule,
                to_wstring(resource_name).as_ptr(),
                IMAGE_ICON,
                64,
                64,
                LR_DEFAULTCOLOR,
            );

            if handle == 0 {
                return Err(get_win_os_error("Error setting icon from resource"));
            }

            Ok(handle)
        }
    }

    fn _set_icon(&self, icon: HICON) -> Result<(), TIError> {
        let mut nid = unsafe { mem::zeroed::<NOTIFYICONDATAW>() };
        nid.cbSize = mem::size_of::<NOTIFYICONDATAW>() as u32;
        nid.hWnd = self.info.hwnd;
        nid.uID = 1;
        nid.uFlags = NIF_ICON;
        nid.hIcon = icon;

        unsafe {
            if Shell_NotifyIconW(NIM_MODIFY, &nid) == 0 {
                return Err(get_win_os_error("Error setting icon"));
            }
        }
        Ok(())
    }

    pub fn quit(&mut self) {
        unsafe {
            PostMessageW(self.info.hwnd, WM_DESTROY, 0, 0);
        }

        if let Some(t) = self.windows_loop.take() {
            t.join().ok();
        }

        if let Some(t) = self.event_loop.take() {
            self.event_tx.send(WindowsTrayEvent(u32::MAX)).ok();
            t.join().ok();
        }
    }

    pub fn shutdown(&self) -> Result<(), TIError> {
        let mut nid = unsafe { mem::zeroed::<NOTIFYICONDATAW>() };
        nid.cbSize = mem::size_of::<NOTIFYICONDATAW>() as u32;
        nid.hWnd = self.info.hwnd;
        nid.uID = 1;
        nid.uFlags = NIF_ICON;

        unsafe {
            if Shell_NotifyIconW(NIM_DELETE, &nid) == 0 {
                return Err(get_win_os_error("Error deleting icon from menu"));
            }
        }

        Ok(())
    }
}

impl Drop for TrayItemWindows {
    fn drop(&mut self) {
        self.shutdown().ok();
        self.quit();
    }
}
