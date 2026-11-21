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
