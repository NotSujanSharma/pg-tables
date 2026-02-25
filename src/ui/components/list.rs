//! Checkbox list with search and file-filter support.

use crate::config::style;
use eframe::egui;
use std::collections::HashSet;

/// Filtered checkbox list. Returns items that had their selection toggled as
/// `(name, new_checked_state)`.
/// `filter_list` — if `Some`, only names in that list are shown.
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
