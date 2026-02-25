//! Tables tab — browsable table list with column detail panel.

use crate::components::{search_bar, section_header};
use crate::db;
use crate::style;
use eframe::egui;
use sqlx::PgPool;

/// State for the Tables tab.
pub struct TablesState {
    pub tables: Vec<String>,
    pub search: String,
    pub selected_table: Option<String>,
    pub columns: Vec<db::ColumnInfo>,
}

impl Default for TablesState {
    fn default() -> Self {
        Self {
            tables: vec![],
            search: String::new(),
            selected_table: None,
            columns: vec![],
        }
    }
}

impl TablesState {
    pub fn clear(&mut self) {
        self.tables.clear();
        self.search.clear();
        self.selected_table = None;
        self.columns.clear();
    }

    pub fn load_tables(&mut self, rt: &tokio::runtime::Runtime, pool: &PgPool, schema: &str) {
        self.tables = rt
            .block_on(db::list_tables(pool, schema))
            .unwrap_or_default();
        self.selected_table = None;
        self.columns.clear();
        self.search.clear();
    }

    pub fn load_columns(&mut self, rt: &tokio::runtime::Runtime, pool: &PgPool, schema: &str, table: &str) {
        self.columns = rt
            .block_on(db::list_columns(pool, schema, table))
            .unwrap_or_default();
        self.selected_table = Some(table.to_string());
    }

    pub fn draw(&mut self, ui: &mut egui::Ui, rt: &tokio::runtime::Runtime, pool: &PgPool, schema: &str) {
        let available = ui.available_size();
        let left_width = (available.x * style::PANEL_LEFT_RATIO).max(180.0);

        // We need to collect the clicked table name outside the borrow
        let mut clicked_table: Option<String> = None;

        ui.horizontal(|ui| {
            // Left: table list
            ui.vertical(|ui| {
                ui.set_width(left_width);
                ui.set_min_height(available.y);

                section_header(ui, "Tables", self.tables.len(), "total");
                ui.add_space(2.0);
                search_bar(ui, &mut self.search, "Filter tables...");
                ui.add_space(4.0);

                let search_lower = self.search.to_lowercase();
                let selected = self.selected_table.clone();

                egui::ScrollArea::vertical()
                    .id_salt("tables_list_scroll")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        let mut any = false;
                        for name in &self.tables {
                            if !search_lower.is_empty()
                                && !name.to_lowercase().contains(&search_lower)
                            {
                                continue;
                            }
                            any = true;
                            let is_selected = selected.as_deref() == Some(name.as_str());
                            if ui.selectable_label(is_selected, name).clicked() && !is_selected {
                                clicked_table = Some(name.clone());
                            }
                        }
                        if !any {
                            ui.colored_label(style::COLOR_MUTED, "No tables match.");
                        }
                    });
            });

            ui.separator();

            // Right: column details
            ui.vertical(|ui| {
                if let Some(table_name) = &self.selected_table.clone() {
                    section_header(ui, &format!("Columns: {table_name}"), self.columns.len(), "columns");
                    ui.add_space(4.0);

                    egui::ScrollArea::vertical()
                        .id_salt("columns_scroll")
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            egui::Grid::new("columns_grid")
                                .num_columns(4)
                                .spacing([16.0, 4.0])
                                .striped(true)
                                .show(ui, |ui| {
                                    // Header
                                    ui.colored_label(style::COLOR_HEADER, "Column");
                                    ui.colored_label(style::COLOR_HEADER, "Type");
                                    ui.colored_label(style::COLOR_HEADER, "Nullable");
                                    ui.colored_label(style::COLOR_HEADER, "Default");
                                    ui.end_row();

                                    for col in &self.columns {
                                        ui.monospace(&col.name);
                                        ui.colored_label(style::COLOR_ACCENT, &col.data_type);

                                        if col.is_nullable == "YES" {
                                            ui.colored_label(style::COLOR_NULL_BADGE, "NULL");
                                        } else {
                                            ui.colored_label(style::COLOR_PK_BADGE, "NOT NULL");
                                        }

                                        match &col.column_default {
                                            Some(d) => ui.colored_label(style::COLOR_MUTED, d),
                                            None => ui.colored_label(style::COLOR_MUTED, "—"),
                                        };
                                        ui.end_row();
                                    }
                                });
                        });
                } else {
                    ui.centered_and_justified(|ui| {
                        ui.colored_label(style::COLOR_MUTED, "← Select a table to view its columns");
                    });
                }
            });
        });

        // Handle deferred click
        if let Some(name) = clicked_table {
            self.load_columns(rt, pool, schema, &name);
        }
    }
}
