//! Styling constants — edit these to change the look globally.

use eframe::egui;

// Layout
pub const PANEL_LEFT_RATIO: f32 = 0.28;
pub const SPACING: f32 = 5.0;
pub const SEARCH_WIDTH: f32 = f32::INFINITY;
pub const LOGIN_FIELD_WIDTH: f32 = 260.0;
pub const LOGIN_PANEL_WIDTH: f32 = 400.0;

// Colors
pub const COLOR_ERROR: egui::Color32 = egui::Color32::from_rgb(230, 80, 80);
pub const COLOR_ACCENT: egui::Color32 = egui::Color32::from_rgb(100, 165, 255);
pub const COLOR_MUTED: egui::Color32 = egui::Color32::from_rgb(130, 130, 145);
pub const COLOR_HEADER: egui::Color32 = egui::Color32::from_rgb(175, 200, 235);
pub const COLOR_NULL_BADGE: egui::Color32 = egui::Color32::from_rgb(90, 90, 105);
pub const COLOR_PK_BADGE: egui::Color32 = egui::Color32::from_rgb(60, 130, 70);
pub const COLOR_SUCCESS: egui::Color32 = egui::Color32::from_rgb(80, 200, 120);
#[allow(dead_code)]
pub const COLOR_WARNING: egui::Color32 = egui::Color32::from_rgb(230, 180, 60);
pub const COLOR_FILTER: egui::Color32 = egui::Color32::from_rgb(90, 170, 90);

/// Apply a consistent dark professional theme.
pub fn setup_visuals(ctx: &egui::Context) {
    let mut visuals = egui::Visuals::dark();
    // Panel / window backgrounds
    visuals.panel_fill        = egui::Color32::from_rgb(22, 22, 26);
    visuals.window_fill       = egui::Color32::from_rgb(28, 28, 34);
    visuals.faint_bg_color    = egui::Color32::from_rgb(30, 30, 38);
    visuals.extreme_bg_color  = egui::Color32::from_rgb(14, 14, 18); // text edit bg
    // Button fills
    visuals.widgets.inactive.bg_fill = egui::Color32::from_rgb(48, 50, 62);
    visuals.widgets.hovered.bg_fill  = egui::Color32::from_rgb(62, 66, 82);
    visuals.widgets.active.bg_fill   = egui::Color32::from_rgb(75, 95, 145);
    // Hyperlink / selection tint
    visuals.selection.bg_fill = egui::Color32::from_rgb(50, 80, 150);
    ctx.set_visuals(visuals);

    let mut style = (*ctx.style()).clone();
    style.spacing.item_spacing   = egui::vec2(8.0, 4.0);
    style.spacing.button_padding = egui::vec2(10.0, 5.0);
    style.spacing.window_margin  = egui::Margin::same(12);
    ctx.set_style(style);
}
