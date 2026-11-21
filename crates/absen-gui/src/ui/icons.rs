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

/// Compute closed crescent moon outline points inside `rect` using exact circle-circle intersection.
fn moon_outline_points(rect: Rect) -> Vec<Pos2> {
    let dim = rect.width().min(rect.height());
    let radius = dim * 0.37;
    let cut_dist = dim * 0.23;
    let cut_radius = dim * 0.31;
    let cut_angle = -0.65_f32;
    let ux = cut_angle.cos();
    let uy = cut_angle.sin();

    // Shift outer center slightly toward the cutout so the crescent sits optically centered.
    let c1 = Pos2::new(rect.center().x + dim * 0.05, rect.center().y - dim * 0.03);
    let c2 = Pos2::new(c1.x + cut_dist * ux, c1.y + cut_dist * uy);

    let a = (radius * radius - cut_radius * cut_radius + cut_dist * cut_dist) / (2.0 * cut_dist);
    let h = (radius * radius - a * a).max(0.0).sqrt();
    let mid = Pos2::new(c1.x + a * ux, c1.y + a * uy);
    let vx = -uy;
    let vy = ux;

    let p_start = Pos2::new(mid.x + h * vx, mid.y + h * vy);
    let p_end = Pos2::new(mid.x - h * vx, mid.y - h * vy);

    let tau = std::f32::consts::TAU;
    let a0 = (p_start.y - c1.y).atan2(p_start.x - c1.x);
    let a1 = (p_end.y - c1.y).atan2(p_end.x - c1.x);
    let pos_delta_out = (a1 - a0).rem_euclid(tau);
    let mid_out = a0 + pos_delta_out * 0.5;
    let outer_sweep = if mid_out.cos() * ux + mid_out.sin() * uy < 0.0 {
        pos_delta_out
    } else {
        pos_delta_out - tau
    };

    let b0 = (p_end.y - c2.y).atan2(p_end.x - c2.x);
    let b1 = (p_start.y - c2.y).atan2(p_start.x - c2.x);
    let pos_delta_in = (b1 - b0).rem_euclid(tau);
    let mid_in = b0 + pos_delta_in * 0.5;
    let inner_sweep = if mid_in.cos() * ux + mid_in.sin() * uy < 0.0 {
        pos_delta_in
    } else {
        pos_delta_in - tau
    };

    let outer_steps = 18;
    let inner_steps = 14;
    let mut pts = Vec::with_capacity(outer_steps + inner_steps);
    for i in 0..=outer_steps {
        let t = i as f32 / outer_steps as f32;
        let angle = a0 + outer_sweep * t;
        pts.push(Pos2::new(
            c1.x + radius * angle.cos(),
            c1.y + radius * angle.sin(),
        ));
    }
    for i in 1..inner_steps {
        let t = i as f32 / inner_steps as f32;
        let angle = b0 + inner_sweep * t;
        pts.push(Pos2::new(
            c2.x + cut_radius * angle.cos(),
            c2.y + cut_radius * angle.sin(),
        ));
    }
    pts
}

/// Draw a crescent moon icon (dark theme) inside `rect`.
pub fn draw_moon(painter: &egui::Painter, rect: Rect, color: Color32) {
    let pts = moon_outline_points(rect);
    painter.add(egui::Shape::closed_line(pts, Stroke::new(1.5_f32, color)));
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
    let mut shackle = Vec::with_capacity(11);
    shackle.push(Pos2::new(shackle_x0, body.top()));
    for i in 0..=8 {
        let t = i as f32 / 8.0;
        let a = std::f32::consts::PI * (1.0 - t);
        shackle.push(Pos2::new(
            r.center().x + (shackle_w * 0.5) * a.cos(),
            body.top() - r.height() * 0.14 - (r.height() * 0.22) * a.sin(),
        ));
    }
    shackle.push(Pos2::new(shackle_x1, body.top()));
    painter.add(egui::Shape::line(shackle, stroke));
    let _ = shackle_top;
}

/// Draw a crisp `X` close icon inside `rect`.
pub fn draw_close(painter: &egui::Painter, rect: Rect, color: Color32) {
    let r = rect.shrink(rect.width().min(rect.height()) * 0.24);
    let stroke = Stroke::new(1.7_f32, color);
    painter.line_segment([r.left_top(), r.right_bottom()], stroke);
    painter.line_segment([r.right_top(), r.left_bottom()], stroke);
}

/// Draw two overlapping document rectangles representing clipboard copy inside `rect`.
pub fn draw_copy(painter: &egui::Painter, rect: Rect, color: Color32) {
    let r = rect.shrink(rect.width().min(rect.height()) * 0.18);
    let stroke = Stroke::new(1.4_f32, color);
    let front = Rect::from_min_max(
        Pos2::new(r.left() + r.width() * 0.24, r.top() + r.height() * 0.24),
        r.max,
    );
    painter.rect_stroke(front, 1.5, stroke);
    let back_pts = vec![
        Pos2::new(r.left() + r.width() * 0.14, r.top() + r.height() * 0.72),
        Pos2::new(r.left(), r.top() + r.height() * 0.72),
        r.min,
        Pos2::new(r.left() + r.width() * 0.72, r.top()),
        Pos2::new(r.left() + r.width() * 0.72, r.top() + r.height() * 0.14),
    ];
    painter.add(egui::Shape::line(back_pts, stroke));
}

/// Draw an external link box + top-right arrow inside `rect`.
pub fn draw_external_link(painter: &egui::Painter, rect: Rect, color: Color32) {
    let r = rect.shrink(rect.width().min(rect.height()) * 0.18);
    let stroke = Stroke::new(1.4_f32, color);
    let box_pts = vec![
        Pos2::new(r.left() + r.width() * 0.45, r.top() + r.height() * 0.18),
        Pos2::new(r.left(), r.top() + r.height() * 0.18),
        r.left_bottom(),
        Pos2::new(r.right() - r.width() * 0.18, r.bottom()),
        Pos2::new(r.right() - r.width() * 0.18, r.top() + r.height() * 0.55),
    ];
    painter.add(egui::Shape::line(box_pts, stroke));
    let tip = r.right_top();
    painter.line_segment(
        [
            Pos2::new(
                r.center().x - r.width() * 0.05,
                r.center().y + r.height() * 0.05,
            ),
            tip,
        ],
        stroke,
    );
    painter.line_segment([Pos2::new(tip.x - r.width() * 0.34, tip.y), tip], stroke);
    painter.line_segment([tip, Pos2::new(tip.x, tip.y + r.height() * 0.34)], stroke);
}

/// Draw a shield with a crisp checkmark inside `rect` (attendance proof icon).
pub fn draw_shield_check(painter: &egui::Painter, rect: Rect, color: Color32) {
    let r = rect.shrink(rect.width().min(rect.height()) * 0.14);
    let stroke = Stroke::new(1.5_f32, color);
    let shield = vec![
        Pos2::new(r.center().x, r.top()),
        Pos2::new(r.right(), r.top() + r.height() * 0.18),
        Pos2::new(r.right() - r.width() * 0.05, r.top() + r.height() * 0.56),
        Pos2::new(r.center().x, r.bottom()),
        Pos2::new(r.left() + r.width() * 0.05, r.top() + r.height() * 0.56),
        Pos2::new(r.left(), r.top() + r.height() * 0.18),
    ];
    painter.add(egui::Shape::closed_line(shield, stroke));

    let c0 = Pos2::new(
        r.left() + r.width() * 0.28,
        r.center().y + r.height() * 0.02,
    );
    let c1 = Pos2::new(r.left() + r.width() * 0.44, r.top() + r.height() * 0.65);
    let c2 = Pos2::new(r.right() - r.width() * 0.27, r.top() + r.height() * 0.34);
    painter.add(egui::Shape::line(vec![c0, c1, c2], stroke));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn segments_intersect(a: Pos2, b: Pos2, c: Pos2, d: Pos2) -> bool {
        fn orient(p: Pos2, q: Pos2, r: Pos2) -> f32 {
            (q.x - p.x) * (r.y - p.y) - (q.y - p.y) * (r.x - p.x)
        }
        let o1 = orient(a, b, c);
        let o2 = orient(a, b, d);
        let o3 = orient(c, d, a);
        let o4 = orient(c, d, b);
        (o1 * o2 < 0.0) && (o3 * o4 < 0.0)
    }

    #[test]
    fn moon_outline_forms_simple_closed_crescent_within_bounds() {
        let rect = Rect::from_min_size(Pos2::ZERO, Vec2::splat(16.0));
        let pts = moon_outline_points(rect);
        assert!(pts.len() >= 24);

        for p in &pts {
            assert!(
                rect.contains(*p),
                "moon point {p:?} fell outside icon rect {rect:?}"
            );
        }

        // Verify exact cusp intersection at start and outer-arc end (index 18).
        let c1 = Pos2::new(rect.center().x + 16.0 * 0.05, rect.center().y - 16.0 * 0.03);
        let cut_angle = -0.65_f32;
        let c2 = Pos2::new(
            c1.x + 16.0 * 0.23 * cut_angle.cos(),
            c1.y + 16.0 * 0.23 * cut_angle.sin(),
        );
        for cusp in [pts[0], pts[18]] {
            let d1 = (cusp - c1).length();
            let d2 = (cusp - c2).length();
            assert!((d1 - 16.0 * 0.37).abs() < 1e-3);
            assert!((d2 - 16.0 * 0.31).abs() < 1e-3);
        }

        // Verify the closed loop has zero non-adjacent self-intersections.
        let n = pts.len();
        for i in 0..n {
            let a = pts[i];
            let b = pts[(i + 1) % n];
            for j in (i + 2)..n {
                if i == 0 && j == n - 1 {
                    continue;
                }
                let c = pts[j];
                let d = pts[(j + 1) % n];
                assert!(
                    !segments_intersect(a, b, c, d),
                    "moon outline self-intersects between edge {i} and edge {j}"
                );
            }
        }
    }
}
