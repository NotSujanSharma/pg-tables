//! Schema Generator tab — select tables, pick a target format, generate DDL.

use crate::ui::components::{
    checkbox_list, filter_upload_row, loading_ui, output_panel, search_bar, section_header,
    selection_toolbar,
};
use crate::config::style;
use crate::db;
use crate::schema::{self, SchemaFormat};
use eframe::egui;
use sqlx::PgPool;
use std::collections::HashSet;
use std::sync::{Arc, Mutex};

type Pending<T> = Arc<Mutex<Option<T>>>;

/// State for the Schema Generator tab.
pub struct CreateScriptState {
    // left panel — table selection
    pub search: String,
    pub selected: HashSet<String>,
    pub filter_list: Option<Vec<String>>,

    // format selector
    format: SchemaFormat,

    // output
    pub output: String,
    pub generating: bool,
    pending: Option<Pending<String>>,
}

impl Default for CreateScriptState {
    fn default() -> Self {
        Self {
            search: String::new(),
            selected: HashSet::new(),
            filter_list: None,
            format: SchemaFormat::Postgres,
            output: String::new(),
            generating: false,
            pending: None,
        }
    }
}

impl CreateScriptState {
    pub fn clear(&mut self) {
        self.search.clear();
        self.selected.clear();
        self.output.clear();
        self.generating = false;
        self.filter_list = None;
        self.pending = None;
    }

    /// Kick off async schema generation for all selected tables.
    fn start_generate(
        &mut self,
        rt: &tokio::runtime::Runtime,
        pool: &PgPool,
        schema_name: &str,
    ) {
        let result: Pending<String> = Arc::new(Mutex::new(None));
        self.pending = Some(result.clone());
        self.generating = true;
        self.output.clear();

        let pool = pool.clone();
        let schema_name = schema_name.to_string();
        let fmt = self.format;
        let mut selected: Vec<String> = self.selected.iter().cloned().collect();
        selected.sort();

        rt.spawn(async move {
            let mut metas: Vec<db::TableMeta> = Vec::new();
            let mut errors = String::new();
            for table in &selected {
                match db::fetch_table_meta(&pool, &schema_name, table).await {
                    Ok(m) => metas.push(m),
                    Err(e) => {
                        errors.push_str(&format!("-- Error for {table}: {e}\n"));
                    }
                }
            }
            let mut output = schema::format_tables(&metas, fmt);
            if !errors.is_empty() {
                output.push_str("\n\n");
                output.push_str(&errors);
            }
            *result.lock().unwrap() = Some(output);
        });
    }

    /// Poll the pending async result.
    fn poll(&mut self, ctx: &egui::Context) {
        if !self.generating {
            return;
        }
        ctx.request_repaint();
        let done = self
            .pending
            .as_ref()
            .and_then(|p| p.try_lock().ok())
            .and_then(|mut g| g.take());
        if let Some(output) = done {
            self.output = output;
            self.generating = false;
            self.pending = None;
        }
    }

    pub fn draw(
        &mut self,
        ui: &mut egui::Ui,
        tables: &[String],
        rt: &tokio::runtime::Runtime,
        pool: &PgPool,
        schema_name: &str,
    ) {
        self.poll(ui.ctx());

        let available = ui.available_size();
        let left_width = (available.x * style::PANEL_LEFT_RATIO).max(210.0);

        ui.horizontal(|ui| {
            // ── Left: table selection ─────────────────────────────────────
            ui.vertical(|ui| {
                ui.set_width(left_width);
                ui.set_min_height(available.y);

                let filter_snap: Option<HashSet<String>> = self
                    .filter_list
                    .as_ref()
                    .map(|l| l.iter().cloned().collect());

                let visible_count = tables
                    .iter()
                    .filter(|t| {
                        let s = self.search.to_lowercase();
                        let text_ok = s.is_empty() || t.to_lowercase().contains(&s);
                        let filter_ok = filter_snap
                            .as_ref()
                            .map(|fs| fs.contains(t.as_str()))
                            .unwrap_or(true);
                        text_ok && filter_ok
                    })
                    .count();

                section_header(ui, "Select Tables", visible_count, "shown");
                ui.add_space(3.0);
                search_bar(ui, &mut self.search, "Filter tables…");
                ui.add_space(2.0);

                let filter_changed = filter_upload_row(ui, &mut self.filter_list);
                if filter_changed {
                    if let Some(list) = &self.filter_list {
                        let allowed: HashSet<&str> =
                            list.iter().map(|s| s.as_str()).collect();
                        self.selected.retain(|t| allowed.contains(t.as_str()));
                    }
                }
                ui.add_space(2.0);

                let (sel_all, desel_all) = selection_toolbar(ui, self.selected.len());
                if sel_all {
                    let s = self.search.to_lowercase();
                    for t in tables {
                        let text_ok = s.is_empty() || t.to_lowercase().contains(&s);
                        let filter_ok = filter_snap
                            .as_ref()
                            .map(|fs| fs.contains(t.as_str()))
                            .unwrap_or(true);
                        if text_ok && filter_ok {
                            self.selected.insert(t.clone());
                        }
                    }
                }
                if desel_all {
                    self.selected.clear();
                }
                ui.add_space(2.0);

                let items: Vec<(String, String)> =
                    tables.iter().map(|t| (t.clone(), String::new())).collect();
                let filter_for_list = self.filter_list.as_deref();
                let toggles = checkbox_list(
                    ui,
                    "schema_gen_table_select",
                    &items,
                    &self.selected,
                    &self.search,
                    filter_for_list,
                );
                for (name, checked) in toggles {
                    if checked {
                        self.selected.insert(name);
                    } else {
                        self.selected.remove(&name);
                    }
                }
            });

            ui.separator();

            // ── Right: format selection + output ──────────────────────────
            ui.vertical(|ui| {
                section_header(ui, "Schema Generator", 0, "");
                ui.add_space(3.0);

                // Format picker + Generate button
                ui.horizontal(|ui| {
                    ui.colored_label(style::COLOR_MUTED, "Target format:");
                    egui::ComboBox::from_id_salt("schema_format_combo")
                        .selected_text(self.format.label())
                        .width(170.0)
                        .show_ui(ui, |ui| {
                            for &fmt in SchemaFormat::ALL {
                                ui.selectable_value(&mut self.format, fmt, fmt.label());
                            }
                        });

                    ui.add_space(8.0);

                    let can = !self.selected.is_empty();
                    let btn_text = if self.generating {
                        "⏳  Generating…"
                    } else {
                        "⚙  Generate Schema"
                    };
                    if ui
                        .add_enabled(
                            can && !self.generating,
                            egui::Button::new(egui::RichText::new(btn_text).size(13.0))
                                .min_size(egui::vec2(140.0, 28.0)),
                        )
                        .clicked()
                    {
                        self.start_generate(rt, pool, schema_name);
                    }
                });

                // Format description
                ui.add_space(2.0);
                ui.horizontal(|ui| {
                    let desc = match self.format {
                        SchemaFormat::Postgres => {
                            "Native PostgreSQL DDL with all constraints"
                        }
                        SchemaFormat::Oracle => {
                            "Oracle-compatible types (NUMBER, CLOB, RAW…)"
                        }
                        SchemaFormat::MySQL => "MySQL/MariaDB syntax (backtick quoting)",
                        SchemaFormat::SQLServer => {
                            "T-SQL types (NVARCHAR, UNIQUEIDENTIFIER…)"
                        }
                        SchemaFormat::Databricks => {
                            "Delta Lake CREATE TABLE (no FK/CHECK)"
                        }
                        SchemaFormat::SQLite => {
                            "SQLite affinity types (INTEGER, REAL, TEXT…)"
                        }
                        SchemaFormat::Snowflake => {
                            "Snowflake types (VARIANT, TIMESTAMP_TZ…)"
                        }
                    };
                    ui.colored_label(style::COLOR_MUTED, desc);
                });
                ui.add_space(2.0);

                // Copy / Save row
                if !self.output.is_empty() && !self.generating {
                    ui.horizontal(|ui| {
                        if ui
                            .add(
                                egui::Button::new("📋 Copy")
                                    .min_size(egui::vec2(70.0, 28.0)),
                            )
                            .clicked()
                        {
                            ui.ctx().copy_text(self.output.clone());
                        }
                        let filename = format!(
                            "schema_{}.{}",
                            self.format.label().to_lowercase().replace(' ', "_"),
                            self.format.file_ext()
                        );
                        if ui
                            .add(
                                egui::Button::new("💾 Save")
                                    .min_size(egui::vec2(70.0, 28.0)),
                            )
                            .clicked()
                        {
                            if let Some(path) = rfd::FileDialog::new()
                                .set_file_name(&filename)
                                .save_file()
                            {
                                let _ = std::fs::write(path, &self.output);
                            }
                        }
                        ui.separator();
                        ui.colored_label(
                            style::COLOR_ACCENT,
                            format!(
                                "{} tables  ·  {}",
                                self.selected.len(),
                                self.format.label()
                            ),
                        );
                    });
                    ui.add_space(2.0);
                }

                // Output area
                if self.generating {
                    loading_ui(ui, "Generating schema");
                } else {
                    let output = self.output.clone();
                    output_panel(
                        ui,
                        "schema_gen_output",
                        &output,
                        "Select tables on the left, choose a format, then click ⚙  Generate Schema.",
                    );
                }
            });
        });
    }
}
