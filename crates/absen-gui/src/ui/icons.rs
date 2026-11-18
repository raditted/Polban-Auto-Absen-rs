//! Procedural vector icons drawn with `egui::Painter` (no decorative emojis).

use egui::{Color32, Pos2, Rect, Stroke, Vec2};

/// Allocate an exact square icon slot and invoke `draw_fn`.
pub fn paint_icon(
    ui: &mut egui::Ui,
    size: f32,
    color: Color32,
    draw_fn: impl FnOnce(&egui::Painter, Rect, Color32),
) -> Rect {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(size), egui::Sense::hover());
    draw_fn(ui.painter(), rect, color);
    rect
}

/// Draw a small status indicator dot with a crisp contrast ring.
pub fn draw_status_dot(ui: &mut egui::Ui, fill: Color32, ring: Color32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(10.0), egui::Sense::hover());
    let painter = ui.painter();
    let center = rect.center();
    painter.circle_filled(center, 4.5, ring);
    painter.circle_filled(center, 3.2, fill);
}

/// Draw a play triangle icon inside `rect`.
pub fn draw_play(painter: &egui::Painter, rect: Rect, color: Color32) {
    let r = rect.shrink(rect.width().min(rect.height()) * 0.22);
    let points = vec![
        Pos2::new(r.left() + r.width() * 0.1, r.top()),
        Pos2::new(r.right(), r.center().y),
        Pos2::new(r.left() + r.width() * 0.1, r.bottom()),
    ];
    painter.add(egui::Shape::convex_polygon(points, color, Stroke::NONE));
}

/// Draw a stop square icon inside `rect`.
pub fn draw_stop(painter: &egui::Painter, rect: Rect, color: Color32) {
    let r = rect.shrink(rect.width().min(rect.height()) * 0.25);
    painter.rect_filled(r, 2.0, color);
}

/// Draw a refresh/scan circular arrow stroke inside `rect`.
pub fn draw_refresh(painter: &egui::Painter, rect: Rect, color: Color32) {
    let center = rect.center();
    let radius = rect.width().min(rect.height()) * 0.34;
    let stroke = Stroke::new(1.5_f32, color);
    let segments = 18;
    let start_angle = -0.35_f32;
    let sweep = std::f32::consts::TAU * 0.78;
    let pts: Vec<Pos2> = (0..=segments)
        .map(|i| {
            let t = i as f32 / segments as f32;
            let a = start_angle + t * sweep;
            Pos2::new(center.x + radius * a.cos(), center.y + radius * a.sin())
        })
        .collect();
    if let Some(&tip) = pts.first() {
        let arrow = radius * 0.42;
        painter.line_segment(
            [tip, Pos2::new(tip.x - arrow, tip.y - arrow * 0.15)],
            stroke,
        );
        painter.line_segment(
            [tip, Pos2::new(tip.x + arrow * 0.15, tip.y - arrow)],
            stroke,
        );
    }
    painter.add(egui::Shape::line(pts, stroke));
}

/// Draw a gear/settings icon inside `rect`.
pub fn draw_settings(painter: &egui::Painter, rect: Rect, color: Color32) {
    let center = rect.center();
    let dim = rect.width().min(rect.height());
    let outer = dim * 0.38;
    let inner = dim * 0.27;
    let stroke = Stroke::new(1.5_f32, color);
    painter.circle_stroke(center, inner, stroke);
    painter.circle_stroke(center, dim * 0.10, stroke);
    for i in 0..8 {
        let angle = (i as f32) * std::f32::consts::FRAC_PI_4;
        let p0 = Pos2::new(
            center.x + inner * angle.cos(),
            center.y + inner * angle.sin(),
        );
        let p1 = Pos2::new(
            center.x + outer * angle.cos(),
            center.y + outer * angle.sin(),
        );
        painter.line_segment([p0, p1], stroke);
    }
}

/// Draw a sun icon (light theme) inside `rect`.
pub fn draw_sun(painter: &egui::Painter, rect: Rect, color: Color32) {
    let center = rect.center();
    let dim = rect.width().min(rect.height());
    let stroke = Stroke::new(1.5_f32, color);
    painter.circle_stroke(center, dim * 0.19, stroke);
    let r0 = dim * 0.28;
    let r1 = dim * 0.40;
    for i in 0..8 {
        let angle = (i as f32) * std::f32::consts::FRAC_PI_4;
        let p0 = Pos2::new(center.x + r0 * angle.cos(), center.y + r0 * angle.sin());
        let p1 = Pos2::new(center.x + r1 * angle.cos(), center.y + r1 * angle.sin());
        painter.line_segment([p0, p1], stroke);
    }
}

/// Draw a crescent moon icon (dark theme) inside `rect`.
pub fn draw_moon(painter: &egui::Painter, rect: Rect, color: Color32) {
    let center = rect.center();
    let radius = rect.width().min(rect.height()) * 0.34;
    let stroke = Stroke::new(1.5_f32, color);
    let outer_pts: Vec<Pos2> = (0..=14)
        .map(|i| {
            let t = i as f32 / 14.0;
            let a = -1.1 + t * (std::f32::consts::PI * 1.35);
            Pos2::new(center.x + radius * a.cos(), center.y + radius * a.sin())
        })
        .collect();
    let inner_center = Pos2::new(center.x + radius * 0.38, center.y - radius * 0.22);
    let inner_r = radius * 0.78;
    let inner_pts: Vec<Pos2> = (0..=10)
        .map(|i| {
            let t = i as f32 / 10.0;
            let a = std::f32::consts::PI * 0.85 - t * (std::f32::consts::PI * 0.95);
            Pos2::new(
                inner_center.x + inner_r * a.cos(),
                inner_center.y + inner_r * a.sin(),
            )
        })
        .collect();
    painter.add(egui::Shape::line(outer_pts, stroke));
    painter.add(egui::Shape::line(inner_pts, stroke));
}

/// Draw a desktop monitor icon (system theme) inside `rect`.
pub fn draw_monitor(painter: &egui::Painter, rect: Rect, color: Color32) {
    let dim = rect.width().min(rect.height());
    let stroke = Stroke::new(1.5_f32, color);
    let c = rect.center();
    let screen = Rect::from_center_size(
        Pos2::new(c.x, c.y - dim * 0.08),
        Vec2::new(dim * 0.68, dim * 0.48),
    );
    painter.rect_stroke(screen, 2.0, stroke);
    painter.line_segment(
        [
            Pos2::new(c.x, screen.bottom()),
            Pos2::new(c.x, screen.bottom() + dim * 0.14),
        ],
        stroke,
    );
    painter.line_segment(
        [
            Pos2::new(c.x - dim * 0.18, screen.bottom() + dim * 0.14),
            Pos2::new(c.x + dim * 0.18, screen.bottom() + dim * 0.14),
        ],
        stroke,
    );
}

/// Draw an open eye icon inside `rect`.
pub fn draw_eye(painter: &egui::Painter, rect: Rect, color: Color32) {
    let c = rect.center();
    let dim = rect.width().min(rect.height());
    let stroke = Stroke::new(1.5_f32, color);
    let half_w = dim * 0.38;
    let half_h = dim * 0.22;
    let top: Vec<Pos2> = (0..=10)
        .map(|i| {
            let t = i as f32 / 10.0;
            let x = c.x - half_w + t * (2.0 * half_w);
            let arch = (t * std::f32::consts::PI).sin() * half_h;
            Pos2::new(x, c.y - arch)
        })
        .collect();
    let bot: Vec<Pos2> = (0..=10)
        .map(|i| {
            let t = i as f32 / 10.0;
            let x = c.x - half_w + t * (2.0 * half_w);
            let arch = (t * std::f32::consts::PI).sin() * half_h;
            Pos2::new(x, c.y + arch)
        })
        .collect();
    painter.add(egui::Shape::line(top, stroke));
    painter.add(egui::Shape::line(bot, stroke));
    painter.circle_filled(c, dim * 0.10, color);
}

/// Draw an eye-off (slashed eye) icon inside `rect`.
pub fn draw_eye_off(painter: &egui::Painter, rect: Rect, color: Color32) {
    draw_eye(painter, rect, color);
    let r = rect.shrink(rect.width().min(rect.height()) * 0.18);
    painter.line_segment(
        [r.left_bottom(), r.right_top()],
        Stroke::new(1.6_f32, color),
    );
}

/// Draw a checkmark icon inside `rect`.
pub fn draw_check(painter: &egui::Painter, rect: Rect, color: Color32) {
    let r = rect.shrink(rect.width().min(rect.height()) * 0.20);
    let stroke = Stroke::new(1.8_f32, color);
    let p0 = Pos2::new(r.left(), r.center().y + r.height() * 0.05);
    let p1 = Pos2::new(r.left() + r.width() * 0.38, r.bottom() - r.height() * 0.12);
    let p2 = Pos2::new(r.right(), r.top() + r.height() * 0.12);
    painter.add(egui::Shape::line(vec![p0, p1, p2], stroke));
}

/// Draw a clock icon inside `rect`.
pub fn draw_clock(painter: &egui::Painter, rect: Rect, color: Color32) {
    let c = rect.center();
    let radius = rect.width().min(rect.height()) * 0.36;
    let stroke = Stroke::new(1.5_f32, color);
    painter.circle_stroke(c, radius, stroke);
    painter.line_segment([c, Pos2::new(c.x, c.y - radius * 0.52)], stroke);
    painter.line_segment(
        [c, Pos2::new(c.x + radius * 0.44, c.y + radius * 0.18)],
        stroke,
    );
}

/// Draw an alert triangle with exclamation mark inside `rect`.
pub fn draw_alert(painter: &egui::Painter, rect: Rect, color: Color32) {
    let r = rect.shrink(rect.width().min(rect.height()) * 0.16);
    let stroke = Stroke::new(1.5_f32, color);
    let top = Pos2::new(r.center().x, r.top());
    let left = Pos2::new(r.left(), r.bottom());
    let right = Pos2::new(r.right(), r.bottom());
    painter.add(egui::Shape::closed_line(vec![top, right, left], stroke));
    painter.line_segment(
        [
            Pos2::new(r.center().x, r.top() + r.height() * 0.34),
            Pos2::new(r.center().x, r.top() + r.height() * 0.64),
        ],
        stroke,
    );
    painter.circle_filled(
        Pos2::new(r.center().x, r.top() + r.height() * 0.80),
        1.2,
        color,
    );
}

/// Draw a folder icon (for importing `.env`) inside `rect`.
pub fn draw_folder(painter: &egui::Painter, rect: Rect, color: Color32) {
    let r = rect.shrink(rect.width().min(rect.height()) * 0.16);
    let stroke = Stroke::new(1.5_f32, color);
    let tab_y = r.top() + r.height() * 0.22;
    let pts = vec![
        Pos2::new(r.left(), r.top()),
        Pos2::new(r.left() + r.width() * 0.42, r.top()),
        Pos2::new(r.left() + r.width() * 0.52, tab_y),
        Pos2::new(r.right(), tab_y),
        Pos2::new(r.right(), r.bottom()),
        Pos2::new(r.left(), r.bottom()),
    ];
    painter.add(egui::Shape::closed_line(pts, stroke));
}

/// Draw a download / save file icon (for diagnostic HTML export) inside `rect`.
pub fn draw_download(painter: &egui::Painter, rect: Rect, color: Color32) {
    let r = rect.shrink(rect.width().min(rect.height()) * 0.18);
    let stroke = Stroke::new(1.5_f32, color);
    let cx = r.center().x;
    let tip_y = r.top() + r.height() * 0.68;
    painter.line_segment([Pos2::new(cx, r.top()), Pos2::new(cx, tip_y)], stroke);
    let wing = r.width() * 0.24;
    painter.line_segment(
        [Pos2::new(cx - wing, tip_y - wing), Pos2::new(cx, tip_y)],
        stroke,
    );
    painter.line_segment(
        [Pos2::new(cx + wing, tip_y - wing), Pos2::new(cx, tip_y)],
        stroke,
    );
    let tray = vec![
        Pos2::new(r.left(), r.bottom() - r.height() * 0.22),
        Pos2::new(r.left(), r.bottom()),
        Pos2::new(r.right(), r.bottom()),
        Pos2::new(r.right(), r.bottom() - r.height() * 0.22),
    ];
    painter.add(egui::Shape::line(tray, stroke));
}

/// Draw a padlock icon (for credential store security badge) inside `rect`.
pub fn draw_lock(painter: &egui::Painter, rect: Rect, color: Color32) {
    let r = rect.shrink(rect.width().min(rect.height()) * 0.18);
    let stroke = Stroke::new(1.5_f32, color);
    let body = Rect::from_min_max(
        Pos2::new(r.left() + r.width() * 0.12, r.top() + r.height() * 0.42),
        Pos2::new(r.right() - r.width() * 0.12, r.bottom()),
    );
    painter.rect_stroke(body, 2.0, stroke);
    let shackle_w = body.width() * 0.56;
    let shackle_x0 = r.center().x - shackle_w * 0.5;
    let shackle_x1 = r.center().x + shackle_w * 0.5;
    let shackle_top = r.top() + r.height() * 0.06;
    let arch: Vec<Pos2> = (0..=8)
        .map(|i| {
            let t = i as f32 / 8.0;
            let a = std::f32::consts::PI * (1.0 - t);
            Pos2::new(
                r.center().x + (shackle_w * 0.5) * a.cos(),
                body.top() - r.height() * 0.14 - (r.height() * 0.22) * a.sin(),
            )
        })
        .collect();
    painter.line_segment([Pos2::new(shackle_x0, body.top()), arch[0]], stroke);
    painter.add(egui::Shape::line(arch.clone(), stroke));
    if let Some(&last) = arch.last() {
        painter.line_segment([last, Pos2::new(shackle_x1, body.top())], stroke);
    }
    let _ = shackle_top;
}
