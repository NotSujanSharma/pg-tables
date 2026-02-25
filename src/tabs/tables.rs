//! Tables tab — browsable table list with column detail panel.

use crate::components::{loading_ui, search_bar, section_header};
use crate::db;
use crate::style;
use eframe::egui;
use sqlx::PgPool;
use std::sync::{Arc, Mutex};

type Pending<T> = Arc<Mutex<Option<T>>>;

/// State for the Tables tab.
pub struct TablesState {
    pub tables: Vec<String>,
    pub search: String,
    pub selected_table: Option<String>,
    pub columns: Vec<db::ColumnInfo>,

    // async loading
    pub loading_tables: bool,
    pub loading_columns: bool,
    pending_tables: Option<Pending<Vec<String>>>,
    pending_columns: Option<Pending<Vec<db::ColumnInfo>>>,
}

impl Default for TablesState {
    fn default() -> Self {
        Self {
            tables: vec![],
            search: String::new(),
            selected_table: None,
            columns: vec![],
            loading_tables: false,
            loading_columns: false,
            pending_tables: None,
            pending_columns: None,
        }
    }
}

impl TablesState {
    pub fn clear(&mut self) {
        self.tables.clear();
        self.search.clear();
        self.selected_table = None;
        self.columns.clear();
        self.loading_tables = false;
        self.loading_columns = false;
        self.pending_tables = None;
        self.pending_columns = None;
    }

    /// Non-blocking: spawn table load in background.
    pub fn load_tables(&mut self, rt: &tokio::runtime::Runtime, pool: &PgPool, schema: &str) {
        let result: Pending<Vec<String>> = Arc::new(Mutex::new(None));
        self.pending_tables = Some(result.clone());
        self.loading_tables = true;
        self.tables.clear();
        self.selected_table = None;
        self.columns.clear();
        self.search.clear();

        let pool = pool.clone();
        let schema = schema.to_string();
        rt.spawn(async move {
            let tables = db::list_tables(&pool, &schema).await.unwrap_or_default();
            *result.lock().unwrap() = Some(tables);
        });
    }

    /// Non-blocking: spawn column load in background.
    fn start_load_columns(
        &mut self,
        rt: &tokio::runtime::Runtime,
        pool: &PgPool,
        schema: &str,
        table: &str,
    ) {
        let result: Pending<Vec<db::ColumnInfo>> = Arc::new(Mutex::new(None));
        self.pending_columns = Some(result.clone());
        self.loading_columns = true;
        self.selected_table = Some(table.to_string());
        self.columns.clear();

        let pool = pool.clone();
        let schema = schema.to_string();
        let table = table.to_string();
        rt.spawn(async move {
            let cols = db::list_columns(&pool, &schema, &table)
                .await
                .unwrap_or_default();
            *result.lock().unwrap() = Some(cols);
        });
    }

    /// Poll pending async results — call at the top of `draw`.
    fn poll(&mut self, ctx: &egui::Context) {
        if self.loading_tables {
            ctx.request_repaint();
            let done = {
                let r = self.pending_tables.as_ref()
                    .and_then(|p| p.try_lock().ok())
                    .and_then(|mut g| g.take());
                r
            };
            if let Some(tables) = done {
                self.tables = tables;
                self.loading_tables = false;
                self.pending_tables = None;
            }
        }
        if self.loading_columns {
            ctx.request_repaint();
            let done = {
                let r = self.pending_columns.as_ref()
                    .and_then(|p| p.try_lock().ok())
                    .and_then(|mut g| g.take());
                r
            };
            if let Some(cols) = done {
                self.columns = cols;
                self.loading_columns = false;
                self.pending_columns = None;
            }
        }
    }

    pub fn draw(
        &mut self,
        ui: &mut egui::Ui,
        rt: &tokio::runtime::Runtime,
        pool: &PgPool,
        schema: &str,
    ) {
        self.poll(ui.ctx());

        let available = ui.available_size();
        let left_width = (available.x * style::PANEL_LEFT_RATIO).max(200.0);
        let mut clicked_table: Option<String> = None;

        ui.horizontal(|ui| {
            // ── Left: table list ──────────────────────────────────────────
            ui.vertical(|ui| {
                ui.set_width(left_width);
                ui.set_min_height(available.y);

                section_header(ui, "Tables", self.tables.len(), "total");
                ui.add_space(3.0);
                search_bar(ui, &mut self.search, "Filter tables…");
                ui.add_space(4.0);

                if self.loading_tables {
                    loading_ui(ui, "Loading tables");
                } else {
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
                                if ui
                                    .add(egui::Button::selectable(is_selected, name))
                                    .clicked()
                                    && !is_selected
                                {
                                    clicked_table = Some(name.clone());
                                }
                            }
                            if !any {
                                ui.add_space(8.0);
                                ui.centered_and_justified(|ui| {
                                    ui.colored_label(style::COLOR_MUTED, "No tables match.");
                                });
                            }
                        });
                }
            });

            ui.separator();

            // ── Right: column details ─────────────────────────────────────
            ui.vertical(|ui| {
                if self.loading_columns {
                    let tname = self
                        .selected_table
                        .as_deref()
                        .unwrap_or("table")
                        .to_string();
                    section_header(ui, &format!("Columns: {tname}"), 0, "");
                    ui.add_space(4.0);
                    loading_ui(ui, "Loading columns");
                } else if let Some(table_name) = &self.selected_table.clone() {
                    section_header(
                        ui,
                        &format!("Columns: {table_name}"),
                        self.columns.len(),
                        "columns",
                    );
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
                    section_header(ui, "Columns", 0, "");
                    ui.centered_and_justified(|ui| {
                        ui.colored_label(style::COLOR_MUTED, "← Select a table to view its columns");
                    });
                }
            });
        });

        if let Some(name) = clicked_table {
            self.start_load_columns(rt, pool, schema, &name);
        }
    }
}

