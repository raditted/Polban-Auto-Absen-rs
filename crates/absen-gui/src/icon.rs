//! Deterministic RGBA app and tray icon generator with `PA` Cobalt monogram.

/// Optional status indicator drawn on the bottom-right corner of the icon.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IconStatus {
    Idle,
    Monitoring,
    Error,
}

const COBALT: [u8; 3] = [0x1D, 0x4E, 0xD8];
const WHITE: [u8; 3] = [0xFF, 0xFF, 0xFF];
const RING_DARK: [u8; 3] = [0x0F, 0x17, 0x2A];

fn status_color(status: IconStatus) -> [u8; 3] {
    match status {
        IconStatus::Idle => [0x64, 0x74, 0x8B],
        IconStatus::Monitoring => [0x10, 0xB9, 0x81],
        IconStatus::Error => [0xEF, 0x44, 0x44],
    }
}

/// Signed distance to a rounded rectangle centered at `(0.5, 0.5)` in `[0, 1]^2`.
fn sd_rounded_box(px: f32, py: f32, half_extent: f32, radius: f32) -> f32 {
    let dx = (px - 0.5).abs() - (half_extent - radius);
    let dy = (py - 0.5).abs() - (half_extent - radius);
    let ox = dx.max(0.0);
    let oy = dy.max(0.0);
    (ox * ox + oy * oy).sqrt() + dx.max(dy).min(0.0) - radius
}

/// Distance from point `(px, py)` to segment `(x1, y1)-(x2, y2)`.
fn dist_segment(px: f32, py: f32, x1: f32, y1: f32, x2: f32, y2: f32) -> f32 {
    let vx = x2 - x1;
    let vy = y2 - y1;
    let wx = px - x1;
    let wy = py - y1;
    let len_sq = vx * vx + vy * vy;
    if len_sq <= f32::EPSILON {
        return (wx * wx + wy * wy).sqrt();
    }
    let t = ((wx * vx + wy * vy) / len_sq).clamp(0.0, 1.0);
    let cx = x1 + t * vx;
    let cy = y1 + t * vy;
    ((px - cx).powi(2) + (py - cy).powi(2)).sqrt()
}

/// Distance to the geometric `PA` monogram in normalized `[0, 1]^2` coordinates.
fn sd_monogram_pa(px: f32, py: f32) -> f32 {
    let stroke = 0.048_f32;

    // Letter P (left side)
    let p_stem = dist_segment(px, py, 0.22, 0.29, 0.22, 0.71);
    let p_top = dist_segment(px, py, 0.22, 0.29, 0.36, 0.29);
    let p_mid = dist_segment(px, py, 0.22, 0.50, 0.36, 0.50);
    // Bowl arc on the right side of P
    let arc_cx = 0.36_f32;
    let arc_cy = 0.395_f32;
    let arc_r = 0.105_f32;
    let p_arc = if px >= arc_cx {
        (((px - arc_cx).powi(2) + (py - arc_cy).powi(2)).sqrt() - arc_r).abs()
    } else {
        f32::MAX
    };

    // Letter A (right side)
    let a_left = dist_segment(px, py, 0.54, 0.71, 0.67, 0.29);
    let a_right = dist_segment(px, py, 0.80, 0.71, 0.67, 0.29);
    let a_bar = dist_segment(px, py, 0.59, 0.57, 0.75, 0.57);

    let min_d = p_stem
        .min(p_top)
        .min(p_mid)
        .min(p_arc)
        .min(a_left)
        .min(a_right)
        .min(a_bar);
    min_d - stroke
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

/// Generate deterministic RGBA8888 pixels `(rgba, width, height)` for the given square size.
pub fn generate_rgba(size: u32, status: Option<IconStatus>) -> (Vec<u8>, u32, u32) {
    let size = size.max(16);
    let mut rgba = vec![0u8; (size as usize) * (size as usize) * 4];
    let inv = 1.0 / (size as f32);
    let aa = inv * 0.9;

    for y in 0..size {
        for x in 0..size {
            let px = (x as f32 + 0.5) * inv;
            let py = (y as f32 + 0.5) * inv;
            let mut pixel = [0u8, 0u8, 0u8, 0u8];

            // Rounded square Cobalt background
            let d_box = sd_rounded_box(px, py, 0.45, 0.14);
            let box_cov = (0.5 - d_box / aa).clamp(0.0, 1.0);
            if box_cov > 0.0 {
                blend_pixel(&mut pixel, COBALT, box_cov);

                // Crisp white PA monogram clipped to background
                let d_pa = sd_monogram_pa(px, py);
                let pa_cov = (0.5 - d_pa / aa).clamp(0.0, 1.0) * box_cov;
                if pa_cov > 0.0 {
                    blend_pixel(&mut pixel, WHITE, pa_cov);
                }
            }

            // Optional bottom-right circular status badge
            if let Some(st) = status {
                let cx = 0.78_f32;
                let cy = 0.78_f32;
                let dist = ((px - cx).powi(2) + (py - cy).powi(2)).sqrt();
                let outer_r = 0.185_f32;
                let inner_r = 0.135_f32;

                let ring_cov = (0.5 - (dist - outer_r) / aa).clamp(0.0, 1.0);
                if ring_cov > 0.0 {
                    blend_pixel(&mut pixel, RING_DARK, ring_cov);
                }
                let dot_cov = (0.5 - (dist - inner_r) / aa).clamp(0.0, 1.0);
                if dot_cov > 0.0 {
                    blend_pixel(&mut pixel, status_color(st), dot_cov);
                }
            }

            let idx = ((y * size + x) as usize) * 4;
            rgba[idx..idx + 4].copy_from_slice(&pixel);
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
    fn contains_cobalt_white_and_transparent_corners() {
        let (buf, w, _) = generate_rgba(64, None);
        // Top-left corner (0, 0) should be outside rounded rect -> transparent.
        assert_eq!(buf[3], 0);

        let mut has_cobalt = false;
        let mut has_white = false;
        for chunk in buf.as_chunks::<4>().0 {
            if chunk[3] > 240 {
                if chunk[0] < 45 && chunk[1] < 95 && chunk[2] > 195 {
                    has_cobalt = true;
                }
                if chunk[0] > 240 && chunk[1] > 240 && chunk[2] > 240 {
                    has_white = true;
                }
            }
        }
        assert!(has_cobalt, "expected Cobalt background pixels");
        assert!(has_white, "expected white monogram pixels");
        assert_eq!(w, 64);
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
        let icon = egui_icon(64, None);
        assert_eq!(icon.width, 64);
        assert_eq!(icon.height, 64);
        assert_eq!(icon.rgba.len(), 64 * 64 * 4);
    }
}
