//! Platform detection and ASCII art banner matching `windows/tui.py`.

pub const VERSION: &str = "v3.0";
pub const COMPACT_TITLE: &str = ">>> POLBAN AUTO ABSEN <<<";

pub const ASCII_BANNER: &str = "\
    ___         __           ___    __
   /   | __  __/ /_____     /   |  / /_  ________  ____
  / /| |/ / / / __/ __ \\   / /| | / __ \\/ ___/ _ \\/ __ \\
 / ___ / /_/ / /_/ /_/ /  / ___ |/ /_/ (__  )  __/ / / /
/_/  |_\\__,_/\\__/\\____/  /_/  |_/_.___/____/\\___/_/ /_/";

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
}
