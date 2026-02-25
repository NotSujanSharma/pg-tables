//! Styling constants — edit these to change the look globally.

use eframe::egui;

// Layout
pub const PANEL_LEFT_RATIO: f32 = 0.30;
pub const SPACING: f32 = 6.0;
pub const SEARCH_WIDTH: f32 = f32::INFINITY;
pub const LOGIN_FIELD_WIDTH: f32 = 260.0;
pub const LOGIN_PANEL_WIDTH: f32 = 380.0;

// Colors
pub const COLOR_ERROR: egui::Color32 = egui::Color32::from_rgb(255, 90, 90);
pub const COLOR_ACCENT: egui::Color32 = egui::Color32::from_rgb(100, 160, 255);
pub const COLOR_MUTED: egui::Color32 = egui::Color32::from_rgb(140, 140, 150);
pub const COLOR_HEADER: egui::Color32 = egui::Color32::from_rgb(180, 200, 230);
pub const COLOR_NULL_BADGE: egui::Color32 = egui::Color32::from_rgb(80, 80, 90);
pub const COLOR_PK_BADGE: egui::Color32 = egui::Color32::from_rgb(60, 110, 60);
pub const COLOR_SUCCESS: egui::Color32 = egui::Color32::from_rgb(80, 200, 120);
