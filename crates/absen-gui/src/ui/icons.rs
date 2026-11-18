//! Procedural vector icons drawn with `egui::Painter`.

use egui::{Color32, Pos2, Rect, Stroke, Vec2};

/// Draw a small status indicator dot with a crisp contrast ring.
pub fn draw_status_dot(ui: &mut egui::Ui, fill: Color32, ring: Color32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(10.0), egui::Sense::hover());
    let painter = ui.painter();
    let center = rect.center();
    painter.circle_filled(center, 4.5, ring);
    painter.circle_filled(center, 3.5, fill);
}

/// Draw a play triangle icon inside `rect`.
pub fn draw_play(painter: &egui::Painter, rect: Rect, color: Color32) {
    let r = rect.shrink(rect.width() * 0.22);
    let points = vec![
        Pos2::new(r.left(), r.top()),
        Pos2::new(r.right(), r.center().y),
        Pos2::new(r.left(), r.bottom()),
    ];
    painter.add(egui::Shape::convex_polygon(points, color, Stroke::NONE));
}

/// Draw a stop square icon inside `rect`.
pub fn draw_stop(painter: &egui::Painter, rect: Rect, color: Color32) {
    let r = rect.shrink(rect.width() * 0.25);
    painter.rect_filled(r, 1.5, color);
}

/// Draw a refresh/scan circular arrow stroke inside `rect`.
pub fn draw_refresh(painter: &egui::Painter, rect: Rect, color: Color32) {
    let center = rect.center();
    let radius = rect.width().min(rect.height()) * 0.34;
    let stroke = Stroke::new(1.5_f32, color);
    let segments = 16;
    let start_angle = 0.35_f32;
    let sweep = std::f32::consts::TAU * 0.78;
    let pts: Vec<Pos2> = (0..=segments)
        .map(|i| {
            let t = i as f32 / segments as f32;
            let a = start_angle + t * sweep;
            Pos2::new(center.x + radius * a.cos(), center.y + radius * a.sin())
        })
        .collect();
    painter.add(egui::Shape::line(pts, stroke));
}

/// Draw a gear/settings outline inside `rect`.
pub fn draw_settings(painter: &egui::Painter, rect: Rect, color: Color32) {
    let center = rect.center();
    let radius = rect.width().min(rect.height()) * 0.28;
    painter.circle_stroke(center, radius, Stroke::new(1.5_f32, color));
    painter.circle_filled(center, radius * 0.35, color);
}
