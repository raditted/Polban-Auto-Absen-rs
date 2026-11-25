//! OS integration modules (system tray, desktop notifications, audio, launch at login).

pub mod autostart;
pub mod notify;
pub mod sound;
#[cfg(any(target_os = "macos", test))]
pub mod toast_macos;
pub mod toast_render;
#[cfg(target_os = "windows")]
pub mod toast_win32;
pub mod tray;
#[cfg(target_os = "windows")]
pub mod win32;

pub use autostart::AutostartManager;
pub use notify::Notifier;
pub use tray::{
    detect_window_backend, resolve_hide_to_tray_availability, supports_hide_to_tray,
    HideToTrayAvailability, TrayCommand, TrayManager, TrayWaker, WindowBackend,
};

const TILING_OR_BORDERLESS_WMS: &[&str] = &[
    "hyprland",
    "niri",
    "sway",
    "i3",
    "river",
    "bspwm",
    "dwm",
    "awesome",
    "qtile",
    "xmonad",
    "herbstluftwm",
    "leftwm",
    "spectrwm",
    "ratpoison",
    "stumpwm",
    "exwm",
];

const NATIVE_TITLEBAR_DESKTOPS: &[&str] = &[
    "gnome",
    "kde",
    "plasma",
    "plasmawayland",
    "xfce",
    "xfce4",
    "cinnamon",
    "mate",
    "lxqt",
    "lxde",
    "budgie",
    "pantheon",
    "deepin",
    "cosmic",
    "labwc",
    "wayfire",
];

/// Snapshot of environment variables inspected to decide whether in-app Linux
/// window control buttons (`Minimize`, `Maximize/Restore`, `Close`) are needed.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct WindowControlsEnv<'a> {
    pub is_linux: bool,
    pub polban_window_controls: Option<&'a str>,
    pub hyprland_instance_signature: Option<&'a str>,
    pub niri_socket: Option<&'a str>,
    pub swaysock: Option<&'a str>,
    pub i3sock: Option<&'a str>,
    pub xdg_current_desktop: Option<&'a str>,
    pub xdg_session_desktop: Option<&'a str>,
    pub desktop_session: Option<&'a str>,
}

fn token_matches(token: &str, candidate: &str) -> bool {
    if token == candidate {
        return true;
    }
    if let Some(rest) = token.strip_prefix(candidate) {
        return rest.starts_with('-') || rest.starts_with('_');
    }
    false
}

fn is_tiling_wm_token(token: &str) -> bool {
    TILING_OR_BORDERLESS_WMS
        .iter()
        .any(|&wm| token_matches(token, wm))
        || token == "i3wm"
        || token == "awesomewm"
}

fn is_native_desktop_token(token: &str) -> bool {
    NATIVE_TITLEBAR_DESKTOPS
        .iter()
        .any(|&de| token_matches(token, de))
}

/// Pure, unit-testable helper determining whether the GUI should render its
/// slim top window control bar based on OS and desktop session environment.
pub fn needs_window_controls_from_env(env: WindowControlsEnv<'_>) -> bool {
    if !env.is_linux {
        return false;
    }

    if let Some(raw) = env.polban_window_controls {
        match raw.trim().to_ascii_lowercase().as_str() {
            "1" | "true" | "always" | "on" => return true,
            "0" | "false" | "never" | "off" => return false,
            "" | "auto" => {}
            _ => {}
        }
    }

    let mut saw_tiling = false;
    let mut saw_native = false;
    for field in [
        env.xdg_current_desktop,
        env.xdg_session_desktop,
        env.desktop_session,
    ]
    .into_iter()
    .flatten()
    {
        for raw in field.split(|c: char| matches!(c, ':' | ';' | ',') || c.is_whitespace()) {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                continue;
            }
            let lower = trimmed.to_ascii_lowercase();
            let base = lower.rsplit('/').next().unwrap_or(&lower);
            let token = base.strip_suffix(".desktop").unwrap_or(base);
            if is_tiling_wm_token(token) {
                saw_tiling = true;
            }
            if is_native_desktop_token(token) {
                saw_native = true;
            }
        }
    }

    if saw_tiling {
        return true;
    }
    if saw_native {
        return false;
    }

    let non_empty = |v: Option<&str>| v.is_some_and(|s| !s.trim().is_empty());
    non_empty(env.hyprland_instance_signature)
        || non_empty(env.niri_socket)
        || non_empty(env.swaysock)
        || non_empty(env.i3sock)
}

/// Returns `true` when the active platform and desktop session lack native
/// titlebar buttons and need in-app window controls (Linux tiling/borderless WMs).
pub fn needs_window_controls() -> bool {
    #[cfg(not(target_os = "linux"))]
    {
        false
    }
    #[cfg(target_os = "linux")]
    {
        let get = |name: &str| std::env::var(name).ok();
        let polban_window_controls = get("POLBAN_WINDOW_CONTROLS");
        let hyprland_instance_signature = get("HYPRLAND_INSTANCE_SIGNATURE");
        let niri_socket = get("NIRI_SOCKET");
        let swaysock = get("SWAYSOCK");
        let i3sock = get("I3SOCK");
        let xdg_current_desktop = get("XDG_CURRENT_DESKTOP");
        let xdg_session_desktop = get("XDG_SESSION_DESKTOP");
        let desktop_session = get("DESKTOP_SESSION");

        needs_window_controls_from_env(WindowControlsEnv {
            is_linux: true,
            polban_window_controls: polban_window_controls.as_deref(),
            hyprland_instance_signature: hyprland_instance_signature.as_deref(),
            niri_socket: niri_socket.as_deref(),
            swaysock: swaysock.as_deref(),
            i3sock: i3sock.as_deref(),
            xdg_current_desktop: xdg_current_desktop.as_deref(),
            xdg_session_desktop: xdg_session_desktop.as_deref(),
            desktop_session: desktop_session.as_deref(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn non_linux_never_shows_in_app_window_controls() {
        assert!(!needs_window_controls_from_env(WindowControlsEnv {
            is_linux: false,
            polban_window_controls: Some("1"),
            hyprland_instance_signature: Some("sig"),
            xdg_current_desktop: Some("Hyprland"),
            ..Default::default()
        }));
    }

    #[test]
    fn explicit_override_controls_linux_behavior() {
        for truthy in ["1", "true", "TRUE", "always", "on", " On "] {
            assert!(
                needs_window_controls_from_env(WindowControlsEnv {
                    is_linux: true,
                    polban_window_controls: Some(truthy),
                    xdg_current_desktop: Some("GNOME"),
                    ..Default::default()
                }),
                "override {truthy:?} should force true"
            );
        }

        for falsy in ["0", "false", "FALSE", "never", "off", " Off "] {
            assert!(
                !needs_window_controls_from_env(WindowControlsEnv {
                    is_linux: true,
                    polban_window_controls: Some(falsy),
                    hyprland_instance_signature: Some("abc"),
                    xdg_current_desktop: Some("Hyprland"),
                    ..Default::default()
                }),
                "override {falsy:?} should force false"
            );
        }

        assert!(needs_window_controls_from_env(WindowControlsEnv {
            is_linux: true,
            polban_window_controls: Some("auto"),
            hyprland_instance_signature: Some("abc"),
            ..Default::default()
        }));
        assert!(!needs_window_controls_from_env(WindowControlsEnv {
            is_linux: true,
            polban_window_controls: Some("auto"),
            xdg_current_desktop: Some("GNOME"),
            ..Default::default()
        }));
    }

    #[test]
    fn tiling_compositor_sockets_and_session_tokens_detect_true() {
        // Compositor socket env vars.
        assert!(needs_window_controls_from_env(WindowControlsEnv {
            is_linux: true,
            hyprland_instance_signature: Some("11cbc6c5_1795"),
            ..Default::default()
        }));
        assert!(needs_window_controls_from_env(WindowControlsEnv {
            is_linux: true,
            niri_socket: Some("/run/user/1000/niri.wayland-1.sock"),
            ..Default::default()
        }));
        assert!(needs_window_controls_from_env(WindowControlsEnv {
            is_linux: true,
            swaysock: Some("/run/user/1000/sway-ipc.sock"),
            ..Default::default()
        }));
        assert!(needs_window_controls_from_env(WindowControlsEnv {
            is_linux: true,
            i3sock: Some("/run/user/1000/i3/ipc-socket"),
            ..Default::default()
        }));
        // Whitespace-only sockets must not trigger false positives.
        assert!(!needs_window_controls_from_env(WindowControlsEnv {
            is_linux: true,
            hyprland_instance_signature: Some("   "),
            niri_socket: Some(""),
            xdg_current_desktop: Some("GNOME"),
            ..Default::default()
        }));

        // All tiling/borderless WMs via desktop session tokens.
        for token in [
            "Hyprland",
            "hyprland-uwsm",
            "niri",
            "sway",
            "i3",
            "i3-with-shmlog",
            "river",
            "bspwm",
            "dwm",
            "awesome",
            "qtile",
            "xmonad",
            "herbstluftwm",
            "leftwm",
            "spectrwm",
            "ratpoison",
            "stumpwm",
            "exwm",
            "/usr/share/xsessions/bspwm.desktop",
            "wlroots:sway",
        ] {
            assert!(
                needs_window_controls_from_env(WindowControlsEnv {
                    is_linux: true,
                    xdg_current_desktop: Some(token),
                    ..Default::default()
                }),
                "XDG_CURRENT_DESKTOP={token:?} should return true"
            );
            assert!(
                needs_window_controls_from_env(WindowControlsEnv {
                    is_linux: true,
                    desktop_session: Some(token),
                    ..Default::default()
                }),
                "DESKTOP_SESSION={token:?} should return true"
            );
        }
    }

    #[test]
    fn native_titlebar_desktops_and_unknown_sessions_detect_false() {
        for de in [
            "GNOME",
            "ubuntu:GNOME",
            "KDE",
            "plasma",
            "plasmawayland",
            "XFCE",
            "X-Cinnamon",
            "Cinnamon",
            "MATE",
            "LXQt",
            "Budgie:GNOME",
            "Pantheon",
            "Deepin",
            "COSMIC",
            "labwc:wlroots",
            "wayfire",
        ] {
            assert!(
                !needs_window_controls_from_env(WindowControlsEnv {
                    is_linux: true,
                    xdg_current_desktop: Some(de),
                    xdg_session_desktop: Some(de),
                    ..Default::default()
                }),
                "native DE {de:?} must never show double titlebars"
            );
        }

        // Completely unset env defaults safely to false.
        assert!(!needs_window_controls_from_env(WindowControlsEnv {
            is_linux: true,
            ..Default::default()
        }));

        // Explicit native desktop session guards against stale compositor sockets.
        assert!(!needs_window_controls_from_env(WindowControlsEnv {
            is_linux: true,
            hyprland_instance_signature: Some("stale_sig"),
            xdg_current_desktop: Some("GNOME"),
            desktop_session: Some("gnome"),
            ..Default::default()
        }));

        // Hybrid XFCE + i3 session uses tiling WM controls.
        assert!(needs_window_controls_from_env(WindowControlsEnv {
            is_linux: true,
            xdg_current_desktop: Some("XFCE"),
            desktop_session: Some("i3"),
            ..Default::default()
        }));
    }
}
