//! Windows-specific HWND restore, message-only wakeup window, and clean hidden exit helpers.

use std::sync::Once;

use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    BringWindowToTop, CreateWindowExW, DefWindowProcW, DestroyWindow, IsIconic, IsWindow,
    IsWindowVisible, PostMessageW, RegisterClassW, SetForegroundWindow, ShowWindow, CREATESTRUCTW,
    GWLP_USERDATA, HWND_MESSAGE, SW_RESTORE, SW_SHOW, WM_APP, WM_CLOSE, WM_NCCREATE, WM_NCDESTROY,
    WNDCLASSW,
};

pub const WM_APP_TRAY_CMD: u32 = WM_APP + 0x41;
pub const WM_APP_SYNC_TRAY: u32 = WM_APP + 0x42;
pub const WM_APP_SHOW_WINDOW: u32 = WM_APP + 0x43;

const MSG_CLASS_NAME: &str = "PolbanPresensiMsgWindow";
static REGISTER_CLASS: Once = Once::new();

fn encode_wide(s: &str) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    std::ffi::OsStr::new(s)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

#[inline]
unsafe fn get_user_data(hwnd: HWND) -> isize {
    #[cfg(target_pointer_width = "64")]
    {
        windows_sys::Win32::UI::WindowsAndMessaging::GetWindowLongPtrW(hwnd, GWLP_USERDATA)
    }
    #[cfg(target_pointer_width = "32")]
    {
        windows_sys::Win32::UI::WindowsAndMessaging::GetWindowLongW(hwnd, GWLP_USERDATA) as isize
    }
}

#[inline]
unsafe fn set_user_data(hwnd: HWND, value: isize) -> isize {
    #[cfg(target_pointer_width = "64")]
    {
        windows_sys::Win32::UI::WindowsAndMessaging::SetWindowLongPtrW(hwnd, GWLP_USERDATA, value)
    }
    #[cfg(target_pointer_width = "32")]
    {
        windows_sys::Win32::UI::WindowsAndMessaging::SetWindowLongW(
            hwnd,
            GWLP_USERDATA,
            value as i32,
        ) as isize
    }
}

/// Extract the native Win32 `HWND` as `isize` from an `eframe::CreationContext`.
pub fn extract_hwnd(cc: &eframe::CreationContext<'_>) -> Option<isize> {
    let handle = cc.window_handle().ok()?;
    match handle.as_raw() {
        RawWindowHandle::Win32(win32) => Some(win32.hwnd.get()),
        _ => None,
    }
}

/// Returns `true` if `hwnd` refers to a valid, currently visible Win32 window.
pub fn is_window_visible(hwnd: isize) -> bool {
    if hwnd == 0 {
        return false;
    }
    unsafe {
        let raw = hwnd as HWND;
        IsWindow(raw) != 0 && IsWindowVisible(raw) != 0
    }
}

/// Immediately show, un-minimize if iconic, and bring `hwnd` to the foreground.
pub fn restore_and_focus_window(hwnd: isize) {
    if hwnd == 0 {
        return;
    }
    unsafe {
        let raw = hwnd as HWND;
        if IsWindow(raw) == 0 {
            return;
        }
        if IsIconic(raw) != 0 {
            ShowWindow(raw, SW_RESTORE);
        } else {
            ShowWindow(raw, SW_SHOW);
        }
        BringWindowToTop(raw);
        SetForegroundWindow(raw);
    }
}

/// Post a non-blocking message to `hwnd` (thread-safe).
pub fn post_app_message(hwnd: isize, msg: u32) {
    if hwnd == 0 {
        return;
    }
    unsafe {
        let raw = hwnd as HWND;
        if IsWindow(raw) != 0 {
            PostMessageW(raw, msg, 0, 0);
        }
    }
}

/// Post `WM_CLOSE` to a visible Win32 window.
pub fn post_close_window(hwnd: isize) {
    post_app_message(hwnd, WM_CLOSE);
}

/// Destroy `hwnd` directly on the UI thread (used to exit cleanly when hidden).
pub fn destroy_window(hwnd: isize) {
    if hwnd == 0 {
        return;
    }
    unsafe {
        let raw = hwnd as HWND;
        if IsWindow(raw) != 0 {
            DestroyWindow(raw);
        }
    }
}

type MsgCallback = Box<dyn FnMut(u32)>;

unsafe extern "system" fn msg_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_NCCREATE => {
            let cs = &*(lparam as *const CREATESTRUCTW);
            set_user_data(hwnd, cs.lpCreateParams as isize);
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
        WM_APP_TRAY_CMD | WM_APP_SYNC_TRAY | WM_APP_SHOW_WINDOW => {
            let ptr = get_user_data(hwnd) as *mut MsgCallback;
            if !ptr.is_null() {
                (*ptr)(msg);
            }
            0
        }
        WM_NCDESTROY => {
            let ptr = set_user_data(hwnd, 0) as *mut MsgCallback;
            if !ptr.is_null() {
                drop(Box::from_raw(ptr));
            }
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

/// Message-only Win32 window created on the UI thread so background threads and tray
/// callbacks can wake the UI thread even while the main `eframe` window is hidden.
pub struct MessageWindow {
    hwnd: isize,
}

impl MessageWindow {
    pub fn new(callback: impl FnMut(u32) + 'static) -> Result<Self, String> {
        let class_wide = encode_wide(MSG_CLASS_NAME);
        unsafe {
            let hinstance = GetModuleHandleW(std::ptr::null());
            REGISTER_CLASS.call_once(|| {
                let wc = WNDCLASSW {
                    lpfnWndProc: Some(msg_wnd_proc),
                    hInstance: hinstance,
                    lpszClassName: class_wide.as_ptr(),
                    ..std::mem::zeroed()
                };
                RegisterClassW(&wc);
            });

            let boxed: Box<MsgCallback> = Box::new(Box::new(callback));
            let raw_ctx = Box::into_raw(boxed);
            let hwnd = CreateWindowExW(
                0,
                class_wide.as_ptr(),
                std::ptr::null(),
                0,
                0,
                0,
                0,
                0,
                HWND_MESSAGE,
                std::ptr::null_mut(),
                hinstance,
                raw_ctx.cast(),
            );
            if hwnd.is_null() {
                drop(Box::from_raw(raw_ctx));
                return Err(format!(
                    "Gagal membuat jendela pesan Win32: {}",
                    std::io::Error::last_os_error()
                ));
            }
            Ok(Self {
                hwnd: hwnd as isize,
            })
        }
    }

    pub fn hwnd(&self) -> isize {
        self.hwnd
    }
}

impl Drop for MessageWindow {
    fn drop(&mut self) {
        if self.hwnd != 0 {
            destroy_window(self.hwnd);
            self.hwnd = 0;
        }
    }
}
