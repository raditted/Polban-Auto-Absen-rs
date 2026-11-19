//! Platform detection and ASCII art banner matching `windows/tui.py`.

pub const VERSION: &str = "v3.0";
pub const COMPACT_TITLE: &str = ">>> POLBAN AUTO ABSEN <<<";

pub const ASCII_BANNER: &str = concat!(
    "    ___         __           ___    __\n",
    "   /   | __  __/ /_____     /   |  / /_  ________  ____\n",
    "  / /| |/ / / / __/ __ \\   / /| | / __ \\/ ___/ _ \\/ __ \\\n",
    " / ___ / /_/ / /_/ /_/ /  / ___ |/ /_/ (__  )  __/ / / /\n",
    "/_/  |_\\__,_/\\__/\\____/  /_/  |_/_.___/____/\\___/_/ /_/",
);

pub const BANNER_WIDTH: usize = 56;

/// Human-readable platform label for dashboard subtitle & footer.
pub fn platform_label() -> &'static str {
    if cfg!(target_os = "windows") {
        return "Windows";
    }
    if std::env::var_os("PREFIX")
        .and_then(|v| v.into_string().ok())
        .is_some_and(|p| p.contains("com.termux"))
    {
        return "Termux";
    }
    if cfg!(target_os = "macos") {
        return "macOS";
    }
    "Linux"
}

pub fn subtitle() -> String {
    format!("Polban Auto Absen -- {} {VERSION}", platform_label())
}

pub fn term_width(default: usize) -> usize {
    crossterm::terminal::size()
        .map(|(w, _)| w as usize)
        .unwrap_or(default)
}

fn center_text(text: &str, width: usize) -> String {
    let char_len = text.chars().count();
    if char_len >= width {
        return text.to_string();
    }
    let total_pad = width - char_len;
    let left = total_pad / 2;
    let right = total_pad - left;
    format!("{}{}{}", " ".repeat(left), text, " ".repeat(right))
}

/// Print ASCII banner (or compact title on narrow terminals) to stdout.
pub fn print_ascii_banner() {
    let cols = term_width(80);
    let sub = subtitle();
    if cols < BANNER_WIDTH + 2 {
        println!("{}", center_text(COMPACT_TITLE, cols));
        println!("{}", center_text(&sub, cols));
        println!();
        return;
    }
    let pad = " ".repeat((cols - BANNER_WIDTH) / 2);
    for line in ASCII_BANNER.lines() {
        println!("{pad}{line}");
    }
    println!("{}", center_text(&sub, cols));
    println!();
}

/// Best-effort console title and icon setup on Windows (`conhost.exe`).
///
/// Safe and non-failing: if no console window or embedded icon resource is present
/// (for example under headless test runners or Windows Terminal), this is a no-op.
#[cfg(windows)]
pub fn configure_console_window() {
    use std::ffi::c_void;

    const IMAGE_ICON: u32 = 1;
    const LR_DEFAULTSIZE: u32 = 0x0000_0040;
    const LR_SHARED: u32 = 0x0000_8000;
    const WM_SETICON: u32 = 0x0080;
    const ICON_SMALL: usize = 0;
    const ICON_BIG: usize = 1;
    const SMTO_ABORTIFHUNG: u32 = 0x0002;

    #[link(name = "kernel32")]
    extern "system" {
        fn GetConsoleWindow() -> *mut c_void;
        fn GetModuleHandleW(lp_module_name: *const u16) -> *mut c_void;
        fn SetConsoleTitleW(lp_console_title: *const u16) -> i32;
    }

    #[link(name = "user32")]
    extern "system" {
        fn LoadImageW(
            h_inst: *mut c_void,
            name: *const u16,
            r#type: u32,
            cx: i32,
            cy: i32,
            fu_load: u32,
        ) -> *mut c_void;
        fn SendMessageTimeoutW(
            h_wnd: *mut c_void,
            msg: u32,
            w_param: usize,
            l_param: isize,
            fu_flags: u32,
            u_timeout: u32,
            lpdw_result: *mut usize,
        ) -> isize;
    }

    let title: Vec<u16> = "Polban Auto Absen\0".encode_utf16().collect();
    unsafe {
        let _ = SetConsoleTitleW(title.as_ptr());
        let hwnd = GetConsoleWindow();
        if hwnd.is_null() {
            return;
        }
        let hinst = GetModuleHandleW(std::ptr::null());
        if hinst.is_null() {
            return;
        }
        // Resource ID 1 matches `winresource` (`1 ICON "../../assets/app-icon.ico"`).
        let hicon = LoadImageW(
            hinst,
            std::ptr::without_provenance::<u16>(1),
            IMAGE_ICON,
            0,
            0,
            LR_DEFAULTSIZE | LR_SHARED,
        );
        if hicon.is_null() {
            return;
        }
        let mut result: usize = 0;
        let _ = SendMessageTimeoutW(
            hwnd,
            WM_SETICON,
            ICON_SMALL,
            hicon as isize,
            SMTO_ABORTIFHUNG,
            200,
            &mut result,
        );
        let _ = SendMessageTimeoutW(
            hwnd,
            WM_SETICON,
            ICON_BIG,
            hicon as isize,
            SMTO_ABORTIFHUNG,
            200,
            &mut result,
        );
    }
}

#[cfg(not(windows))]
pub fn configure_console_window() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn banner_width_matches_longest_line() {
        let actual = ASCII_BANNER.lines().map(|l| l.len()).max().unwrap_or(0);
        assert_eq!(BANNER_WIDTH, actual);
    }

    #[test]
    fn subtitle_includes_platform_and_v3() {
        let sub = subtitle();
        assert!(sub.starts_with("Polban Auto Absen -- "));
        assert!(sub.ends_with(" v3.0"));
    }

    #[test]
    fn configure_console_window_is_safe_and_non_panicking() {
        configure_console_window();
    }
}
