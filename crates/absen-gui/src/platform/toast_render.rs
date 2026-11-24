//! Cross-platform pure-Rust renderer for authentic Minecraft-style desktop toast cards.
//!
//! Uses official Minecraft 1.21.4 9-slice toast sprites (`advancement.png`, `system.png`),
//! proportional `ascii.png` bitmap typography with 25%-brightness `(+1, +1)` drop shadows,
//! and crisp integer nearest-neighbor scaling.

use std::io::Cursor;
use std::sync::{Mutex, OnceLock};

use image::codecs::png::PngEncoder;
use image::{ColorType, ImageEncoder, ImageFormat, RgbaImage};

use crate::theme::Palette;

const ADVANCEMENT_PNG: &[u8] = include_bytes!("../../../../assets/toast/advancement.png");
const SYSTEM_PNG: &[u8] = include_bytes!("../../../../assets/toast/system.png");
const ASCII_PNG: &[u8] = include_bytes!("../../../../assets/toast/ascii.png");

const MIN_GUI_WIDTH: u32 = 160;
const MAX_GUI_WIDTH: u32 = 184;
const TEXT_RIGHT_PAD: u32 = 6;

const COLOR_TITLE_YELLOW: [u8; 4] = [255, 255, 85, 255];
const COLOR_BODY_WHITE: [u8; 4] = [255, 255, 255, 255];

type CachedIcon = (u32, Vec<u8>);

static ADVANCEMENT_SPRITE: OnceLock<RgbaImage> = OnceLock::new();
static SYSTEM_SPRITE: OnceLock<RgbaImage> = OnceLock::new();
static BITMAP_FONT: OnceLock<MinecraftFont> = OnceLock::new();
static ICON_CACHE: OnceLock<Mutex<Option<CachedIcon>>> = OnceLock::new();

/// Semantic category selecting the Minecraft toast sprite (`advancement` vs `system`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToastKind {
    Success,
    Info,
    Error,
}

/// Input specification for rendering a single toast notification frame.
#[derive(Debug, Clone)]
pub struct ToastCardSpec {
    pub kind: ToastKind,
    pub title: String,
    pub body: String,
    /// Retained for API compatibility; Minecraft toasts do not render a progress bar.
    pub progress: f32,
    /// Retained for API compatibility; Minecraft toasts do not render a close button.
    pub close_hovered: bool,
    /// Retained for API compatibility; Minecraft toasts use authentic sprite palettes.
    pub palette: Palette,
}

/// Rendered straight-alpha RGBA8888 bitmap and hit-test regions in canvas pixel coordinates.
#[derive(Debug, Clone)]
pub struct RenderedToast {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
    /// Card bounding rectangle as `[x, y, width, height]` in canvas pixels.
    pub card_rect: [i32; 4],
    /// Always `[0, 0, 0, 0]` because Minecraft toasts have no close (`×`) button.
    pub close_rect: [i32; 4],
}

/// Compute the integer pixel scale factor matching compact Minecraft Java GUI scaling.
pub fn toast_pixel_scale(scale: f32) -> u32 {
    let safe = if scale.is_finite() { scale } else { 1.0 };
    if safe < 1.5 {
        2
    } else if safe < 2.25 {
        3
    } else {
        (safe.round() as u32).clamp(4, 6)
    }
}

struct MinecraftFont {
    sheet: RgbaImage,
    widths: [u32; 256],
}

impl MinecraftFont {
    fn load() -> Self {
        let sheet = image::load_from_memory_with_format(ASCII_PNG, ImageFormat::Png)
            .expect("bundled assets/toast/ascii.png must be a valid PNG")
            .to_rgba8();
        let mut widths = [4u32; 256];
        for (cp, slot) in widths.iter_mut().enumerate() {
            if cp == 0x20 {
                *slot = 4;
                continue;
            }
            let cell_x = (cp as u32 % 16) * 8;
            let cell_y = (cp as u32 / 16) * 8;
            let mut max_col: Option<u32> = None;
            for gx in 0..8 {
                let mut col_has_ink = false;
                for gy in 0..8 {
                    let sx = cell_x + gx;
                    let sy = cell_y + gy;
                    if sx < sheet.width() && sy < sheet.height() && sheet.get_pixel(sx, sy)[3] > 0 {
                        col_has_ink = true;
                        break;
                    }
                }
                if col_has_ink {
                    max_col = Some(gx);
                }
            }
            *slot = match max_col {
                Some(col) => col + 2,
                None => 4,
            };
        }
        Self { sheet, widths }
    }

    #[inline]
    fn map_char(ch: char) -> u8 {
        match ch {
            ' '..='~' => ch as u8,
            '\u{2013}' | '\u{2014}' => b'-',
            '\u{2018}' | '\u{2019}' => b'\'',
            '\u{201C}' | '\u{201D}' => b'"',
            '\u{00B7}' | '\u{2022}' => 0xFA,
            'á' | 'à' | 'â' | 'ä' | 'ã' => b'a',
            'Á' | 'À' | 'Â' | 'Ä' | 'Ã' => b'A',
            'é' | 'è' | 'ê' | 'ë' => b'e',
            'É' | 'È' | 'Ê' | 'Ë' => b'E',
            'í' | 'ì' | 'î' | 'ï' => b'i',
            'Í' | 'Ì' | 'Î' | 'Ï' => b'I',
            'ó' | 'ò' | 'ô' | 'ö' | 'õ' => b'o',
            'Ó' | 'Ò' | 'Ô' | 'Ö' | 'Õ' => b'O',
            'ú' | 'ù' | 'û' | 'ü' => b'u',
            'Ú' | 'Ù' | 'Û' | 'Ü' => b'U',
            _ => b'?',
        }
    }

    #[inline]
    fn char_advance(&self, ch: char) -> u32 {
        self.widths[Self::map_char(ch) as usize]
    }

    fn measure(&self, text: &str) -> u32 {
        text.chars().map(|ch| self.char_advance(ch)).sum()
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_text(
        &self,
        dst: &mut [u8],
        dst_w: u32,
        dst_h: u32,
        start_x: i32,
        start_y: i32,
        color: [u8; 4],
        text: &str,
    ) {
        let shadow = [color[0] / 4, color[1] / 4, color[2] / 4, color[3]];

        // Pass 1: drop shadow at (+1, +1) GUI pixel so foreground never gets overlapped by a later shadow.
        let mut pen_x = start_x;
        for ch in text.chars() {
            let cp = Self::map_char(ch);
            self.draw_glyph_raw(dst, dst_w, dst_h, pen_x + 1, start_y + 1, cp, shadow);
            pen_x += self.widths[cp as usize] as i32;
        }

        // Pass 2: crisp foreground glyph pixels.
        let mut pen_x = start_x;
        for ch in text.chars() {
            let cp = Self::map_char(ch);
            self.draw_glyph_raw(dst, dst_w, dst_h, pen_x, start_y, cp, color);
            pen_x += self.widths[cp as usize] as i32;
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_glyph_raw(
        &self,
        dst: &mut [u8],
        dst_w: u32,
        dst_h: u32,
        ox: i32,
        oy: i32,
        cp: u8,
        color: [u8; 4],
    ) {
        let cell_x = (cp as u32 % 16) * 8;
        let cell_y = (cp as u32 / 16) * 8;
        for gy in 0..8u32 {
            let py = oy + gy as i32;
            if py < 0 || py >= dst_h as i32 {
                continue;
            }
            for gx in 0..8u32 {
                let px = ox + gx as i32;
                if px < 0 || px >= dst_w as i32 {
                    continue;
                }
                let sx = cell_x + gx;
                let sy = cell_y + gy;
                if sx >= self.sheet.width() || sy >= self.sheet.height() {
                    continue;
                }
                let src_a = self.sheet.get_pixel(sx, sy)[3];
                if src_a == 0 {
                    continue;
                }
                let alpha = ((color[3] as u32 * src_a as u32 + 127) / 255) as u8;
                let idx = ((py as u32 * dst_w + px as u32) as usize) * 4;
                let mut cur = [dst[idx], dst[idx + 1], dst[idx + 2], dst[idx + 3]];
                blend_over(&mut cur, [color[0], color[1], color[2], alpha]);
                dst[idx..idx + 4].copy_from_slice(&cur);
            }
        }
    }
}

fn advancement_sprite() -> &'static RgbaImage {
    ADVANCEMENT_SPRITE.get_or_init(|| {
        image::load_from_memory_with_format(ADVANCEMENT_PNG, ImageFormat::Png)
            .expect("bundled assets/toast/advancement.png must be a valid PNG")
            .to_rgba8()
    })
}

fn system_sprite() -> &'static RgbaImage {
    SYSTEM_SPRITE.get_or_init(|| {
        image::load_from_memory_with_format(SYSTEM_PNG, ImageFormat::Png)
            .expect("bundled assets/toast/system.png must be a valid PNG")
            .to_rgba8()
    })
}

fn bitmap_font() -> &'static MinecraftFont {
    BITMAP_FONT.get_or_init(MinecraftFont::load)
}

fn cached_app_icon_rgba(size: u32) -> Vec<u8> {
    let size = size.max(16);
    let cache = ICON_CACHE.get_or_init(|| Mutex::new(None));
    if let Ok(guard) = cache.lock() {
        if let Some((cached_size, bytes)) = guard.as_ref() {
            if *cached_size == size {
                return bytes.clone();
            }
        }
    }
    let (rgba, _, _) = crate::icon::generate_rgba(size, None);
    if let Ok(mut guard) = cache.lock() {
        *guard = Some((size, rgba.clone()));
    }
    rgba
}

/// Returns `true` when `(x, y)` lies within `rect` (`[x, y, width, height]`).
pub fn point_in_rect(x: i32, y: i32, rect: [i32; 4]) -> bool {
    let [rx, ry, rw, rh] = rect;
    if rw <= 0 || rh <= 0 {
        return false;
    }
    x >= rx && y >= ry && x < rx.saturating_add(rw) && y < ry.saturating_add(rh)
}

/// Convert straight-alpha RGBA8888 pixels to premultiplied BGRA8888 for Win32 `UpdateLayeredWindow`.
pub fn rgba_to_premultiplied_bgra(rgba: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(rgba.len());
    for px in rgba.as_chunks::<4>().0 {
        let a = px[3] as u32;
        let r = (px[0] as u32 * a + 127) / 255;
        let g = (px[1] as u32 * a + 127) / 255;
        let b = (px[2] as u32 * a + 127) / 255;
        out.push(b as u8);
        out.push(g as u8);
        out.push(r as u8);
        out.push(px[3]);
    }
    out
}

/// Encode a `RenderedToast` into a PNG byte buffer.
pub fn encode_png(rendered: &RenderedToast) -> Result<Vec<u8>, String> {
    let expected = (rendered.width as usize)
        .saturating_mul(rendered.height as usize)
        .saturating_mul(4);
    if rendered.rgba.len() != expected {
        return Err(format!(
            "ukuran buffer RGBA tidak cocok: {} != {}",
            rendered.rgba.len(),
            expected
        ));
    }
    let mut buf = Vec::new();
    let encoder = PngEncoder::new(Cursor::new(&mut buf));
    encoder
        .write_image(
            &rendered.rgba,
            rendered.width,
            rendered.height,
            ColorType::Rgba8.into(),
        )
        .map_err(|e| format!("gagal encode PNG toast: {e}"))?;
    Ok(buf)
}

#[inline]
fn blend_over(dst: &mut [u8; 4], src: [u8; 4]) {
    let sa = src[3];
    if sa == 0 {
        return;
    }
    if sa == 255 || dst[3] == 0 {
        *dst = src;
        return;
    }
    let src_a = sa as f32 / 255.0;
    let dst_a = dst[3] as f32 / 255.0;
    let out_a = src_a + dst_a * (1.0 - src_a);
    if out_a <= 0.0005 {
        *dst = [0, 0, 0, 0];
        return;
    }
    let inv_out = 1.0 / out_a;
    let dst_factor = dst_a * (1.0 - src_a);
    for i in 0..3 {
        let c = (src[i] as f32 * src_a + dst[i] as f32 * dst_factor) * inv_out;
        dst[i] = c.round().clamp(0.0, 255.0) as u8;
    }
    dst[3] = (out_a * 255.0).round().clamp(0.0, 255.0) as u8;
}

#[allow(clippy::too_many_arguments)]
fn nine_slice_rgba(
    src: &RgbaImage,
    dst_w: u32,
    dst_h: u32,
    left: u32,
    top: u32,
    right: u32,
    bottom: u32,
) -> Vec<u8> {
    let sw = src.width();
    let sh = src.height();
    let mut out = vec![0u8; (dst_w as usize) * (dst_h as usize) * 4];

    let mid_src_w = sw.saturating_sub(left + right).max(1);
    let mid_src_h = sh.saturating_sub(top + bottom).max(1);
    let mid_dst_w = dst_w.saturating_sub(left + right).max(1);
    let mid_dst_h = dst_h.saturating_sub(top + bottom).max(1);

    for dy in 0..dst_h {
        let sy = if dy < top {
            dy.min(sh - 1)
        } else if dy >= dst_h.saturating_sub(bottom) {
            (sh.saturating_sub(bottom) + (dy - dst_h.saturating_sub(bottom))).min(sh - 1)
        } else {
            let offset = ((dy - top) * mid_src_h) / mid_dst_h;
            (top + offset.min(mid_src_h - 1)).min(sh - 1)
        };

        for dx in 0..dst_w {
            let sx = if dx < left {
                dx.min(sw - 1)
            } else if dx >= dst_w.saturating_sub(right) {
                (sw.saturating_sub(right) + (dx - dst_w.saturating_sub(right))).min(sw - 1)
            } else {
                let offset = ((dx - left) * mid_src_w) / mid_dst_w;
                (left + offset.min(mid_src_w - 1)).min(sw - 1)
            };

            let px = src.get_pixel(sx, sy);
            let idx = ((dy * dst_w + dx) as usize) * 4;
            out[idx..idx + 4].copy_from_slice(&px.0);
        }
    }
    out
}

fn sanitize_single_line(input: &str) -> String {
    input
        .replace(['\r', '\n'], " ")
        .replace(" \u{2014} ", ": ")
        .replace('\u{2014}', ", ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn truncate_line(font: &MinecraftFont, text: &str, max_w: u32) -> String {
    let trimmed = text.trim();
    if font.measure(trimmed) <= max_w {
        return trimmed.to_string();
    }
    let ellipsis = "...";
    let ell_w = font.measure(ellipsis);
    let budget = max_w.saturating_sub(ell_w);
    let mut out = String::new();
    let mut width = 0u32;
    for ch in trimmed.chars() {
        let adv = font.char_advance(ch);
        if width + adv > budget {
            break;
        }
        out.push(ch);
        width += adv;
    }
    while out.ends_with(char::is_whitespace) {
        out.pop();
    }
    out.push_str(ellipsis);
    out
}

fn split_long_token(font: &MinecraftFont, word: &str, max_w: u32) -> Vec<String> {
    let mut pieces = Vec::new();
    let mut current = String::new();
    let mut cur_w = 0u32;
    for ch in word.chars() {
        let adv = font.char_advance(ch);
        if !current.is_empty() && cur_w + adv > max_w {
            pieces.push(std::mem::take(&mut current));
            cur_w = 0;
        }
        current.push(ch);
        cur_w += adv;
    }
    if !current.is_empty() {
        pieces.push(current);
    }
    pieces
}

fn wrap_body_lines(font: &MinecraftFont, text: &str, max_w: u32, max_lines: usize) -> Vec<String> {
    let clean = sanitize_single_line(text);
    let words: Vec<&str> = clean.split_whitespace().collect();
    if words.is_empty() || max_lines == 0 {
        return Vec::new();
    }

    let mut tokens = Vec::with_capacity(words.len());
    for word in words {
        if font.measure(word) <= max_w {
            tokens.push(word.to_string());
        } else {
            tokens.extend(split_long_token(font, word, max_w));
        }
    }

    let mut lines = Vec::new();
    let mut current = String::new();
    let mut idx = 0usize;

    while idx < tokens.len() {
        let tok = &tokens[idx];
        let candidate = if current.is_empty() {
            tok.clone()
        } else {
            format!("{current} {tok}")
        };

        if font.measure(&candidate) <= max_w {
            current = candidate;
            idx += 1;
        } else if lines.len() + 1 == max_lines {
            let remainder = if current.is_empty() {
                tokens[idx..].join(" ")
            } else {
                format!("{current} {}", tokens[idx..].join(" "))
            };
            lines.push(truncate_line(font, &remainder, max_w));
            return lines;
        } else if !current.is_empty() {
            lines.push(std::mem::take(&mut current));
        } else {
            lines.push(truncate_line(font, tok, max_w));
            idx += 1;
        }
    }

    if !current.is_empty() {
        lines.push(current);
    }
    lines
}

fn upscale_nearest_neighbor(src: &[u8], src_w: u32, src_h: u32, scale: u32) -> Vec<u8> {
    let dst_w = src_w * scale;
    let dst_h = src_h * scale;
    let mut dst = vec![0u8; (dst_w as usize) * (dst_h as usize) * 4];

    for sy in 0..src_h {
        for sx in 0..src_w {
            let sidx = ((sy * src_w + sx) as usize) * 4;
            let px = &src[sidx..sidx + 4];
            let base_y = sy * scale;
            let base_x = sx * scale;
            for oy in 0..scale {
                let row_off = ((base_y + oy) * dst_w + base_x) as usize * 4;
                for ox in 0..scale {
                    let didx = row_off + (ox as usize) * 4;
                    dst[didx..didx + 4].copy_from_slice(px);
                }
            }
        }
    }
    dst
}

fn blit_icon_over(
    dst: &mut [u8],
    dst_w: u32,
    dst_h: u32,
    icon_rgba: &[u8],
    icon_size: u32,
    ox: u32,
    oy: u32,
) {
    for sy in 0..icon_size {
        let py = oy + sy;
        if py >= dst_h {
            continue;
        }
        for sx in 0..icon_size {
            let px = ox + sx;
            if px >= dst_w {
                continue;
            }
            let sidx = ((sy * icon_size + sx) as usize) * 4;
            let src_px = [
                icon_rgba[sidx],
                icon_rgba[sidx + 1],
                icon_rgba[sidx + 2],
                icon_rgba[sidx + 3],
            ];
            let didx = ((py * dst_w + px) as usize) * 4;
            let mut cur = [dst[didx], dst[didx + 1], dst[didx + 2], dst[didx + 3]];
            blend_over(&mut cur, src_px);
            dst[didx..didx + 4].copy_from_slice(&cur);
        }
    }
}

/// Render an authentic Minecraft-style toast card into a straight-alpha RGBA8888 bitmap.
pub fn render_toast_card(spec: &ToastCardSpec, scale: f32) -> RenderedToast {
    let pixel_scale = toast_pixel_scale(scale);
    let font = bitmap_font();

    let text_x: u32 = match spec.kind {
        ToastKind::Success | ToastKind::Info => 30,
        ToastKind::Error => 18,
    };
    let max_line_w = MAX_GUI_WIDTH - text_x - TEXT_RIGHT_PAD;

    let clean_title = sanitize_single_line(&spec.title);
    let title = truncate_line(font, &clean_title, max_line_w);
    let body_lines = wrap_body_lines(font, &spec.body, max_line_w, 2);

    let title_w = font.measure(&title);
    let body_max_w = body_lines
        .iter()
        .map(|l| font.measure(l))
        .max()
        .unwrap_or(0);
    let widest_line_w = title_w.max(body_max_w);

    let gui_w = (text_x + widest_line_w + TEXT_RIGHT_PAD).clamp(MIN_GUI_WIDTH, MAX_GUI_WIDTH);
    let has_two_body_lines = body_lines.len() >= 2;
    let gui_h: u32 = match spec.kind {
        ToastKind::Success | ToastKind::Info => {
            if has_two_body_lines {
                42
            } else {
                32
            }
        }
        ToastKind::Error => {
            if has_two_body_lines {
                44
            } else {
                34
            }
        }
    };

    // 1. 9-slice the official Minecraft toast sprite at 1x GUI pixel resolution.
    let mut gui_canvas = match spec.kind {
        ToastKind::Success | ToastKind::Info => {
            nine_slice_rgba(advancement_sprite(), gui_w, gui_h, 5, 5, 5, 5)
        }
        ToastKind::Error => nine_slice_rgba(system_sprite(), gui_w, gui_h, 17, 30, 4, 4),
    };

    // 2. Render yellow title at y = 7 and white body lines at y = 18 / y = 28 with (+1, +1) drop shadow.
    font.draw_text(
        &mut gui_canvas,
        gui_w,
        gui_h,
        text_x as i32,
        7,
        COLOR_TITLE_YELLOW,
        &title,
    );
    for (idx, line) in body_lines.iter().enumerate() {
        let y = 18 + (idx as i32) * 10;
        font.draw_text(
            &mut gui_canvas,
            gui_w,
            gui_h,
            text_x as i32,
            y,
            COLOR_BODY_WHITE,
            line,
        );
    }

    // 3. Exact integer nearest-neighbor upscale so every Minecraft pixel stays razor-sharp.
    let out_w = gui_w * pixel_scale;
    let out_h = gui_h * pixel_scale;
    let mut out_rgba = upscale_nearest_neighbor(&gui_canvas, gui_w, gui_h, pixel_scale);

    // 4. Composite crisp Polban Presensi icon for Success/Info (Error preserves system.png's golden '!').
    if matches!(spec.kind, ToastKind::Success | ToastKind::Info) {
        let icon_px = 16 * pixel_scale;
        let icon_rgba = cached_app_icon_rgba(icon_px);
        let icon_x = 8 * pixel_scale;
        let icon_y = ((gui_h - 16) / 2) * pixel_scale;
        blit_icon_over(
            &mut out_rgba,
            out_w,
            out_h,
            &icon_rgba,
            icon_px,
            icon_x,
            icon_y,
        );
    }

    RenderedToast {
        width: out_w,
        height: out_h,
        rgba: out_rgba,
        card_rect: [0, 0, out_w as i32, out_h as i32],
        close_rect: [0, 0, 0, 0],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_spec(kind: ToastKind, title: &str, body: &str) -> ToastCardSpec {
        ToastCardSpec {
            kind,
            title: title.to_string(),
            body: body.to_string(),
            progress: 1.0,
            close_hovered: false,
            palette: Palette::DARK,
        }
    }

    #[test]
    fn pixel_scale_matches_compact_minecraft_gui_tiers() {
        assert_eq!(toast_pixel_scale(1.0), 2);
        assert_eq!(toast_pixel_scale(1.25), 2);
        assert_eq!(toast_pixel_scale(1.5), 3);
        assert_eq!(toast_pixel_scale(2.5), 4);
        assert_eq!(toast_pixel_scale(f32::NAN), 2);
    }

    #[test]
    fn renders_authentic_minecraft_dimensions_stepped_corners_and_opaque_interior() {
        for scale in [1.0_f32, 1.25_f32, 1.5_f32] {
            let ps = toast_pixel_scale(scale);
            for (kind, title, body, expected_h_1line) in [
                (
                    ToastKind::Success,
                    "Presensi Berhasil!",
                    "Basis Data",
                    32u32,
                ),
                (
                    ToastKind::Info,
                    "Berjalan di Latar Belakang",
                    "Klik ikon di system tray.",
                    32u32,
                ),
                (
                    ToastKind::Error,
                    "Login Portal Gagal!",
                    "Cek NIM & sandi.",
                    34u32,
                ),
            ] {
                let rendered = render_toast_card(&sample_spec(kind, title, body), scale);
                assert_eq!(rendered.height, expected_h_1line * ps);
                assert!(rendered.width >= MIN_GUI_WIDTH * ps);
                assert!(rendered.width <= 184 * ps);
                assert_eq!(rendered.width % ps, 0);
                assert_eq!(rendered.height % ps, 0);
                assert_eq!(rendered.close_rect, [0, 0, 0, 0]);
                assert_eq!(
                    rendered.card_rect,
                    [0, 0, rendered.width as i32, rendered.height as i32]
                );

                // Official advancement.png has stepped transparent corners; system.png has square black corners.
                let corners = [
                    (0, 0),
                    (rendered.width - 1, 0),
                    (0, rendered.height - 1),
                    (rendered.width - 1, rendered.height - 1),
                ];
                let expected_corner_alpha = match kind {
                    ToastKind::Success | ToastKind::Info => 0,
                    ToastKind::Error => system_sprite().get_pixel(0, 0)[3],
                };
                for (cx, cy) in corners {
                    let idx = ((cy * rendered.width + cx) as usize) * 4;
                    assert_eq!(
                        rendered.rgba[idx + 3],
                        expected_corner_alpha,
                        "{kind:?} corner ({cx}, {cy}) alpha mismatch"
                    );
                }

                // Interior center pixel must be fully opaque.
                let mx = rendered.width / 2;
                let my = rendered.height / 2;
                let mid_idx = ((my * rendered.width + mx) as usize) * 4;
                assert_eq!(rendered.rgba[mid_idx + 3], 255);

                // PNG encoding and premultiplied BGRA invariant.
                let png = encode_png(&rendered).expect("PNG encoding must succeed");
                assert!(png.starts_with(b"\x89PNG\r\n\x1a\n"));
                let bgra = rgba_to_premultiplied_bgra(&rendered.rgba);
                assert_eq!(bgra.len(), rendered.rgba.len());
                for px in bgra.as_chunks::<4>().0 {
                    assert!(px[0] <= px[3] && px[1] <= px[3] && px[2] <= px[3]);
                }
            }
        }
    }

    #[test]
    fn title_has_minecraft_yellow_and_drop_shadow_pixels_with_crisp_nearest_neighbor_blocks() {
        let ps = toast_pixel_scale(1.0); // 2
        let rendered = render_toast_card(
            &sample_spec(
                ToastKind::Success,
                "Presensi Berhasil!",
                "Pengantar Rekayasa Perangkat Lunak",
            ),
            1.0,
        );
        // Two-line body expands advancement toast to 42 GUI px tall.
        assert_eq!(rendered.height, 42 * ps);

        let yellow_shadow = [
            COLOR_TITLE_YELLOW[0] / 4,
            COLOR_TITLE_YELLOW[1] / 4,
            COLOR_TITLE_YELLOW[2] / 4,
            255,
        ];
        let mut yellow_count = 0usize;
        let mut shadow_count = 0usize;

        // Inspect title band (y = 7..16 GUI px, x >= 30 GUI px).
        for gy in 7..16u32 {
            for gx in 30..(rendered.width / ps) {
                let base_idx = (((gy * ps) * rendered.width + (gx * ps)) as usize) * 4;
                let base_px = &rendered.rgba[base_idx..base_idx + 4];
                if base_px == COLOR_TITLE_YELLOW {
                    yellow_count += 1;
                } else if base_px == yellow_shadow {
                    shadow_count += 1;
                }
                // Verify exact nearest-neighbor uniformity across every pixel_scale x pixel_scale block.
                for oy in 0..ps {
                    for ox in 0..ps {
                        let idx = (((gy * ps + oy) * rendered.width + (gx * ps + ox)) as usize) * 4;
                        assert_eq!(
                            &rendered.rgba[idx..idx + 4],
                            base_px,
                            "non-uniform upscaled pixel block at GUI ({gx}, {gy})"
                        );
                    }
                }
            }
        }
        assert!(yellow_count > 40, "expected Minecraft yellow title pixels");
        assert!(shadow_count > 10, "expected (+1, +1) drop shadow pixels");
    }

    #[test]
    fn error_toast_preserves_system_sprite_golden_exclamation_icon_and_point_in_rect() {
        let ps = toast_pixel_scale(1.0);
        let rendered = render_toast_card(
            &sample_spec(
                ToastKind::Error,
                "Login Portal Gagal!",
                "Cek NIM & sandi di Pengaturan.",
            ),
            1.0,
        );
        // Inspect the left icon band (x = 4..16 GUI px, y = 4..28 GUI px) for golden '!' pixels.
        let mut golden_pixels = 0usize;
        for gy in 4..28u32 {
            for gx in 4..16u32 {
                let idx = (((gy * ps) * rendered.width + (gx * ps)) as usize) * 4;
                let r = rendered.rgba[idx];
                let g = rendered.rgba[idx + 1];
                let b = rendered.rgba[idx + 2];
                let a = rendered.rgba[idx + 3];
                if a == 255 && r > 180 && g > 140 && b < 120 {
                    golden_pixels += 1;
                }
            }
        }
        assert!(
            golden_pixels >= 8,
            "expected system.png golden '!' icon pixels, found {golden_pixels}"
        );

        assert!(point_in_rect(0, 0, rendered.card_rect));
        assert!(point_in_rect(
            rendered.width as i32 - 1,
            rendered.height as i32 - 1,
            rendered.card_rect
        ));
        assert!(!point_in_rect(-1, 0, rendered.card_rect));
        assert!(!point_in_rect(0, 0, rendered.close_rect));
    }
}
