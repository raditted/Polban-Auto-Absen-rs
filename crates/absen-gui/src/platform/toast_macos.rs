//! Native macOS non-activating floating corner toast stack with Minecraft-style horizontal slide.
//!
//! Renders the shared pixel-art toast sprites from [`super::toast_render`] inside borderless
//! non-activating `NSPanel` overlays anchored flush to the top-right `NSScreen::visibleFrame`
//! edge (below the menu bar), stacking downward and clipping cleanly at the monitor edge so
//! toasts never steal focus or bleed onto adjacent monitors.

#[cfg(target_os = "macos")]
use std::cell::RefCell;
#[cfg(target_os = "macos")]
use std::ptr::NonNull;
#[cfg(target_os = "macos")]
use std::sync::{Arc, Mutex, OnceLock};
#[cfg(target_os = "macos")]
use std::time::Instant;

#[cfg(target_os = "macos")]
use block2::RcBlock;
#[cfg(target_os = "macos")]
use dispatch2::DispatchQueue;
#[cfg(target_os = "macos")]
use objc2::rc::Retained;
#[cfg(target_os = "macos")]
use objc2::{define_class, msg_send, AnyThread, DefinedClass, MainThreadMarker, MainThreadOnly};
#[cfg(target_os = "macos")]
use objc2_app_kit::{
    NSApplication, NSAutoresizingMaskOptions, NSBackingStoreType, NSColor, NSEvent, NSImage,
    NSImageAlignment, NSImageScaling, NSImageView, NSPanel, NSResponder, NSScreen,
    NSStatusWindowLevel, NSView, NSWindow, NSWindowAnimationBehavior, NSWindowCollectionBehavior,
    NSWindowStyleMask,
};
#[cfg(target_os = "macos")]
use objc2_foundation::{
    NSData, NSObject, NSPoint, NSRect, NSRunLoop, NSRunLoopCommonModes, NSSize, NSTimer,
};

#[cfg(target_os = "macos")]
use super::toast_render::{encode_png, render_toast_card, ToastCardSpec, ToastKind};
#[cfg(target_os = "macos")]
use crate::theme::Palette;

#[cfg(target_os = "macos")]
const MAX_VISIBLE_TOASTS: usize = 3;
const TOP_MARGIN_PT: f64 = 16.0;
const STACK_GAP_PT: f64 = 8.0;

const SLIDE_IN_MS: f32 = 320.0;
#[cfg(target_os = "macos")]
const HOLD_MS: f32 = 4500.0;
const SLIDE_OUT_MS: f32 = 320.0;

#[cfg(target_os = "macos")]
const TIMER_ANIM_MS: u32 = 16;
#[cfg(target_os = "macos")]
const TIMER_HOLD_MS: u32 = 50;

/// Compute the `render_toast_card` scale parameter so every macOS display (1x, 2x Retina, 3x)
/// renders exact integer pixel blocks at a consistent `2.0 pt` per Minecraft GUI pixel
/// (`320–368 × 64 pt` for a 1-line toast).
pub fn macos_render_scale(backing_scale: f64) -> f32 {
    let safe = if backing_scale.is_finite() && backing_scale >= 1.0 {
        backing_scale
    } else {
        1.0
    };
    if safe < 1.5 {
        1.0
    } else {
        (safe * 2.0).clamp(4.0, 6.0) as f32
    }
}

/// Convert rendered bitmap pixel dimensions to macOS logical points for the given backing scale.
pub fn macos_logical_size(rendered_w: u32, rendered_h: u32, backing_scale: f64) -> (f64, f64) {
    let safe = if backing_scale.is_finite() && backing_scale >= 1.0 {
        backing_scale
    } else {
        1.0
    };
    (
        (rendered_w as f64 / safe).max(1.0),
        (rendered_h as f64 / safe).max(1.0),
    )
}

/// Compute Cocoa bottom-left `origin.y` targets for a top-right downward-growing toast stack.
///
/// `heights` is ordered oldest-to-newest; the newest toast sits highest (closest to the menu bar)
/// and older toasts stack below it, matching standard macOS top-right notification flow.
pub fn compute_macos_stack_targets(
    visible_bottom: f64,
    visible_top: f64,
    heights: &[f64],
) -> Vec<f64> {
    let mut targets = vec![visible_bottom; heights.len()];
    let mut cursor_top = visible_top - TOP_MARGIN_PT;
    for (idx, &h) in heights.iter().enumerate().rev() {
        let target_y = (cursor_top - h).max(visible_bottom + TOP_MARGIN_PT);
        targets[idx] = target_y;
        cursor_top = target_y - STACK_GAP_PT;
    }
    targets
}

/// Compute a panel's Cocoa frame `(origin_x, origin_y, visible_w, full_h)` anchored flush to
/// `work_right` and clipped cleanly at the right screen boundary.
pub fn compute_macos_frame(
    work_right: f64,
    y: f64,
    full_w: f64,
    full_h: f64,
    fraction: f64,
) -> (f64, f64, f64, f64) {
    let safe_w = full_w.max(1.0);
    let safe_h = full_h.max(1.0);
    let clamped_frac = if fraction.is_finite() {
        fraction.clamp(0.0, 1.0)
    } else {
        0.0
    };
    let visible_w = (safe_w * clamped_frac).round().clamp(1.0, safe_w);
    let origin_x = work_right - visible_w;
    (origin_x, y.round(), visible_w, safe_h)
}

/// Returns `true` when global Cocoa point `(px, py)` falls within `(x, y, width, height)`.
pub fn point_in_macos_rect(px: f64, py: f64, rect: (f64, f64, f64, f64)) -> bool {
    let (rx, ry, rw, rh) = rect;
    rw > 0.0 && rh > 0.0 && px >= rx && py >= ry && px <= rx + rw && py <= ry + rh
}

/// Cubic ease-out slide-in progress (`0.0..=1.0`).
pub fn slide_in_fraction(elapsed_ms: f32) -> (f32, bool) {
    let t = (elapsed_ms / SLIDE_IN_MS).clamp(0.0, 1.0);
    let ease_t = 1.0 - (1.0 - t).powi(3);
    (ease_t, t >= 1.0)
}

/// Cubic ease-in slide-out progress (`start_fraction..=0.0`).
pub fn slide_out_fraction(elapsed_ms: f32, start_fraction: f32) -> (f32, bool) {
    let t = (elapsed_ms / SLIDE_OUT_MS).clamp(0.0, 1.0);
    let ease_t = start_fraction.clamp(0.0, 1.0) * (1.0 - t.powi(3));
    (ease_t, t >= 1.0)
}

#[cfg(target_os = "macos")]
/// Payload submitted to the macOS main-queue toast presenter.
#[derive(Clone)]
pub struct ToastRequest {
    pub kind: ToastKind,
    pub title: String,
    pub body: String,
    pub palette: Palette,
    pub on_open: Option<Arc<dyn Fn() + Send + Sync>>,
}

#[cfg(target_os = "macos")]
static PENDING_TOASTS: OnceLock<Mutex<Vec<ToastRequest>>> = OnceLock::new();

#[cfg(target_os = "macos")]
fn pending_queue() -> &'static Mutex<Vec<ToastRequest>> {
    PENDING_TOASTS.get_or_init(|| Mutex::new(Vec::new()))
}

#[cfg(target_os = "macos")]
/// Enqueue a custom Polban Presensi Minecraft-style corner toast without blocking the caller.
pub fn show_toast(req: ToastRequest) {
    match pending_queue().lock() {
        Ok(mut guard) => guard.push(req),
        Err(poisoned) => poisoned.into_inner().push(req),
    }
    DispatchQueue::main().exec_async(|| {
        if let Some(mtm) = MainThreadMarker::new() {
            drain_incoming_requests(mtm);
        }
    });
}

#[cfg(target_os = "macos")]
define_class!(
    #[unsafe(super(NSPanel, NSWindow, NSResponder, NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "PolbanPresensiToastPanel"]
    struct ToastPanel;

    impl ToastPanel {
        #[unsafe(method(canBecomeKeyWindow))]
        fn __can_become_key_window(&self) -> bool {
            false
        }

        #[unsafe(method(canBecomeMainWindow))]
        fn __can_become_main_window(&self) -> bool {
            false
        }
    }
);

#[cfg(target_os = "macos")]
impl ToastPanel {
    fn new(frame: NSRect, mtm: MainThreadMarker) -> Retained<Self> {
        let style = NSWindowStyleMask::Borderless | NSWindowStyleMask::NonactivatingPanel;
        let this = Self::alloc(mtm).set_ivars(());
        let panel: Retained<Self> = unsafe {
            msg_send![
                super(this),
                initWithContentRect: frame,
                styleMask: style,
                backing: NSBackingStoreType::Buffered,
                defer: false
            ]
        };
        panel.setFloatingPanel(true);
        panel.setBecomesKeyOnlyIfNeeded(true);
        panel.setWorksWhenModal(true);
        panel.setHidesOnDeactivate(false);
        panel.setLevel(NSStatusWindowLevel);
        panel.setOpaque(false);
        panel.setBackgroundColor(Some(&NSColor::clearColor()));
        panel.setHasShadow(false);
        panel.setAnimationBehavior(NSWindowAnimationBehavior::None);
        panel.setCollectionBehavior(
            NSWindowCollectionBehavior::CanJoinAllSpaces
                | NSWindowCollectionBehavior::Stationary
                | NSWindowCollectionBehavior::IgnoresCycle
                | NSWindowCollectionBehavior::FullScreenAuxiliary,
        );
        unsafe {
            panel.setReleasedWhenClosed(false);
        }
        panel
    }
}

#[cfg(target_os = "macos")]
struct ToastViewIvars {
    toast_id: u64,
}

#[cfg(target_os = "macos")]
define_class!(
    #[unsafe(super(NSView, NSResponder, NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "PolbanPresensiToastContentView"]
    #[ivars = ToastViewIvars]
    struct ToastContentView;

    impl ToastContentView {
        #[unsafe(method(acceptsFirstMouse:))]
        fn __accepts_first_mouse(&self, _event: Option<&NSEvent>) -> bool {
            true
        }

        #[unsafe(method(mouseDown:))]
        fn __mouse_down(&self, _event: &NSEvent) {
            handle_toast_click(self.ivars().toast_id, true);
        }

        #[unsafe(method(rightMouseDown:))]
        fn __right_mouse_down(&self, _event: &NSEvent) {
            handle_toast_click(self.ivars().toast_id, false);
        }
    }
);

#[cfg(target_os = "macos")]
impl ToastContentView {
    fn new(frame: NSRect, toast_id: u64, mtm: MainThreadMarker) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(ToastViewIvars { toast_id });
        let view: Retained<Self> = unsafe { msg_send![super(this), initWithFrame: frame] };
        view.setWantsLayer(true);
        view
    }
}

#[cfg(target_os = "macos")]
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

#[cfg(target_os = "macos")]
struct ActiveToast {
    id: u64,
    panel: Retained<ToastPanel>,
    on_open: Option<Arc<dyn Fn() + Send + Sync>>,
    full_w: f64,
    full_h: f64,
    work_right: f64,
    target_y: f64,
    cur_y: f64,
    cur_fraction: f32,
    phase: ToastPhase,
}

#[cfg(target_os = "macos")]
impl ActiveToast {
    fn current_frame(&self) -> (f64, f64, f64, f64) {
        compute_macos_frame(
            self.work_right,
            self.cur_y,
            self.full_w,
            self.full_h,
            f64::from(self.cur_fraction),
        )
    }

    fn apply_frame(&self) {
        let (x, y, w, h) = self.current_frame();
        let rect = NSRect {
            origin: NSPoint { x, y },
            size: NSSize {
                width: w,
                height: h,
            },
        };
        self.panel.setFrame_display(rect, true);
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

#[cfg(target_os = "macos")]
struct MacToastManager {
    next_id: u64,
    toasts: Vec<ActiveToast>,
    timer: Option<Retained<NSTimer>>,
    timer_interval_ms: Option<u32>,
}

#[cfg(target_os = "macos")]
impl MacToastManager {
    fn new() -> Self {
        Self {
            next_id: 1,
            toasts: Vec::new(),
            timer: None,
            timer_interval_ms: None,
        }
    }

    fn recompute_targets(&mut self, mtm: MainThreadMarker) {
        if self.toasts.is_empty() {
            return;
        }
        let (vis_bottom, vis_top, vis_right, _) = active_screen_metrics(mtm);
        let heights: Vec<f64> = self.toasts.iter().map(|t| t.full_h).collect();
        let targets = compute_macos_stack_targets(vis_bottom, vis_top, &heights);
        for (toast, target_y) in self.toasts.iter_mut().zip(targets) {
            toast.work_right = vis_right;
            toast.target_y = target_y;
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
            ) || (t.cur_y - t.target_y).abs() > 0.5
        });
        if needs_fast {
            Some(TIMER_ANIM_MS)
        } else {
            Some(TIMER_HOLD_MS)
        }
    }

    fn sync_timer(&mut self) {
        let desired = self.desired_timer_interval();
        if desired == self.timer_interval_ms {
            return;
        }
        if let Some(old) = self.timer.take() {
            old.invalidate();
        }
        self.timer_interval_ms = desired;
        if let Some(ms) = desired {
            let block: RcBlock<dyn Fn(NonNull<NSTimer>)> =
                RcBlock::new(|_timer: NonNull<NSTimer>| {
                    if let Some(mtm) = MainThreadMarker::new() {
                        tick_toasts(mtm);
                    }
                });
            let timer = unsafe {
                NSTimer::timerWithTimeInterval_repeats_block(f64::from(ms) / 1000.0, true, &block)
            };
            unsafe {
                NSRunLoop::mainRunLoop().addTimer_forMode(&timer, NSRunLoopCommonModes);
            }
            self.timer = Some(timer);
        }
    }
}

#[cfg(target_os = "macos")]
thread_local! {
    static MANAGER: RefCell<MacToastManager> = RefCell::new(MacToastManager::new());
}

#[cfg(target_os = "macos")]
fn active_screen_metrics(mtm: MainThreadMarker) -> (f64, f64, f64, f64) {
    let screen = NSScreen::mainScreen(mtm).or_else(|| {
        let screens = NSScreen::screens(mtm);
        screens.firstObject()
    });
    if let Some(screen) = screen {
        let frame = screen.visibleFrame();
        let scale = screen.backingScaleFactor();
        (
            frame.origin.y,
            frame.origin.y + frame.size.height,
            frame.origin.x + frame.size.width,
            if scale.is_finite() && scale >= 1.0 {
                scale
            } else {
                1.0
            },
        )
    } else {
        (0.0, 900.0, 1440.0, 2.0)
    }
}

#[cfg(target_os = "macos")]
fn spawn_toast_panel(req: ToastRequest, mtm: MainThreadMarker) {
    let (_vis_bottom, _vis_top, vis_right, backing_scale) = active_screen_metrics(mtm);
    let render_scale = macos_render_scale(backing_scale);
    let spec = ToastCardSpec {
        kind: req.kind,
        title: req.title,
        body: req.body,
        progress: 1.0,
        close_hovered: false,
        palette: req.palette,
    };
    let rendered = render_toast_card(&spec, render_scale);
    let Ok(png_bytes) = encode_png(&rendered) else {
        return;
    };
    let (full_w, full_h) = macos_logical_size(rendered.width, rendered.height, backing_scale);

    let data = NSData::with_bytes(&png_bytes);
    let Some(image) = NSImage::initWithData(NSImage::alloc(), &data) else {
        return;
    };
    image.setSize(NSSize {
        width: full_w,
        height: full_h,
    });

    let toast_id = MANAGER.with(|m| {
        let mut mgr = m.borrow_mut();
        let id = mgr.next_id;
        mgr.next_id = mgr.next_id.wrapping_add(1).max(1);
        id
    });

    let initial_rect = NSRect {
        origin: NSPoint {
            x: vis_right - 1.0,
            y: 0.0,
        },
        size: NSSize {
            width: 1.0,
            height: full_h,
        },
    };
    let panel = ToastPanel::new(initial_rect, mtm);
    let content_view = ToastContentView::new(
        NSRect {
            origin: NSPoint { x: 0.0, y: 0.0 },
            size: NSSize {
                width: 1.0,
                height: full_h,
            },
        },
        toast_id,
        mtm,
    );
    let image_view = NSImageView::imageViewWithImage(&image, mtm);
    image_view.setImageScaling(NSImageScaling::ScaleNone);
    image_view.setImageAlignment(NSImageAlignment::AlignLeft);
    image_view.setAutoresizingMask(NSAutoresizingMaskOptions::ViewNotSizable);
    image_view.setFrame(NSRect {
        origin: NSPoint { x: 0.0, y: 0.0 },
        size: NSSize {
            width: full_w,
            height: full_h,
        },
    });
    content_view.addSubview(&image_view);
    panel.setContentView(Some(&content_view));

    let now = Instant::now();
    let toast = ActiveToast {
        id: toast_id,
        panel: panel.clone(),
        on_open: req.on_open,
        full_w,
        full_h,
        work_right: vis_right,
        target_y: 0.0,
        cur_y: 0.0,
        cur_fraction: 0.0,
        phase: ToastPhase::SlideIn { started: now },
    };

    MANAGER.with(|m| {
        let mut mgr = m.borrow_mut();

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
        mgr.recompute_targets(mtm);

        if let Some(newest) = mgr.toasts.last_mut() {
            newest.cur_y = newest.target_y;
            newest.cur_fraction = 0.0;
            newest.apply_frame();
        }
        mgr.sync_timer();
    });

    panel.orderFrontRegardless();
}

#[cfg(target_os = "macos")]
fn drain_incoming_requests(mtm: MainThreadMarker) {
    let pending = match pending_queue().lock() {
        Ok(mut guard) => std::mem::take(&mut *guard),
        Err(poisoned) => std::mem::take(&mut *poisoned.into_inner()),
    };
    for req in pending {
        spawn_toast_panel(req, mtm);
    }
}

#[cfg(target_os = "macos")]
fn tick_toasts(mtm: MainThreadMarker) {
    let cursor = NSEvent::mouseLocation();
    let now = Instant::now();
    let mut to_close: Vec<Retained<ToastPanel>> = Vec::new();

    MANAGER.with(|m| {
        let mut mgr = m.borrow_mut();
        mgr.recompute_targets(mtm);

        let mut finished_ids: Vec<u64> = Vec::new();
        for toast in &mut mgr.toasts {
            let dy = toast.target_y - toast.cur_y;
            let moved_y = dy.abs() > 0.5;
            if dy.abs() <= 0.6 {
                toast.cur_y = toast.target_y;
            } else {
                toast.cur_y += dy * 0.28;
            }

            let mut needs_present = moved_y;
            let mut should_remove = false;

            match toast.phase {
                ToastPhase::SlideIn { started } => {
                    let elapsed = now.saturating_duration_since(started).as_secs_f32() * 1000.0;
                    let (ease_t, done) = slide_in_fraction(elapsed);
                    toast.cur_fraction = ease_t;
                    needs_present = true;
                    if done {
                        toast.cur_fraction = 1.0;
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
                    let hovered = point_in_macos_rect(cursor.x, cursor.y, toast.current_frame());
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
                    let (ease_t, done) = slide_out_fraction(elapsed, start_fraction);
                    toast.cur_fraction = ease_t;
                    needs_present = true;
                    if done {
                        should_remove = true;
                    }
                }
            }

            if should_remove {
                finished_ids.push(toast.id);
                to_close.push(toast.panel.clone());
            } else if needs_present {
                toast.apply_frame();
            }
        }

        if !finished_ids.is_empty() {
            mgr.toasts.retain(|t| !finished_ids.contains(&t.id));
            mgr.recompute_targets(mtm);
        }
        mgr.sync_timer();
    });

    for panel in to_close {
        panel.orderOut(None);
        panel.close();
    }
}

#[cfg(target_os = "macos")]
fn handle_toast_click(toast_id: u64, open_main_window: bool) {
    let now = Instant::now();
    let mut on_open: Option<Arc<dyn Fn() + Send + Sync>> = None;

    MANAGER.with(|m| {
        let mut mgr = m.borrow_mut();
        if let Some(toast) = mgr.toasts.iter_mut().find(|t| t.id == toast_id) {
            if open_main_window {
                on_open = toast.on_open.clone();
            }
            toast.begin_slide_out(now);
        }
        mgr.sync_timer();
    });

    if open_main_window {
        if let Some(cb) = on_open {
            cb();
        }
        if let Some(mtm) = MainThreadMarker::new() {
            #[allow(deprecated)]
            NSApplication::sharedApplication(mtm).activateIgnoringOtherApps(true);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::toast_render::toast_pixel_scale;

    #[test]
    fn macos_render_scale_keeps_consistent_two_points_per_gui_pixel() {
        // 1x non-Retina: pixel_scale=2 -> 160*2=320px / 1.0 = 320pt
        let s1 = macos_render_scale(1.0);
        assert_eq!(toast_pixel_scale(s1), 2);
        assert_eq!(macos_logical_size(160 * 2, 32 * 2, 1.0), (320.0, 64.0));

        // 2x Retina: pixel_scale=4 -> 160*4=640px / 2.0 = 320pt
        let s2 = macos_render_scale(2.0);
        assert_eq!(toast_pixel_scale(s2), 4);
        assert_eq!(macos_logical_size(160 * 4, 32 * 4, 2.0), (320.0, 64.0));

        // 3x HiDPI: pixel_scale=6 -> 160*6=960px / 3.0 = 320pt
        let s3 = macos_render_scale(3.0);
        assert_eq!(toast_pixel_scale(s3), 6);
        assert_eq!(macos_logical_size(160 * 6, 32 * 6, 3.0), (320.0, 64.0));

        // Non-finite fallback stays safe.
        assert_eq!(macos_render_scale(f64::NAN), 1.0);
    }

    #[test]
    fn macos_stack_targets_anchor_top_right_below_menu_bar_and_stack_downward() {
        // Simulated 1440x900 screen with 25pt menu bar and 70pt Dock: visible y = 70..875.
        let targets = compute_macos_stack_targets(70.0, 875.0, &[64.0, 84.0, 64.0]);
        // Newest (index 2) sits highest: 875 - 16 - 64 = 795.
        assert_eq!(targets[2], 795.0);
        // Middle (index 1): 795 - 8 - 84 = 703.
        assert_eq!(targets[1], 703.0);
        // Oldest (index 0): 703 - 8 - 64 = 631.
        assert_eq!(targets[0], 631.0);
    }

    #[test]
    fn macos_frame_clips_flush_to_right_edge_and_hit_test_matches() {
        let (x0, y0, w0, h0) = compute_macos_frame(1440.0, 795.0, 320.0, 64.0, 0.0);
        assert_eq!((x0, y0, w0, h0), (1439.0, 795.0, 1.0, 64.0));

        let (x_half, _, w_half, _) = compute_macos_frame(1440.0, 795.0, 320.0, 64.0, 0.5);
        assert_eq!((x_half, w_half), (1280.0, 160.0));
        assert_eq!(x_half + w_half, 1440.0);

        let full = compute_macos_frame(1440.0, 795.0, 320.0, 64.0, 1.0);
        assert_eq!(full, (1120.0, 795.0, 320.0, 64.0));
        assert!(point_in_macos_rect(1200.0, 810.0, full));
        assert!(!point_in_macos_rect(1119.0, 810.0, full));
        assert!(!point_in_macos_rect(1200.0, 860.0, full));
    }

    #[test]
    fn slide_easing_curves_are_monotonic_and_bounded() {
        let (f0, done0) = slide_in_fraction(0.0);
        let (f_mid, done_mid) = slide_in_fraction(SLIDE_IN_MS * 0.5);
        let (f1, done1) = slide_in_fraction(SLIDE_IN_MS);
        assert_eq!(f0, 0.0);
        assert!(!done0 && !done_mid && done1);
        assert!(f_mid > 0.5 && f_mid < 1.0);
        assert_eq!(f1, 1.0);

        let (o0, od0) = slide_out_fraction(0.0, 1.0);
        let (o_mid, od_mid) = slide_out_fraction(SLIDE_OUT_MS * 0.5, 1.0);
        let (o1, od1) = slide_out_fraction(SLIDE_OUT_MS, 1.0);
        assert_eq!(o0, 1.0);
        assert!(!od0 && !od_mid && od1);
        assert!(o_mid > 0.5 && o_mid < 1.0);
        assert_eq!(o1, 0.0);
    }
}
