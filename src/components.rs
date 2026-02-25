//! Reusable UI components used across all tabs.

use crate::style;
use eframe::egui;
use std::collections::HashSet;

/// Search bar with magnifying glass icon and clear button.
pub fn search_bar(ui: &mut egui::Ui, search: &mut String, hint: &str) {
    ui.horizontal(|ui| {
        ui.label("🔍");
        ui.add(
            egui::TextEdit::singleline(search)
                .hint_text(hint)
                .desired_width(style::SEARCH_WIDTH),
        );
        if !search.is_empty() && ui.small_button("✕").clicked() {
            search.clear();
        }
    });
}

/// Count badge — shows "N label" in muted text.
pub fn count_badge(ui: &mut egui::Ui, count: usize, label: &str) {
    ui.colored_label(style::COLOR_MUTED, format!("{count} {label}"));
}

/// Header row with a title and right-aligned count badge.
pub fn section_header(ui: &mut egui::Ui, title: &str, count: usize, count_label: &str) {
    ui.horizontal(|ui| {
        ui.strong(title);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            count_badge(ui, count, count_label);
        });
    });
}

/// Toolbar with Select All / Clear buttons. Returns (select_all_clicked, clear_clicked).
pub fn selection_toolbar(ui: &mut egui::Ui, selected_count: usize) -> (bool, bool) {
    let mut select_all = false;
    let mut clear = false;
    ui.horizontal(|ui| {
        if ui.small_button("Select All").clicked() {
            select_all = true;
        }
        if ui.small_button("Clear").clicked() {
            clear = true;
        }
        if selected_count > 0 {
            count_badge(ui, selected_count, "selected");
        }
    });
    (select_all, clear)
}

/// Output action bar: primary action button + Copy + optional Save.
/// Returns `true` if the primary action was clicked.
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
            "⏳ Working...".to_string()
        } else {
            action_label.to_string()
        };
        if ui
            .add_enabled(can_act && !is_loading, egui::Button::new(&btn_text))
            .clicked()
        {
            acted = true;
        }

        if !output.is_empty() {
            ui.separator();
            if ui.small_button("📋 Copy").clicked() {
                ui.ctx().copy_text(output.to_string());
            }
            if let Some(filename) = save_filename {
                if ui.small_button("💾 Save").clicked() {
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

/// Monospace read-only output text area filling all available space.
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

/// Checkbox list with search filtering.
/// Returns items that had their selection toggled as `(name, new_checked_state)`.
pub fn checkbox_list(
    ui: &mut egui::Ui,
    id: &str,
    items: &[(String, String)], // (name, subtitle)
    selected: &HashSet<String>,
    search: &str,
) -> Vec<(String, bool)> {
    let mut toggles = vec![];
    let search_lower = search.to_lowercase();

    egui::ScrollArea::vertical()
        .id_salt(id)
        .auto_shrink([false, false])
        .show(ui, |ui| {
            let mut any = false;
            for (name, subtitle) in items {
                if !search_lower.is_empty() && !name.to_lowercase().contains(&search_lower) {
                    continue;
                }
                any = true;
                let mut checked = selected.contains(name);
                let label = if subtitle.is_empty() {
                    name.clone()
                } else {
                    format!("{name}  ({subtitle})")
                };
                if ui.checkbox(&mut checked, label).changed() {
                    toggles.push((name.clone(), checked));
                }
            }
            if !any {
                ui.colored_label(style::COLOR_MUTED, "No items match your search.");
            }
        });
    toggles
}
