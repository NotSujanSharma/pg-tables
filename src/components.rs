//! Reusable UI components used across all tabs.

use crate::style;
use eframe::egui;
use std::collections::HashSet;

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
                .add(egui::Button::new(egui::RichText::new("✕").color(style::COLOR_MUTED)).frame(false))
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
            ui.colored_label(
                style::COLOR_MUTED,
                format!("{message}{dots}"),
            );
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
                ui.colored_label(
                    style::COLOR_MUTED,
                    format!("⏳  {message}{dots}"),
                );
            });
        });
}

// ── Selection toolbar ────────────────────────────────────────────────────────

/// "Select All" / "Deselect All" buttons + selected-count badge.
/// Returns `(select_all_clicked, deselect_all_clicked)`.
pub fn selection_toolbar(ui: &mut egui::Ui, selected_count: usize) -> (bool, bool) {
    let mut select_all = false;
    let mut clear = false;
    ui.horizontal(|ui| {
        select_all = ui
            .add(egui::Button::new(egui::RichText::new("☑ Select All").size(12.0)))
            .on_hover_text("Select all visible items")
            .clicked();
        clear = ui
            .add(egui::Button::new(egui::RichText::new("☐ Deselect All").size(12.0)))
            .on_hover_text("Clear all selections")
            .clicked();
        if selected_count > 0 {
            ui.separator();
            ui.colored_label(
                style::COLOR_ACCENT,
                format!("{selected_count} selected"),
            );
        }
    });
    (select_all, clear)
}

// ── Filter upload row ─────────────────────────────────────────────────────────

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
                .add(egui::Button::new(egui::RichText::new("✕").color(style::COLOR_MUTED)).frame(false))
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

// ── Checkbox list ────────────────────────────────────────────────────────────

/// Filtered checkbox list.  Returns items that had their selection toggled as
/// `(name, new_checked_state)`.
/// `filter_list` – if `Some`, only names in that list are shown.
pub fn checkbox_list(
    ui: &mut egui::Ui,
    id: &str,
    items: &[(String, String)], // (name, subtitle)
    selected: &HashSet<String>,
    search: &str,
    filter_list: Option<&[String]>,
) -> Vec<(String, bool)> {
    let mut toggles = vec![];
    let search_lower = search.to_lowercase();

    // Build an optional fast-lookup set
    let filter_set: Option<HashSet<&str>> =
        filter_list.map(|lst| lst.iter().map(|s| s.as_str()).collect());

    egui::ScrollArea::vertical()
        .id_salt(id)
        .auto_shrink([false, false])
        .show(ui, |ui| {
            let mut any = false;
            for (name, subtitle) in items {
                // Text search
                if !search_lower.is_empty() && !name.to_lowercase().contains(&search_lower) {
                    continue;
                }
                // File filter
                if let Some(ref set) = filter_set {
                    if !set.contains(name.as_str()) {
                        continue;
                    }
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
                ui.add_space(8.0);
                ui.centered_and_justified(|ui| {
                    ui.colored_label(style::COLOR_MUTED, "No items match.");
                });
            }
        });
    toggles
}

