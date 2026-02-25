//! Action-oriented components — selection toolbars, file filters, output panels.

use crate::config::style;
use eframe::egui;

// ── Selection toolbar ────────────────────────────────────────────────────────

/// "Select All" / "Deselect All" buttons + selected-count badge.
/// Returns `(select_all_clicked, deselect_all_clicked)`.
pub fn selection_toolbar(ui: &mut egui::Ui, selected_count: usize) -> (bool, bool) {
    let mut select_all = false;
    let mut clear = false;
    ui.horizontal(|ui| {
        select_all = ui
            .add(egui::Button::new(
                egui::RichText::new("☑ Select All").size(12.0),
            ))
            .on_hover_text("Select all visible items")
            .clicked();
        clear = ui
            .add(egui::Button::new(
                egui::RichText::new("☐ Deselect All").size(12.0),
            ))
            .on_hover_text("Clear all selections")
            .clicked();
        if selected_count > 0 {
            ui.separator();
            ui.colored_label(style::COLOR_ACCENT, format!("{selected_count} selected"));
        }
    });
    (select_all, clear)
}

// ── Filter upload row ────────────────────────────────────────────────────────

/// Upload-a-text-file filter row. Returns `true` when the filter changes.
/// `filter_list` holds the current loaded names (None = no filter).
pub fn filter_upload_row(ui: &mut egui::Ui, filter_list: &mut Option<Vec<String>>) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        let icon = if filter_list.is_some() { "📂" } else { "📂" };
        if ui
            .add(egui::Button::new(
                egui::RichText::new(format!("{icon} Load Filter List")).size(12.0),
            ))
            .on_hover_text("Load a .txt file (one name per line) to filter the list below")
            .clicked()
        {
            if let Some(path) = rfd::FileDialog::new()
                .add_filter("Text file", &["txt", "csv"])
                .set_title("Pick a name-list file")
                .pick_file()
            {
                if let Ok(content) = std::fs::read_to_string(path) {
                    let names: Vec<String> = content
                        .lines()
                        .map(|l| l.trim().to_string())
                        .filter(|l| !l.is_empty())
                        .collect();
                    *filter_list = Some(names);
                    changed = true;
                }
            }
        }

        if let Some(list) = filter_list {
            ui.separator();
            ui.colored_label(
                style::COLOR_FILTER,
                format!("Filter active – {} names", list.len()),
            );
            if ui
                .add(
                    egui::Button::new(egui::RichText::new("✕").color(style::COLOR_MUTED))
                        .frame(false),
                )
                .on_hover_text("Remove filter")
                .clicked()
            {
                *filter_list = None;
                changed = true;
            }
        } else {
            ui.colored_label(style::COLOR_MUTED, "Optional name-list filter");
        }
    });
    changed
}

// ── Output actions bar ───────────────────────────────────────────────────────

/// Primary action button + Copy + optional Save button row.
/// Returns `true` when the primary action button is clicked.
pub fn output_actions(
    ui: &mut egui::Ui,
    action_label: &str,
    can_act: bool,
    is_loading: bool,
    output: &str,
    save_filename: Option<&str>,
) -> bool {
    let mut acted = false;
    ui.horizontal(|ui| {
        let btn_text = if is_loading {
            format!("⏳ Working…")
        } else {
            action_label.to_string()
        };
        let btn = ui.add_enabled(
            can_act && !is_loading,
            egui::Button::new(egui::RichText::new(&btn_text).size(13.0))
                .min_size(egui::vec2(90.0, 28.0)),
        );
        if btn.clicked() {
            acted = true;
        }

        if !output.is_empty() {
            ui.separator();
            if ui
                .add(egui::Button::new("📋 Copy").min_size(egui::vec2(70.0, 28.0)))
                .clicked()
            {
                ui.ctx().copy_text(output.to_string());
            }
            if let Some(filename) = save_filename {
                if ui
                    .add(egui::Button::new("💾 Save").min_size(egui::vec2(70.0, 28.0)))
                    .clicked()
                {
                    if let Some(path) = rfd::FileDialog::new()
                        .set_file_name(filename)
                        .save_file()
                    {
                        let _ = std::fs::write(path, output);
                    }
                }
            }
        }
    });
    acted
}

// ── Output panel ─────────────────────────────────────────────────────────────

/// Monospace read-only text panel filling available space.
pub fn output_panel(ui: &mut egui::Ui, id: &str, output: &str, empty_msg: &str) {
    egui::ScrollArea::vertical()
        .id_salt(id)
        .auto_shrink([false, false])
        .show(ui, |ui| {
            if output.is_empty() {
                ui.centered_and_justified(|ui| {
                    ui.colored_label(style::COLOR_MUTED, empty_msg);
                });
            } else {
                ui.add(
                    egui::TextEdit::multiline(&mut output.to_string().as_str())
                        .font(egui::TextStyle::Monospace)
                        .desired_width(f32::INFINITY),
                );
            }
        });
}
