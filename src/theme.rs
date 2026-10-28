//! Visual identity: fonts, colours, and a few shared widgets.

use eframe::egui::{
    self, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Frame, Margin, Response, RichText, Stroke,
    TextStyle, Ui, Vec2, vec2,
};
use std::sync::Arc;

#[derive(Clone, Copy)]
pub struct Palette {
    pub accent: Color32,
    pub accent_soft: Color32,
    pub on_accent: Color32,
    pub ok: Color32,
    pub danger: Color32,
    pub card: Color32,
    pub card_border: Color32,
    pub weak: Color32,
}

pub fn palette(dark: bool) -> Palette {
    if dark {
        Palette {
            accent: Color32::from_rgb(0xD9, 0x4A, 0x72),
            accent_soft: Color32::from_rgb(0x3A, 0x22, 0x2B),
            on_accent: Color32::WHITE,
            ok: Color32::from_rgb(0x4C, 0xC2, 0x8A),
            danger: Color32::from_rgb(0xF0, 0x6A, 0x6A),
            card: Color32::from_rgb(0x26, 0x26, 0x2A),
            card_border: Color32::from_rgb(0x3A, 0x3A, 0x40),
            weak: Color32::from_rgb(0x9A, 0x9A, 0xA4),
        }
    } else {
        Palette {
            accent: Color32::from_rgb(0xB1, 0x1F, 0x4B),
            accent_soft: Color32::from_rgb(0xFB, 0xE9, 0xEF),
            on_accent: Color32::WHITE,
            ok: Color32::from_rgb(0x10, 0x7C, 0x41),
            danger: Color32::from_rgb(0xC4, 0x2B, 0x1C),
            card: Color32::WHITE,
            card_border: Color32::from_rgb(0xE1, 0xE1, 0xE6),
            weak: Color32::from_rgb(0x6B, 0x6B, 0x76),
        }
    }
}

pub fn install_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();

    // Segoe UI gives the app a native Windows look; fall back to egui's bundled font if missing.
    let windir = std::env::var("WINDIR").unwrap_or_else(|_| "C:\\Windows".into());
    if let Ok(bytes) = std::fs::read(format!("{windir}\\Fonts\\segoeui.ttf")) {
        fonts.font_data.insert("segoe".into(), Arc::new(FontData::from_owned(bytes)));
        fonts.families.entry(FontFamily::Proportional).or_default().insert(0, "segoe".into());
    }
    if let Ok(bytes) = std::fs::read(format!("{windir}\\Fonts\\consola.ttf")) {
        fonts.font_data.insert("consolas".into(), Arc::new(FontData::from_owned(bytes)));
        fonts.families.entry(FontFamily::Monospace).or_default().insert(0, "consolas".into());
    }

    egui_phosphor::add_to_fonts(&mut fonts, egui_phosphor::Variant::Regular);
    ctx.set_fonts(fonts);
}

pub fn apply_style(ctx: &egui::Context) {
    for (theme, dark) in [(egui::Theme::Dark, true), (egui::Theme::Light, false)] {
        ctx.set_visuals_of(theme, visuals_for(dark));
    }
    ctx.all_styles_mut(|style| {
        style.spacing.item_spacing = vec2(8.0, 8.0);
        style.spacing.button_padding = vec2(12.0, 6.0);
        style.spacing.interact_size.y = 30.0;
        style.text_styles.insert(TextStyle::Body, FontId::proportional(14.5));
        style.text_styles.insert(TextStyle::Button, FontId::proportional(14.5));
        style.text_styles.insert(TextStyle::Small, FontId::proportional(12.0));
        style.text_styles.insert(TextStyle::Heading, FontId::proportional(22.0));
        style.text_styles.insert(TextStyle::Monospace, FontId::monospace(13.5));
    });
}

fn visuals_for(dark: bool) -> egui::Visuals {
    let p = palette(dark);
    let mut visuals = if dark { egui::Visuals::dark() } else { egui::Visuals::light() };
    visuals.selection.bg_fill = p.accent;
    visuals.selection.stroke = Stroke::new(1.0, p.on_accent);
    visuals.hyperlink_color = p.accent;
    if dark {
        visuals.panel_fill = Color32::from_rgb(0x1B, 0x1B, 0x1F);
        visuals.extreme_bg_color = Color32::from_rgb(0x15, 0x15, 0x18);
    } else {
        visuals.panel_fill = Color32::from_rgb(0xF5, 0xF5, 0xF8);
        visuals.extreme_bg_color = Color32::WHITE;
    }
    let radius = CornerRadius::same(7);
    visuals.widgets.noninteractive.corner_radius = radius;
    visuals.widgets.inactive.corner_radius = radius;
    visuals.widgets.hovered.corner_radius = radius;
    visuals.widgets.active.corner_radius = radius;
    visuals.widgets.open.corner_radius = radius;
    visuals.window_corner_radius = CornerRadius::same(12);
    visuals
}

pub fn card_frame(p: &Palette) -> egui::Frame {
    egui::Frame::new()
        .fill(p.card)
        .stroke(Stroke::new(1.0, p.card_border))
        .corner_radius(CornerRadius::same(10))
        .inner_margin(Margin::same(14))
}

pub fn primary_button(ui: &mut Ui, p: &Palette, label: impl Into<String>, min: Vec2) -> Response {
    ui.add(
        egui::Button::new(RichText::new(label.into()).color(p.on_accent).strong())
            .fill(p.accent)
            .min_size(min)
            .corner_radius(CornerRadius::same(8)),
    )
}

pub fn danger_button(ui: &mut Ui, p: &Palette, label: impl Into<String>, min: Vec2) -> Response {
    ui.add(
        egui::Button::new(RichText::new(label.into()).color(Color32::WHITE).strong())
            .fill(p.danger)
            .min_size(min)
            .corner_radius(CornerRadius::same(8)),
    )
}

pub fn icon_button(ui: &mut Ui, icon: &str, tip: &str) -> Response {
    ui.add(egui::Button::new(RichText::new(icon).size(16.0)).frame(false).min_size(vec2(28.0, 28.0)))
        .on_hover_text(tip)
}

pub fn small_caps(p: &Palette, text: &str) -> RichText {
    RichText::new(text.to_uppercase()).size(11.5).color(p.weak).strong()
}

/// A small keyboard-key badge, e.g. `F2` or `Shift+F3`.
pub fn keycap(ui: &mut Ui, p: &Palette, text: &str) {
    Frame::new()
        .fill(p.card_border)
        .stroke(Stroke::new(1.0, p.weak.gamma_multiply(0.45)))
        .corner_radius(CornerRadius::same(5))
        .inner_margin(Margin::symmetric(7, 1))
        .show(ui, |ui| {
            ui.label(RichText::new(text).monospace().size(12.5).color(ui.visuals().strong_text_color()));
        });
}

/// A compact pill used for step metadata (after-key, delay, note).
pub fn chip(ui: &mut Ui, p: &Palette, text: impl Into<String>) -> Response {
    ui.label(RichText::new(text.into()).size(12.0).color(p.weak).background_color(p.card_border))
}

/// Segmented progress through a script: done steps are soft, the next step is solid.
pub fn step_segments(ui: &mut Ui, p: &Palette, current: usize, total: usize, width: f32) {
    let total = total.max(1);
    let (rect, response) = ui.allocate_exact_size(vec2(width, 8.0), egui::Sense::hover());
    let gap = 3.0;
    let seg = ((width - gap * (total as f32 - 1.0)) / total as f32).max(2.0);
    for i in 0..total {
        let x = rect.left() + i as f32 * (seg + gap);
        let color = if i == current {
            p.accent
        } else if i < current {
            p.accent.gamma_multiply(0.4)
        } else {
            p.card_border
        };
        ui.painter().rect_filled(
            egui::Rect::from_min_size(egui::pos2(x, rect.top()), vec2(seg, 8.0)),
            CornerRadius::same(3),
            color,
        );
    }
    response.on_hover_text(format!("Step {} of {}", current + 1, total));
}

/// Collapses whitespace/newlines into a single-line preview of at most `max` characters.
pub fn one_line(text: &str, max: usize) -> String {
    let flat: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() <= max {
        flat
    } else {
        let cut: String = flat.chars().take(max).collect();
        format!("{}…", cut.trim_end())
    }
}
