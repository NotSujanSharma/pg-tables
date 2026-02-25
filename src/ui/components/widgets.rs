//! Visual indicator widgets — search bar, section headers, loading spinners.

use crate::config::style;
use eframe::egui;

// ── Search bar ───────────────────────────────────────────────────────────────

/// Styled search bar with inline clear button.
pub fn search_bar(ui: &mut egui::Ui, search: &mut String, hint: &str) {
    ui.horizontal(|ui| {
        ui.colored_label(style::COLOR_MUTED, "🔍");
        let resp = ui.add(
            egui::TextEdit::singleline(search)
                .hint_text(hint)
                .desired_width(style::SEARCH_WIDTH - 20.0),
        );
        if !search.is_empty() {
            if ui
                .add(
                    egui::Button::new(egui::RichText::new("✕").color(style::COLOR_MUTED))
                        .frame(false),
                )
                .on_hover_text("Clear search")
                .clicked()
            {
                search.clear();
                resp.request_focus();
            }
        }
    });
}

// ── Count badge ──────────────────────────────────────────────────────────────

#[allow(dead_code)]
pub fn count_badge(ui: &mut egui::Ui, count: usize, label: &str) {
    ui.colored_label(style::COLOR_MUTED, format!("{count} {label}"));
}

// ── Section header ───────────────────────────────────────────────────────────

/// Bold title on the left, muted count on the right, followed by a separator.
pub fn section_header(ui: &mut egui::Ui, title: &str, count: usize, count_label: &str) {
    ui.horizontal(|ui| {
        ui.add(egui::Label::new(
            egui::RichText::new(title).strong().size(13.5),
        ));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if !count_label.is_empty() {
                ui.colored_label(style::COLOR_MUTED, format!("{count} {count_label}"));
            }
        });
    });
    ui.separator();
}

// ── Loading indicator ────────────────────────────────────────────────────────

/// Show an animated loading indicator centred in the available area.
/// Calls `ctx.request_repaint()` so animation keeps running.
pub fn loading_ui(ui: &mut egui::Ui, message: &str) {
    let t = ui.ctx().input(|i| i.time);
    let dots = match (t * 2.5) as usize % 4 {
        0 => "",
        1 => ".",
        2 => "..",
        _ => "...",
    };
    ui.ctx().request_repaint();

    ui.centered_and_justified(|ui| {
        ui.vertical_centered(|ui| {
            ui.add_space(8.0);
            ui.add(
                egui::ProgressBar::new(0.0)
                    .animate(true)
                    .desired_width(200.0),
            );
            ui.add_space(6.0);
            ui.colored_label(style::COLOR_MUTED, format!("{message}{dots}"));
        });
    });
}

/// Show a modal "Loading…" window centred on screen (use in top-level update).
pub fn loading_modal(ctx: &egui::Context, message: &str) {
    let t = ctx.input(|i| i.time);
    let dots = match (t * 2.5) as usize % 4 {
        0 => "",
        1 => ".",
        2 => "..",
        _ => "...",
    };
    ctx.request_repaint();

    egui::Window::new("##loading_modal")
        .title_bar(false)
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .fixed_size([220.0, 70.0])
        .show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(8.0);
                ui.add(
                    egui::ProgressBar::new(0.0)
                        .animate(true)
                        .desired_width(180.0),
                );
                ui.add_space(6.0);
                ui.colored_label(style::COLOR_MUTED, format!("⏳  {message}{dots}"));
            });
        });
}
