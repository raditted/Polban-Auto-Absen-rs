//! Application and system tray icon loader backed by `assets/app-icon.png`.

use std::sync::OnceLock;

use image::imageops::FilterType;
use image::{ImageFormat, RgbaImage};

/// Optional status indicator drawn on the bottom-right corner of the icon.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IconStatus {
    Idle,
    Monitoring,
    Error,
}

const APP_ICON_PNG: &[u8] = include_bytes!("../../../assets/app-icon.png");
const RING_DARK: [u8; 3] = [0x0F, 0x17, 0x2A];

static BASE_ICON_RGBA: OnceLock<RgbaImage> = OnceLock::new();

fn base_icon() -> &'static RgbaImage {
    BASE_ICON_RGBA.get_or_init(|| {
        image::load_from_memory_with_format(APP_ICON_PNG, ImageFormat::Png)
            .map(|img| img.to_rgba8())
            .unwrap_or_else(|_| {
                RgbaImage::from_pixel(64, 64, image::Rgba([0xFF, 0xFF, 0xFF, 0xFF]))
            })
    })
}

fn status_color(status: IconStatus) -> [u8; 3] {
    match status {
        IconStatus::Idle => [0x64, 0x74, 0x8B],
        IconStatus::Monitoring => [0x10, 0xB9, 0x81],
        IconStatus::Error => [0xEF, 0x44, 0x44],
    }
}

fn blend_pixel(dst: &mut [u8; 4], src_rgb: [u8; 3], coverage: f32) {
    let a = coverage.clamp(0.0, 1.0);
    if a <= 0.0 {
        return;
    }
    let dst_a = dst[3] as f32 / 255.0;
    let out_a = a + dst_a * (1.0 - a);
    if out_a <= f32::EPSILON {
        *dst = [0, 0, 0, 0];
        return;
    }
    for i in 0..3 {
        let fg = src_rgb[i] as f32;
        let bg = dst[i] as f32;
        let out_c = (fg * a + bg * dst_a * (1.0 - a)) / out_a;
        dst[i] = out_c.round().clamp(0.0, 255.0) as u8;
    }
    dst[3] = (out_a * 255.0).round().clamp(0.0, 255.0) as u8;
}

/// Generate RGBA8888 pixels `(rgba, width, height)` for the given square size
/// from the bundled `assets/app-icon.png` asset with an optional status badge.
pub fn generate_rgba(size: u32, status: Option<IconStatus>) -> (Vec<u8>, u32, u32) {
    let size = size.max(16);
    let base = base_icon();
    let resized = if base.width() == size && base.height() == size {
        base.clone()
    } else {
        image::imageops::resize(base, size, size, FilterType::Lanczos3)
    };
    let mut rgba = resized.into_raw();

    if let Some(st) = status {
        let inv = 1.0 / (size as f32);
        let aa = inv * 0.9;
        let cx = 0.78_f32;
        let cy = 0.78_f32;
        let outer_r = 0.195_f32;
        let inner_r = 0.140_f32;
        let dot_rgb = status_color(st);

        for y in 0..size {
            for x in 0..size {
                let px = (x as f32 + 0.5) * inv;
                let py = (y as f32 + 0.5) * inv;
                let dist = ((px - cx).powi(2) + (py - cy).powi(2)).sqrt();
                let ring_cov = (0.5 - (dist - outer_r) / aa).clamp(0.0, 1.0);
                if ring_cov <= 0.0 {
                    continue;
                }
                let idx = ((y * size + x) as usize) * 4;
                let mut pixel = [rgba[idx], rgba[idx + 1], rgba[idx + 2], rgba[idx + 3]];
                blend_pixel(&mut pixel, RING_DARK, ring_cov);
                let dot_cov = (0.5 - (dist - inner_r) / aa).clamp(0.0, 1.0);
                if dot_cov > 0.0 {
                    blend_pixel(&mut pixel, dot_rgb, dot_cov);
                }
                rgba[idx..idx + 4].copy_from_slice(&pixel);
            }
        }
    }

    (rgba, size, size)
}

/// Build an `egui::IconData` for the application window icon.
pub fn egui_icon(size: u32, status: Option<IconStatus>) -> egui::IconData {
    let (rgba, width, height) = generate_rgba(size, status);
    egui::IconData {
        rgba,
        width,
        height,
    }
}

/// Build an `(rgba, width, height)` tuple suitable for `tray_icon::Icon::from_rgba`.
pub fn tray_rgba(size: u32, status: Option<IconStatus>) -> (Vec<u8>, u32, u32) {
    generate_rgba(size, status)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rgba_dimensions_and_determinism() {
        let (buf32_a, w32, h32) = generate_rgba(32, None);
        let (buf32_b, _, _) = generate_rgba(32, None);
        assert_eq!((w32, h32), (32, 32));
        assert_eq!(buf32_a.len(), 32 * 32 * 4);
        assert_eq!(buf32_a, buf32_b);

        let (buf64, w64, h64) = tray_rgba(64, Some(IconStatus::Monitoring));
        assert_eq!((w64, h64), (64, 64));
        assert_eq!(buf64.len(), 64 * 64 * 4);
    }

    #[test]
    fn bundled_app_icon_decodes_with_artwork_pixels() {
        let (buf, w, h) = generate_rgba(256, None);
        assert_eq!((w, h), (256, 256));
        assert_eq!(buf.len(), 256 * 256 * 4);

        let mut has_warm_artwork = false;
        let mut has_opaque = false;
        for chunk in buf.as_chunks::<4>().0 {
            if chunk[3] == 255 {
                has_opaque = true;
            }
            if chunk[0] > 180 && chunk[1] < 160 && chunk[2] < 160 {
                has_warm_artwork = true;
            }
        }
        assert!(has_opaque, "expected opaque pixels from app-icon.png");
        assert!(
            has_warm_artwork,
            "expected character hair/accent pixels from app-icon.png"
        );
    }

    #[test]
    fn status_badge_differs_by_state() {
        let (none_buf, _, _) = generate_rgba(32, None);
        let (idle_buf, _, _) = generate_rgba(32, Some(IconStatus::Idle));
        let (mon_buf, _, _) = generate_rgba(32, Some(IconStatus::Monitoring));
        let (err_buf, _, _) = generate_rgba(32, Some(IconStatus::Error));

        assert_ne!(none_buf, idle_buf);
        assert_ne!(idle_buf, mon_buf);
        assert_ne!(mon_buf, err_buf);

        // Center of badge (0.78 * 32 = 25, 25) should reflect Monitoring emerald / Error red.
        let idx = ((25 * 32 + 25) * 4) as usize;
        assert!(mon_buf[idx + 1] > mon_buf[idx], "expected emerald green");
        assert!(err_buf[idx] > err_buf[idx + 1], "expected error red");
    }

    #[test]
    fn egui_icon_has_valid_dimensions() {
        let icon = egui_icon(256, None);
        assert_eq!(icon.width, 256);
        assert_eq!(icon.height, 256);
        assert_eq!(icon.rgba.len(), 256 * 256 * 4);
    }
}
