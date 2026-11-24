//! Native Windows non-activating layered corner toast stack with Minecraft-style horizontal slide.
//!
//! Renders crisp pixel-art toast sprites via `UpdateLayeredWindow` on a persistent
//! background Win32 thread (`polban-toast-ui`), anchored flush to the right work-area edge
//! and clipped cleanly at the monitor boundary so toasts never steal focus or bleed onto
//! adjacent monitors.

use std::cell::RefCell;
use std::sync::atomic::{AtomicIsize, Ordering};
use std::sync::{mpsc, Arc, OnceLock};
use std::time::Instant;

use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, SIZE, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{
    CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, GetDC, ReleaseDC, SelectObject,
    AC_SRC_ALPHA, AC_SRC_OVER, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, BLENDFUNCTION, DIB_RGB_COLORS,
    HBITMAP, HDC, HGDIOBJ,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::HiDpi::GetDpiForSystem;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, FindWindowW, GetCursorPos,
    GetMessageW, GetSystemMetrics, GetWindowRect, IsWindowVisible, KillTimer, LoadCursorW,
    PostMessageW, RegisterClassW, SetTimer, ShowWindow, SystemParametersInfoW, TranslateMessage,
    UpdateLayeredWindow, HWND_MESSAGE, IDC_ARROW, MA_NOACTIVATE, MSG, SM_CYSCREEN, SPI_GETWORKAREA,
    SW_SHOWNOACTIVATE, ULW_ALPHA, WM_APP, WM_LBUTTONUP, WM_MOUSEACTIVATE, WM_RBUTTONUP,
    WM_SETCURSOR, WM_TIMER, WNDCLASSW, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
    WS_EX_TOPMOST, WS_POPUP,
};

use super::toast_render::{
    point_in_rect, render_toast_card, rgba_to_premultiplied_bgra, toast_pixel_scale, RenderedToast,
    ToastCardSpec, ToastKind,
};
use crate::theme::Palette;

const TOAST_CLASS_NAME: &str = "PolbanPresensiToastWindow";
const DISPATCHER_CLASS_NAME: &str = "PolbanPresensiToastDispatcher";
const WM_APP_NEW_TOAST: u32 = WM_APP + 0x61;
const TIMER_ID: usize = 1;

const MAX_VISIBLE_TOASTS: usize = 3;
const TASKBAR_RESERVED_DIP: f32 = 56.0;
const BOTTOM_MARGIN_DIP: f32 = 16.0;
const OVERFLOW_MARGIN_DIP: f32 = 12.0;

const SLIDE_IN_MS: f32 = 320.0;
const HOLD_MS: f32 = 4500.0;
const SLIDE_OUT_MS: f32 = 320.0;

const TIMER_ANIM_MS: u32 = 16;
const TIMER_HOLD_MS: u32 = 50;

/// Payload submitted to the background Win32 toast worker thread.
#[derive(Debug, Clone)]
pub struct ToastRequest {
    pub kind: ToastKind,
    pub title: String,
    pub body: String,
    pub palette: Palette,
    pub wake_hwnd: isize,
}

struct DispatcherHandle {
    tx: mpsc::Sender<ToastRequest>,
    wake_hwnd: Arc<AtomicIsize>,
}

static DISPATCHER: OnceLock<DispatcherHandle> = OnceLock::new();

/// Enqueue a custom Polban Presensi Minecraft-style corner toast without blocking the caller.
pub fn show_toast(req: ToastRequest) {
    let handle = DISPATCHER.get_or_init(|| {
        let (tx, rx) = mpsc::channel::<ToastRequest>();
        let wake_hwnd = Arc::new(AtomicIsize::new(0));
        let worker_wake = Arc::clone(&wake_hwnd);
        let _ = std::thread::Builder::new()
            .name("polban-toast-ui".to_string())
            .spawn(move || {
                worker_thread_main(rx, worker_wake);
            });
        DispatcherHandle { tx, wake_hwnd }
    });

    if handle.tx.send(req).is_ok() {
        let hwnd = handle.wake_hwnd.load(Ordering::Acquire);
        if hwnd != 0 {
            unsafe {
                PostMessageW(hwnd as HWND, WM_APP_NEW_TOAST, 0, 0);
            }
        }
    }
}

fn encode_wide(s: &str) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    std::ffi::OsStr::new(s)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

fn system_scale() -> f32 {
    let dpi = unsafe { GetDpiForSystem() };
    let raw = if dpi == 0 { 96.0 } else { dpi as f32 };
    (raw / 96.0).clamp(1.0, 3.0)
}

fn work_area_rect() -> RECT {
    let mut rect = RECT {
        left: 0,
        top: 0,
        right: 1920,
        bottom: 1040,
    };
    unsafe {
        SystemParametersInfoW(SPI_GETWORKAREA, 0, (&mut rect as *mut RECT).cast(), 0);
    }
    rect
}

/// Compute the lowest Y coordinate allowed for the bottom edge of the toast stack so toasts
/// clear both docked/auto-hide Windows taskbars and any open bottom-right tray overflow flyout.
fn compute_safe_bottom_limit(
    work_bottom: i32,
    screen_h: i32,
    scale: f32,
    overflow_top: Option<i32>,
) -> i32 {
    let safe_scale = if scale.is_finite() {
        scale.clamp(1.0, 3.0)
    } else {
        1.0
    };
    let taskbar_reserved = (TASKBAR_RESERVED_DIP * safe_scale).round() as i32;
    let margin = (BOTTOM_MARGIN_DIP * safe_scale).round() as i32;
    let auto_hide_threshold = (8.0 * safe_scale).round() as i32;
    let taskbar_top = if screen_h > 0 && work_bottom >= screen_h - auto_hide_threshold {
        work_bottom.min(screen_h - taskbar_reserved)
    } else {
        work_bottom
    };
    let mut limit = taskbar_top - margin;
    if let Some(ov_top) = overflow_top {
        limit = limit.min(ov_top - (OVERFLOW_MARGIN_DIP * safe_scale).round() as i32);
    }
    limit.max(120)
}

/// Detect the top Y coordinate of a visible Windows 10/11 system tray overflow flyout
/// positioned near the bottom-right work-area corner.
fn detect_tray_overflow_top(work_right: i32, screen_h: i32, scale: f32) -> Option<i32> {
    let classes = [
        "TopLevelWindowForOverflowXamlIsland",
        "NotifyIconOverflowWindow",
    ];
    let safe_scale = if scale.is_finite() {
        scale.clamp(1.0, 3.0)
    } else {
        1.0
    };
    let right_zone_left = work_right - (520.0 * safe_scale) as i32;
    let mut best_top: Option<i32> = None;

    for class_name in classes {
        let wide = encode_wide(class_name);
        unsafe {
            let hwnd = FindWindowW(wide.as_ptr(), std::ptr::null());
            if hwnd.is_null() || IsWindowVisible(hwnd) == 0 {
                continue;
            }
            let mut rect = RECT {
                left: 0,
                top: 0,
                right: 0,
                bottom: 0,
            };
            if GetWindowRect(hwnd, &mut rect) == 0 {
                continue;
            }
            if rect.bottom <= rect.top || rect.right <= rect.left {
                continue;
            }
            if rect.top > screen_h / 3 && rect.right > right_zone_left {
                best_top = Some(best_top.map_or(rect.top, |cur| cur.min(rect.top)));
            }
        }
    }

    best_top
}

struct LayeredSurface {
    mem_dc: HDC,
    bitmap: HBITMAP,
    old_obj: HGDIOBJ,
    width: i32,
    height: i32,
}

impl LayeredSurface {
    fn from_premultiplied_bgra(width: u32, height: u32, bgra: &[u8]) -> Option<Self> {
        let w = width as i32;
        let h = height as i32;
        if w <= 0 || h <= 0 || bgra.len() < (w as usize) * (h as usize) * 4 {
            return None;
        }
        unsafe {
            let screen_dc = GetDC(std::ptr::null_mut());
            let mem_dc = CreateCompatibleDC(screen_dc);
            if mem_dc.is_null() {
                if !screen_dc.is_null() {
                    ReleaseDC(std::ptr::null_mut(), screen_dc);
                }
                return None;
            }

            let bmi = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: w,
                    biHeight: -h, // top-down DIB
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB,
                    biSizeImage: 0,
                    biXPelsPerMeter: 0,
                    biYPelsPerMeter: 0,
                    biClrUsed: 0,
                    biClrImportant: 0,
                },
                bmiColors: [std::mem::zeroed()],
            };

            let mut bits: *mut core::ffi::c_void = std::ptr::null_mut();
            let bitmap = CreateDIBSection(
                mem_dc,
                &bmi,
                DIB_RGB_COLORS,
                &mut bits,
                std::ptr::null_mut(),
                0,
            );
            if !screen_dc.is_null() {
                ReleaseDC(std::ptr::null_mut(), screen_dc);
            }
            if bitmap.is_null() || bits.is_null() {
                if !bitmap.is_null() {
                    DeleteObject(bitmap.cast());
                }
                DeleteDC(mem_dc);
                return None;
            }

            std::ptr::copy_nonoverlapping(bgra.as_ptr(), bits.cast::<u8>(), (w * h * 4) as usize);
            let old_obj = SelectObject(mem_dc, bitmap.cast());
            Some(Self {
                mem_dc,
                bitmap,
                old_obj,
                width: w,
                height: h,
            })
        }
    }

    fn present_clipped(&self, hwnd: HWND, x: i32, y: i32, visible_w: i32) {
        let cx = visible_w.clamp(1, self.width);
        unsafe {
            let screen_dc = GetDC(std::ptr::null_mut());
            let dst_pt = POINT { x, y };
            let src_pt = POINT { x: 0, y: 0 };
            let size = SIZE {
                cx,
                cy: self.height,
            };
            let blend = BLENDFUNCTION {
                BlendOp: AC_SRC_OVER as u8,
                BlendFlags: 0,
                SourceConstantAlpha: 255,
                AlphaFormat: AC_SRC_ALPHA as u8,
            };
            UpdateLayeredWindow(
                hwnd,
                screen_dc,
                &dst_pt,
                &size,
                self.mem_dc,
                &src_pt,
                0,
                &blend,
                ULW_ALPHA,
            );
            if !screen_dc.is_null() {
                ReleaseDC(std::ptr::null_mut(), screen_dc);
            }
        }
    }
}

impl Drop for LayeredSurface {
    fn drop(&mut self) {
        unsafe {
            if !self.mem_dc.is_null() {
                if !self.old_obj.is_null() {
                    SelectObject(self.mem_dc, self.old_obj);
                }
                if !self.bitmap.is_null() {
                    DeleteObject(self.bitmap.cast());
                }
                DeleteDC(self.mem_dc);
            }
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum ToastPhase {
    SlideIn {
        started: Instant,
    },
    Hold {
        elapsed_ms: f32,
        last_tick: Instant,
    },
    SlideOut {
        started: Instant,
        start_fraction: f32,
    },
}

struct ActiveToast {
    hwnd: HWND,
    scale: f32,
    pixel_scale: u32,
    wake_hwnd: isize,
    rendered: RenderedToast,
    surface: LayeredSurface,
    phase: ToastPhase,
    work_right: i32,
    target_y: i32,
    cur_x: i32,
    cur_y: f32,
    cur_visible_w: i32,
    cur_fraction: f32,
}

impl ActiveToast {
    fn full_w(&self) -> i32 {
        self.rendered.width as i32
    }

    fn full_h(&self) -> i32 {
        self.rendered.height as i32
    }

    fn present_now(&self) {
        self.surface.present_clipped(
            self.hwnd,
            self.cur_x,
            self.cur_y.round() as i32,
            self.cur_visible_w,
        );
    }

    fn is_sliding_out(&self) -> bool {
        matches!(self.phase, ToastPhase::SlideOut { .. })
    }

    fn begin_slide_out(&mut self, now: Instant) {
        if !self.is_sliding_out() {
            self.phase = ToastPhase::SlideOut {
                started: now,
                start_fraction: self.cur_fraction.clamp(0.05, 1.0),
            };
        }
    }
}

struct ToastManager {
    dispatcher_hwnd: HWND,
    rx: Option<mpsc::Receiver<ToastRequest>>,
    toasts: Vec<ActiveToast>,
    timer_interval_ms: Option<u32>,
}

impl ToastManager {
    fn new() -> Self {
        Self {
            dispatcher_hwnd: std::ptr::null_mut(),
            rx: None,
            toasts: Vec::new(),
            timer_interval_ms: None,
        }
    }

    fn recompute_targets(&mut self) {
        if self.toasts.is_empty() {
            return;
        }
        let work = work_area_rect();
        let screen_h = unsafe { GetSystemMetrics(SM_CYSCREEN) };
        let base_scale = self.toasts.last().map_or_else(system_scale, |t| t.scale);
        let overflow_top = detect_tray_overflow_top(work.right, screen_h, base_scale);
        let safe_bottom =
            compute_safe_bottom_limit(work.bottom, screen_h, base_scale, overflow_top);

        // Anchor flush to work.right above the taskbar & tray overflow flyout, stacking upward.
        let mut cursor_bottom = safe_bottom;
        for toast in self.toasts.iter_mut().rev() {
            let full_h = toast.full_h();
            let stack_gap = (4 * toast.pixel_scale) as i32;

            let target_y = cursor_bottom - full_h;

            toast.work_right = work.right;
            toast.target_y = target_y;
            cursor_bottom = target_y - stack_gap;
        }
    }

    fn desired_timer_interval(&self) -> Option<u32> {
        if self.toasts.is_empty() {
            return None;
        }
        let needs_fast = self.toasts.iter().any(|t| {
            matches!(
                t.phase,
                ToastPhase::SlideIn { .. } | ToastPhase::SlideOut { .. }
            ) || (t.cur_y - t.target_y as f32).abs() > 0.5
        });
        if needs_fast {
            Some(TIMER_ANIM_MS)
        } else {
            Some(TIMER_HOLD_MS)
        }
    }

    fn update_timer_plan(&mut self) -> Option<Option<u32>> {
        let desired = self.desired_timer_interval();
        if desired != self.timer_interval_ms {
            self.timer_interval_ms = desired;
            Some(desired)
        } else {
            None
        }
    }
}

thread_local! {
    static MANAGER: RefCell<ToastManager> = RefCell::new(ToastManager::new());
}

fn apply_timer_plan(dispatcher_hwnd: HWND, plan: Option<Option<u32>>) {
    let Some(target) = plan else {
        return;
    };
    if dispatcher_hwnd.is_null() {
        return;
    }
    unsafe {
        match target {
            Some(ms) => {
                SetTimer(dispatcher_hwnd, TIMER_ID, ms, None);
            }
            None => {
                KillTimer(dispatcher_hwnd, TIMER_ID);
            }
        }
    }
}

fn spawn_toast_window(req: ToastRequest) {
    let scale = system_scale();
    let pixel_scale = toast_pixel_scale(scale);
    let spec = ToastCardSpec {
        kind: req.kind,
        title: req.title,
        body: req.body,
        progress: 1.0,
        close_hovered: false,
        palette: req.palette,
    };
    let rendered = render_toast_card(&spec, scale);
    let bgra = rgba_to_premultiplied_bgra(&rendered.rgba);
    let Some(surface) =
        LayeredSurface::from_premultiplied_bgra(rendered.width, rendered.height, &bgra)
    else {
        return;
    };

    let class_wide = encode_wide(TOAST_CLASS_NAME);
    let hwnd = unsafe {
        let hinstance = GetModuleHandleW(std::ptr::null());
        CreateWindowExW(
            WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            class_wide.as_ptr(),
            std::ptr::null(),
            WS_POPUP,
            0,
            0,
            rendered.width as i32,
            rendered.height as i32,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            hinstance,
            std::ptr::null(),
        )
    };
    if hwnd.is_null() {
        return;
    }

    let now = Instant::now();
    let toast = ActiveToast {
        hwnd,
        scale,
        pixel_scale,
        wake_hwnd: req.wake_hwnd,
        rendered,
        surface,
        phase: ToastPhase::SlideIn { started: now },
        work_right: 1920,
        target_y: 0,
        cur_x: 1919,
        cur_y: 0.0,
        cur_visible_w: 1,
        cur_fraction: 0.0,
    };

    let (disp_hwnd, timer_plan) = MANAGER.with(|m| {
        let mut mgr = m.borrow_mut();

        // Slide out oldest live toast early if already at capacity (3).
        let mut live_count = mgr.toasts.iter().filter(|t| !t.is_sliding_out()).count();
        for existing in &mut mgr.toasts {
            if live_count < MAX_VISIBLE_TOASTS {
                break;
            }
            if !existing.is_sliding_out() {
                existing.begin_slide_out(now);
                live_count = live_count.saturating_sub(1);
            }
        }

        mgr.toasts.push(toast);
        mgr.recompute_targets();

        if let Some(newest) = mgr.toasts.last_mut() {
            newest.cur_visible_w = 1;
            newest.cur_x = newest.work_right - 1;
            newest.cur_y = newest.target_y as f32;
            newest.cur_fraction = 0.0;
            newest.present_now();
        }

        let plan = mgr.update_timer_plan();
        (mgr.dispatcher_hwnd, plan)
    });

    unsafe {
        ShowWindow(hwnd, SW_SHOWNOACTIVATE);
    }
    apply_timer_plan(disp_hwnd, timer_plan);
}

fn drain_incoming_requests() {
    let mut pending = Vec::new();
    MANAGER.with(|m| {
        let mgr = m.borrow();
        if let Some(rx) = &mgr.rx {
            while let Ok(req) = rx.try_recv() {
                pending.push(req);
            }
        }
    });
    for req in pending {
        spawn_toast_window(req);
    }
}

fn tick_toasts() {
    let mut cursor = POINT {
        x: i32::MIN,
        y: i32::MIN,
    };
    let has_cursor = unsafe { GetCursorPos(&mut cursor) != 0 };
    let now = Instant::now();

    let mut to_destroy: Vec<HWND> = Vec::new();
    let (disp_hwnd, timer_plan) = MANAGER.with(|m| {
        let mut mgr = m.borrow_mut();
        mgr.recompute_targets();

        for toast in &mut mgr.toasts {
            // Smoothly glide vertically toward stacked target_y when another toast enters/leaves.
            let dy = toast.target_y as f32 - toast.cur_y;
            let moved_y = dy.abs() > 0.5;
            if dy.abs() <= 0.6 {
                toast.cur_y = toast.target_y as f32;
            } else {
                toast.cur_y += dy * 0.28;
            }

            let full_w = toast.full_w();
            let mut needs_present = moved_y;
            let mut should_remove = false;

            match toast.phase {
                ToastPhase::SlideIn { started } => {
                    let elapsed = now.saturating_duration_since(started).as_secs_f32() * 1000.0;
                    let t = (elapsed / SLIDE_IN_MS).clamp(0.0, 1.0);
                    let ease_t = 1.0 - (1.0 - t).powi(3);
                    toast.cur_fraction = ease_t;
                    toast.cur_visible_w =
                        ((full_w as f32) * ease_t).round().clamp(1.0, full_w as f32) as i32;
                    toast.cur_x = toast.work_right - toast.cur_visible_w;
                    needs_present = true;
                    if t >= 1.0 {
                        toast.cur_fraction = 1.0;
                        toast.cur_visible_w = full_w;
                        toast.cur_x = toast.work_right - full_w;
                        toast.phase = ToastPhase::Hold {
                            elapsed_ms: 0.0,
                            last_tick: now,
                        };
                    }
                }
                ToastPhase::Hold {
                    mut elapsed_ms,
                    last_tick,
                } => {
                    let dt_ms = now.saturating_duration_since(last_tick).as_secs_f32() * 1000.0;
                    let hovered = if has_cursor {
                        let visible_rect = [
                            toast.cur_x,
                            toast.cur_y.round() as i32,
                            toast.cur_visible_w,
                            toast.full_h(),
                        ];
                        point_in_rect(cursor.x, cursor.y, visible_rect)
                    } else {
                        false
                    };
                    if !hovered {
                        elapsed_ms += dt_ms;
                    }
                    if elapsed_ms >= HOLD_MS {
                        toast.phase = ToastPhase::SlideOut {
                            started: now,
                            start_fraction: 1.0,
                        };
                    } else {
                        toast.phase = ToastPhase::Hold {
                            elapsed_ms,
                            last_tick: now,
                        };
                    }
                }
                ToastPhase::SlideOut {
                    started,
                    start_fraction,
                } => {
                    let elapsed = now.saturating_duration_since(started).as_secs_f32() * 1000.0;
                    let t = (elapsed / SLIDE_OUT_MS).clamp(0.0, 1.0);
                    // Ease-in cubic retreat back into the right screen edge.
                    let ease_t = start_fraction * (1.0 - t.powi(3));
                    toast.cur_fraction = ease_t;
                    toast.cur_visible_w =
                        ((full_w as f32) * ease_t).round().clamp(1.0, full_w as f32) as i32;
                    toast.cur_x = toast.work_right - toast.cur_visible_w;
                    needs_present = true;
                    if t >= 1.0 {
                        should_remove = true;
                    }
                }
            }

            if should_remove {
                to_destroy.push(toast.hwnd);
            } else if needs_present {
                toast.present_now();
            }
        }

        if !to_destroy.is_empty() {
            mgr.toasts.retain(|t| !to_destroy.contains(&t.hwnd));
            mgr.recompute_targets();
        }

        let plan = mgr.update_timer_plan();
        (mgr.dispatcher_hwnd, plan)
    });

    for hwnd in to_destroy {
        unsafe {
            DestroyWindow(hwnd);
        }
    }
    apply_timer_plan(disp_hwnd, timer_plan);
}

fn handle_toast_click(hwnd: HWND, open_main_window: bool) {
    let now = Instant::now();
    let mut wake_target: isize = 0;

    let (disp_hwnd, timer_plan) = MANAGER.with(|m| {
        let mut mgr = m.borrow_mut();
        if let Some(toast) = mgr.toasts.iter_mut().find(|t| t.hwnd == hwnd) {
            if open_main_window {
                wake_target = toast.wake_hwnd;
            }
            toast.begin_slide_out(now);
        }
        let plan = mgr.update_timer_plan();
        (mgr.dispatcher_hwnd, plan)
    });

    if wake_target != 0 {
        super::win32::post_app_message(wake_target, super::win32::WM_APP_SHOW_WINDOW);
    }
    apply_timer_plan(disp_hwnd, timer_plan);
}

unsafe extern "system" fn toast_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_MOUSEACTIVATE => MA_NOACTIVATE as LRESULT,
        WM_SETCURSOR => {
            windows_sys::Win32::UI::WindowsAndMessaging::SetCursor(LoadCursorW(
                std::ptr::null_mut(),
                IDC_ARROW,
            ));
            1
        }
        WM_LBUTTONUP => {
            handle_toast_click(hwnd, true);
            0
        }
        WM_RBUTTONUP => {
            handle_toast_click(hwnd, false);
            0
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

unsafe extern "system" fn dispatcher_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_APP_NEW_TOAST => {
            drain_incoming_requests();
            0
        }
        WM_TIMER => {
            if wparam == TIMER_ID {
                tick_toasts();
            }
            0
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

fn worker_thread_main(rx: mpsc::Receiver<ToastRequest>, wake_hwnd_slot: Arc<AtomicIsize>) {
    let toast_class = encode_wide(TOAST_CLASS_NAME);
    let disp_class = encode_wide(DISPATCHER_CLASS_NAME);

    unsafe {
        let hinstance = GetModuleHandleW(std::ptr::null());

        let toast_wc = WNDCLASSW {
            lpfnWndProc: Some(toast_wnd_proc),
            hInstance: hinstance,
            hCursor: LoadCursorW(std::ptr::null_mut(), IDC_ARROW),
            lpszClassName: toast_class.as_ptr(),
            ..std::mem::zeroed()
        };
        RegisterClassW(&toast_wc);

        let disp_wc = WNDCLASSW {
            lpfnWndProc: Some(dispatcher_wnd_proc),
            hInstance: hinstance,
            lpszClassName: disp_class.as_ptr(),
            ..std::mem::zeroed()
        };
        RegisterClassW(&disp_wc);

        let disp_hwnd = CreateWindowExW(
            0,
            disp_class.as_ptr(),
            std::ptr::null(),
            0,
            0,
            0,
            0,
            0,
            HWND_MESSAGE,
            std::ptr::null_mut(),
            hinstance,
            std::ptr::null(),
        );
        if disp_hwnd.is_null() {
            return;
        }

        MANAGER.with(|m| {
            let mut mgr = m.borrow_mut();
            mgr.dispatcher_hwnd = disp_hwnd;
            mgr.rx = Some(rx);
        });

        wake_hwnd_slot.store(disp_hwnd as isize, Ordering::Release);
        PostMessageW(disp_hwnd, WM_APP_NEW_TOAST, 0, 0);

        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compute_safe_bottom_limit_clears_autohide_docked_and_tray_overflow() {
        // Auto-hide taskbar on 1080p at 125% DPI: work_bottom == screen_h -> reserves 56*1.25 + 16*1.25 = 90px.
        let autohide = compute_safe_bottom_limit(1080, 1080, 1.25, None);
        assert!(
            autohide <= 990,
            "expected <= 990 for auto-hide, got {autohide}"
        );
        assert_eq!(autohide, 990);

        // Docked taskbar on 1080p at 125% DPI: work_bottom already excludes taskbar (1020) -> 1020 - 20 = 1000.
        let docked = compute_safe_bottom_limit(1020, 1080, 1.25, None);
        assert_eq!(docked, 1000);

        // Open tray overflow flyout with top at y = 860 -> 860 - round(12 * 1.25) = 845.
        let overflow = compute_safe_bottom_limit(1080, 1080, 1.25, Some(860));
        assert_eq!(overflow, 845);

        // Clamp floor prevents negative or off-screen targets on tiny displays.
        assert_eq!(compute_safe_bottom_limit(100, 100, 1.0, Some(50)), 120);
    }
}
