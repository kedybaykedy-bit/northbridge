use egui::{Color32, CornerRadius, Margin, Shadow, Stroke, Style, TextStyle, Visuals};

pub const K0: Color32 = Color32::from_rgb(0x00, 0x00, 0x00);
pub const K1: Color32 = Color32::from_rgb(0x0a, 0x0a, 0x0a);
pub const K2: Color32 = Color32::from_rgb(0x1a, 0x1a, 0x1a);
pub const K3: Color32 = Color32::from_rgb(0x26, 0x26, 0x26);
pub const G4: Color32 = Color32::from_rgb(0x4a, 0x4a, 0x4a);
pub const G5: Color32 = Color32::from_rgb(0x6a, 0x6a, 0x6a);
pub const G6: Color32 = Color32::from_rgb(0x8a, 0x8a, 0x8a);
pub const G7: Color32 = Color32::from_rgb(0x9a, 0x9a, 0x9a);
pub const G8: Color32 = Color32::from_rgb(0xc0, 0xc0, 0xc0);
pub const G9: Color32 = Color32::from_rgb(0xcf, 0xcf, 0xcf);
pub const W: Color32 = Color32::from_rgb(0xf2, 0xf2, 0xf2);

pub const SKEW_TAN: f32 = 0.176_3;
pub const TILE_GAP: f32 = 12.0;
pub const TILE_MIN: f32 = 150.0;
pub const TILE_H: f32 = 96.0;
pub const SHADOW_DX: f32 = 6.0;
pub const SHADOW_DY: f32 = 6.0;

pub fn paper_frame() -> egui::Frame {
    egui::Frame::new()
        .fill(K1)
        .stroke(Stroke::new(2.0, W))
        .inner_margin(Margin::same(14))
        .corner_radius(CornerRadius::ZERO)
        .shadow(Shadow {
            offset: [12, 12],
            blur: 0,
            spread: 0,
            color: W,
        })
}

pub fn mini_frame() -> egui::Frame {
    egui::Frame::new()
        .fill(K1)
        .stroke(Stroke::new(1.0, W))
        .inner_margin(Margin::same(10))
        .corner_radius(CornerRadius::ZERO)
        .shadow(Shadow {
            offset: [6, 6],
            blur: 0,
            spread: 0,
            color: G4,
        })
}

pub fn band_frame() -> egui::Frame {
    egui::Frame::new()
        .fill(K0)
        .stroke(Stroke::new(1.0, G5))
        .inner_margin(Margin::same(10))
        .corner_radius(CornerRadius::ZERO)
        .shadow(Shadow {
            offset: [0, 0],
            blur: 0,
            spread: 0,
            color: Color32::TRANSPARENT,
        })
}

pub fn apply(ctx: &egui::Context) {
    let mut v = Visuals::dark();
    v.dark_mode = true;
    v.override_text_color = None;
    v.window_fill = K0;
    v.panel_fill = K0;
    v.faint_bg_color = K2;
    v.extreme_bg_color = K3;
    v.code_bg_color = K1;
    v.warn_fg_color = G8;
    v.error_fg_color = W;
    v.window_stroke = Stroke::new(1.0, G4);
    v.window_shadow = Shadow {
        offset: [6, 6],
        blur: 0,
        spread: 0,
        color: G4,
    };
    v.popup_shadow = Shadow {
        offset: [6, 6],
        blur: 0,
        spread: 0,
        color: W,
    };
    v.selection.bg_fill = W;
    v.selection.stroke = Stroke::new(1.0, K0);
    v.widgets.noninteractive.bg_fill = K1;
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, G4);
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, G8);
    v.widgets.inactive.bg_fill = K2;
    v.widgets.inactive.bg_stroke = Stroke::new(1.0, W);
    v.widgets.inactive.fg_stroke = Stroke::new(1.0, G8);
    v.widgets.hovered.bg_fill = W;
    v.widgets.hovered.bg_stroke = Stroke::new(1.0, W);
    v.widgets.hovered.fg_stroke = Stroke::new(1.5, K0);
    v.widgets.hovered.expansion = 0.0;
    v.widgets.active.bg_fill = W;
    v.widgets.active.bg_stroke = Stroke::new(1.0, W);
    v.widgets.active.fg_stroke = Stroke::new(1.5, K0);
    v.widgets.active.expansion = 0.0;
    v.widgets.open.bg_fill = K2;
    v.widgets.open.bg_stroke = Stroke::new(1.0, W);
    v.widgets.open.fg_stroke = Stroke::new(1.0, W);
    v.widgets.open.expansion = 0.0;
    ctx.set_visuals(v);
    ctx.global_style_mut(|s: &mut Style| {
        for corner in [
            &mut s.visuals.widgets.noninteractive.corner_radius,
            &mut s.visuals.widgets.inactive.corner_radius,
            &mut s.visuals.widgets.hovered.corner_radius,
            &mut s.visuals.widgets.active.corner_radius,
            &mut s.visuals.widgets.open.corner_radius,
        ] {
            *corner = CornerRadius::ZERO;
        }
        s.spacing.button_padding = egui::vec2(14.0, 8.0);
        s.spacing.item_spacing = egui::vec2(10.0, 8.0);
        s.spacing.indent = 18.0;
        if let Some(body) = s.text_styles.get_mut(&TextStyle::Body) {
            body.size = 14.0;
        }
        if let Some(small) = s.text_styles.get_mut(&TextStyle::Small) {
            small.size = 12.0;
        }
        if let Some(mono) = s.text_styles.get_mut(&TextStyle::Monospace) {
            mono.size = 12.5;
        }
        if let Some(btn) = s.text_styles.get_mut(&TextStyle::Button) {
            btn.size = 14.0;
        }
        if let Some(h) = s.text_styles.get_mut(&TextStyle::Heading) {
            h.size = 26.0;
        }
    });
}

pub fn kicker(text: &str) -> egui::RichText {
    egui::RichText::new(text.to_uppercase())
        .size(11.0)
        .color(G7)
        .strong()
}

pub fn title(text: &str) -> egui::RichText {
    egui::RichText::new(text.to_uppercase()).size(26.0).color(W).strong()
}

pub fn body(text: &str) -> egui::RichText {
    egui::RichText::new(text).size(14.0).color(G9)
}

pub fn meta(text: &str) -> egui::RichText {
    egui::RichText::new(text).size(12.0).color(G6)
}

pub fn mono(text: &str) -> egui::RichText {
    egui::RichText::new(text).monospace().size(12.5).color(G8)
}

pub fn chip(text: &str) -> egui::RichText {
    egui::RichText::new(text.to_uppercase()).size(11.0).color(W).strong()
}

pub fn slant_for(height: f32) -> f32 {
    height * SKEW_TAN * 0.5
}

pub fn skewed_quad(rect: egui::Rect, slant: f32) -> Vec<egui::Pos2> {
    vec![
        egui::pos2(rect.min.x + slant, rect.min.y),
        egui::pos2(rect.max.x + slant, rect.min.y),
        egui::pos2(rect.max.x - slant, rect.max.y),
        egui::pos2(rect.min.x - slant, rect.max.y),
    ]
}

pub fn tile_columns(avail: f32, min_tile: f32, gap: f32) -> usize {
    if avail <= 0.0 {
        return 1;
    }
    let step = min_tile + gap;
    let n = ((avail + gap) / step).floor() as usize;
    n.clamp(1, 4)
}

pub fn tile_width(avail: f32, cols: usize, gap: f32) -> f32 {
    if cols == 0 {
        return avail;
    }
    ((avail - gap * (cols as f32 - 1.0)) / cols as f32).clamp(120.0, 240.0)
}

pub fn title_size_for(label: &str) -> f32 {
    let n = label.chars().count();
    if n > 9 {
        15.0
    } else if n > 6 {
        17.0
    } else {
        19.0
    }
}

pub fn ellipsize(s: &str, max: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= max {
        return s.to_string();
    }
    if max <= 1 {
        return chars[..max].iter().collect();
    }
    let mut out: String = chars[..max - 1].iter().collect();
    out.push('…');
    out
}

pub fn paint_double_rule(painter: &egui::Painter, rect: egui::Rect) {
    let top = egui::Rect::from_min_size(rect.min, egui::vec2(rect.width(), 3.0));
    painter.add(egui::Shape::rect_filled(top, CornerRadius::ZERO, W));
    let foot = egui::pos2(rect.min.x, rect.max.y - 1.0);
    let bottom = egui::Rect::from_min_size(foot, egui::vec2(rect.width(), 1.0));
    painter.add(egui::Shape::rect_filled(bottom, CornerRadius::ZERO, W));
}

pub fn double_rule(ui: &mut egui::Ui) {
    let w = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(egui::vec2(w, 6.0), egui::Sense::hover());
    paint_double_rule(ui.painter(), rect);
}

pub fn paint_halftone(
    painter: &egui::Painter,
    rect: egui::Rect,
    step: f32,
    radius: f32,
    color: Color32,
) {
    let mut row: usize = 0;
    let mut y = rect.min.y + radius + 1.0;
    while y < rect.max.y - radius {
        let off = if row % 2 == 1 { step * 0.5 } else { 0.0 };
        let mut x = rect.min.x + radius + 1.0 + off;
        while x < rect.max.x - radius {
            painter.circle_filled(egui::pos2(x, y), radius, color);
            x += step;
        }
        y += step;
        row += 1;
    }
}

pub fn tram_band(ui: &mut egui::Ui, height: f32) {
    let w = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(egui::vec2(w, height), egui::Sense::hover());
    paint_halftone(ui.painter(), rect, 14.0, 2.6, G5);
}

pub fn paint_tile_shape(
    painter: &egui::Painter,
    rect: egui::Rect,
    slant: f32,
    fill: Color32,
    stroke: Stroke,
    shadow: Color32,
) {
    let quad = skewed_quad(rect, slant);
    if shadow != Color32::TRANSPARENT {
        let ghost: Vec<egui::Pos2> = quad
            .iter()
            .map(|p| *p + egui::vec2(SHADOW_DX, SHADOW_DY))
            .collect();
        painter.add(egui::Shape::convex_polygon(ghost, shadow, Stroke::NONE));
    }
    painter.add(egui::Shape::convex_polygon(quad, fill, stroke));
}

pub struct TileSpec<'a> {
    pub kicker: &'a str,
    pub title: &'a str,
    pub hint: &'a str,
    pub no: &'a str,
}

pub fn skewed_tile(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash + std::fmt::Debug,
    spec: TileSpec<'_>,
    size: egui::Vec2,
    active: bool,
    enabled: bool,
) -> egui::Response {
    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
    let id = ui.make_persistent_id(id);
    let resp = ui.interact(rect, id, egui::Sense::click());
    let slant = slant_for(rect.height());
    let hovered = resp.hovered();
    let focused = resp.has_focus();
    let (fill, edge, shadow) = if !enabled {
        (K1, G5, Color32::TRANSPARENT)
    } else if active {
        (W, K0, G4)
    } else if hovered {
        (K2, W, W)
    } else {
        (K1, W, G4)
    };
    paint_tile_shape(ui.painter(), rect, slant, fill, Stroke::new(2.0, edge), shadow);
    if active && enabled {
        let dots = egui::Rect::from_min_max(
            egui::pos2(rect.max.x - slant.abs() - 40.0, rect.min.y + 8.0),
            egui::pos2(rect.max.x - slant.abs() - 8.0, rect.min.y + 26.0),
        );
        paint_halftone(ui.painter(), dots, 9.0, 1.6, G4);
    }
    let (kick_c, title_c, hint_c, no_c) = if !enabled {
        (G6, G6, G6, G5)
    } else if active {
        (K2, K0, K2, G4)
    } else {
        (G7, W, G8, G6)
    };
    let pad = slant.abs() + 12.0;
    let left = rect.min.x + pad;
    let right = rect.max.x - pad;
    let tp = ui.painter_at(rect);
    tp.text(
        egui::pos2(left, rect.min.y + 8.0),
        egui::Align2::LEFT_TOP,
        spec.kicker.to_uppercase(),
        egui::FontId::new(10.0, egui::FontFamily::Proportional),
        kick_c,
    );
    tp.text(
        egui::pos2(right, rect.max.y - 8.0),
        egui::Align2::RIGHT_BOTTOM,
        spec.no,
        egui::FontId::new(10.0, egui::FontFamily::Monospace),
        no_c,
    );
    tp.text(
        egui::pos2(left, rect.min.y + 24.0),
        egui::Align2::LEFT_TOP,
        spec.title.to_uppercase(),
        egui::FontId::new(title_size_for(spec.title), egui::FontFamily::Proportional),
        title_c,
    );
    tp.text(
        egui::pos2(left, rect.max.y - 8.0),
        egui::Align2::LEFT_BOTTOM,
        ellipsize(spec.hint, 26),
        egui::FontId::new(11.0, egui::FontFamily::Monospace),
        hint_c,
    );
    if focused {
        let fr = rect.expand(3.0);
        let fq = skewed_quad(fr, slant_for(fr.height()));
        ui.painter()
            .add(egui::Shape::convex_polygon(fq, Color32::TRANSPARENT, Stroke::new(2.0, W)));
    }
    resp
}

pub fn paint_transfer_bar(ui: &mut egui::Ui, t: f64) {
    let w = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(egui::vec2(w, 14.0), egui::Sense::hover());
    let p = ui.painter();
    p.add(egui::Shape::rect_stroke(
        rect,
        CornerRadius::ZERO,
        Stroke::new(1.5, W),
        egui::StrokeKind::Middle,
    ));
    let inner = egui::Rect::from_min_max(
        egui::pos2(rect.min.x + 2.0, rect.min.y + 2.0),
        egui::pos2(rect.max.x - 2.0, rect.max.y - 2.0),
    );
    let phase = (t * 48.0) as f32 % 20.0;
    let mut x = inner.min.x - 20.0 + phase;
    while x < inner.max.x {
        let lo = x.max(inner.min.x);
        let hi = (x + 10.0).min(inner.max.x);
        if hi > lo {
            let seg = egui::Rect::from_min_max(egui::pos2(lo, inner.min.y), egui::pos2(hi, inner.max.y));
            p.add(egui::Shape::rect_filled(seg, CornerRadius::ZERO, W));
        }
        x += 20.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slant_scales_with_height() {
        let s = slant_for(100.0);
        assert!(s > 8.0 && s < 10.0);
        assert!(slant_for(200.0) > slant_for(100.0));
        assert_eq!(slant_for(0.0), 0.0);
    }

    #[test]
    fn quad_is_symmetric_parallelogram() {
        let r = egui::Rect::from_min_size(egui::pos2(10.0, 20.0), egui::vec2(180.0, 96.0));
        let q = skewed_quad(r, 8.0);
        assert_eq!(q.len(), 4);
        assert_eq!(q[0].y, 20.0);
        assert_eq!(q[2].y, 116.0);
        assert!((q[1].x - q[0].x - 180.0).abs() < 0.01);
        assert!((q[0].x - 10.0 - 8.0).abs() < 0.01);
        assert!((190.0 - q[2].x - 8.0).abs() < 0.01);
        assert!((q[0].x - 18.0).abs() < 0.01);
        assert!((q[3].x - 2.0).abs() < 0.01);
    }

    #[test]
    fn columns_stay_in_range() {
        assert_eq!(tile_columns(780.0, 150.0, 12.0), 4);
        assert_eq!(tile_columns(300.0, 150.0, 12.0), 1);
        assert_eq!(tile_columns(0.0, 150.0, 12.0), 1);
        assert_eq!(tile_columns(4000.0, 150.0, 12.0), 4);
        assert_eq!(tile_columns(500.0, 150.0, 12.0), 3);
    }

    #[test]
    fn width_fills_row() {
        let w = tile_width(780.0, 4, 12.0);
        assert!((w - 186.0).abs() < 0.01);
        assert!(tile_width(200.0, 1, 12.0) >= 120.0);
        assert!(tile_width(2000.0, 1, 12.0) <= 240.0);
    }

    #[test]
    fn title_size_shrinks_for_long_labels() {
        assert_eq!(title_size_for("PING"), 19.0);
        assert_eq!(title_size_for("UYGULAMA"), 17.0);
        assert_eq!(title_size_for("EKRANI ACIK TUT"), 15.0);
    }

    #[test]
    fn ellipsize_keeps_short_strings() {
        assert_eq!(ellipsize("pong", 24), "pong");
        let long = "ok: telefon — via lan; plugins: ping, clipboard";
        let cut = ellipsize(long, 26);
        assert!(cut.chars().count() <= 26);
        assert!(cut.ends_with('…'));
        assert_eq!(ellipsize("ab", 1), "a");
    }
}
