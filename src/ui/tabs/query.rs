//! SQL Query tab — execute arbitrary SQL and view results.

use crate::ui::components::section_header;
use crate::config::style;
use crate::db;
use eframe::egui;
use sqlx::PgPool;

/// State for the SQL Query tab.
pub struct QueryState {
    pub sql: String,
    pub result: Option<db::QueryResult>,
    pub error: Option<String>,
    pub executing: bool,
    pub history: Vec<String>,
}

impl Default for QueryState {
    fn default() -> Self {
        Self {
            sql: String::new(),
            result: None,
            error: None,
            executing: false,
            history: vec![],
        }
    }
}

impl QueryState {
    pub fn clear(&mut self) {
        self.sql.clear();
        self.result = None;
        self.error = None;
        self.history.clear();
    }

    pub fn execute(&mut self, rt: &tokio::runtime::Runtime, pool: &PgPool) {
        let sql = self.sql.trim().to_string();
        if sql.is_empty() {
            return;
        }
        self.executing = true;
        self.error = None;

        match rt.block_on(db::execute_query(pool, &sql)) {
            Ok(result) => {
                self.result = Some(result);
                // Add to history (avoid duplicates at top)
                if self.history.first() != Some(&sql) {
                    self.history.insert(0, sql);
                    if self.history.len() > 50 {
                        self.history.pop();
                    }
                }
            }
            Err(e) => {
                self.error = Some(e);
                self.result = None;
            }
        }

        self.executing = false;
    }

    pub fn draw(
        &mut self,
        ui: &mut egui::Ui,
        rt: &tokio::runtime::Runtime,
        pool: &PgPool,
    ) {
        let available = ui.available_size();

        // Top section: SQL editor (roughly 35% height)
        let editor_height = (available.y * 0.30).max(100.0);

        ui.vertical(|ui| {
            // SQL input area
            section_header(ui, "SQL Query", 0, "");
            ui.add_space(2.0);

            ui.horizontal(|ui| {
                let btn_text = if self.executing {
                    "⏳ Running..."
                } else {
                    "▶ Execute"
                };
                if ui
                    .add_enabled(
                        !self.sql.trim().is_empty() && !self.executing,
                        egui::Button::new(btn_text),
                    )
                    .clicked()
                {
                    self.execute(rt, pool);
                }

                if ui.small_button("🗑 Clear").clicked() {
                    self.sql.clear();
                    self.result = None;
                    self.error = None;
                }

                if !self.history.is_empty() {
                    egui::ComboBox::from_id_salt("query_history")
                        .selected_text("📜 History")
                        .width(120.0)
                        .show_ui(ui, |ui| {
                            for h in &self.history.clone() {
                                let display: String = h.chars().take(60).collect();
                                if ui.selectable_label(false, &display).clicked() {
                                    self.sql = h.clone();
                                }
                            }
                        });
                }

                // Copy result
                if let Some(ref result) = self.result {
                    if result.is_select && !result.rows.is_empty() {
                        ui.separator();
                        if ui.small_button("📋 Copy Results").clicked() {
                            let text = format_result_text(result);
                            ui.ctx().copy_text(text);
                        }
                        if ui.small_button("💾 Save Results").clicked() {
                            if let Some(path) = rfd::FileDialog::new()
                                .set_file_name("query_results.csv")
                                .save_file()
                            {
                                let csv = format_result_csv(result);
                                let _ = std::fs::write(path, csv);
                            }
                        }
                    }
                }
            });

            ui.add_space(4.0);

            egui::ScrollArea::vertical()
                .id_salt("sql_editor_scroll")
                .max_height(editor_height)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.add(
                        egui::TextEdit::multiline(&mut self.sql)
                            .font(egui::TextStyle::Monospace)
                            .desired_width(f32::INFINITY)
                            .desired_rows(6)
                            .hint_text(
                                "Enter SQL query here... (e.g. SELECT * FROM users LIMIT 10)",
                            ),
                    );
                });

            ui.add_space(4.0);
            ui.separator();
            ui.add_space(4.0);

            // Results
            if let Some(ref err) = self.error {
                ui.colored_label(style::COLOR_ERROR, format!("⚠ Error: {err}"));
            } else if let Some(ref result) = self.result {
                if result.is_select {
                    ui.horizontal(|ui| {
                        ui.strong("Results");
                        ui.colored_label(
                            style::COLOR_MUTED,
                            format!(
                                "{} row(s), {} column(s)",
                                result.rows.len(),
                                result.columns.len()
                            ),
                        );
                    });
                    ui.add_space(4.0);

                    if result.rows.is_empty() {
                        ui.colored_label(style::COLOR_MUTED, "Query returned 0 rows.");
                    } else {
                        egui::ScrollArea::both()
                            .id_salt("query_results_scroll")
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                egui::Grid::new("query_results_grid")
                                    .num_columns(result.columns.len())
                                    .spacing([12.0, 3.0])
                                    .striped(true)
                                    .show(ui, |ui| {
                                        // Header
                                        for col in &result.columns {
                                            ui.colored_label(style::COLOR_HEADER, col);
                                        }
                                        ui.end_row();

                                        // Rows (limit display to 1000)
                                        for row in result.rows.iter().take(1000) {
                                            for cell in row {
                                                if cell == "NULL" {
                                                    ui.colored_label(
                                                        style::COLOR_NULL_BADGE,
                                                        "NULL",
                                                    );
                                                } else {
                                                    ui.monospace(cell);
                                                }
                                            }
                                            ui.end_row();
                                        }
                                    });

                                if result.rows.len() > 1000 {
                                    ui.add_space(4.0);
                                    ui.colored_label(
                                        style::COLOR_MUTED,
                                        format!(
                                            "... showing 1000 of {} rows",
                                            result.rows.len()
                                        ),
                                    );
                                }
                            });
                    }
                } else {
                    ui.colored_label(
                        style::COLOR_SUCCESS,
                        format!(
                            "✓ Statement executed. {} row(s) affected.",
                            result.affected
                        ),
                    );
                }
            } else {
                ui.centered_and_justified(|ui| {
                    ui.colored_label(
                        style::COLOR_MUTED,
                        "Enter a query and click ▶ Execute.",
                    );
                });
            }
        });
    }
}

fn format_result_text(result: &db::QueryResult) -> String {
    let mut out = result.columns.join("\t") + "\n";
    for row in &result.rows {
        out.push_str(&row.join("\t"));
        out.push('\n');
    }
    out
}

fn format_result_csv(result: &db::QueryResult) -> String {
    let mut out = result.columns.join(",") + "\n";
    for row in &result.rows {
        let escaped: Vec<String> = row
            .iter()
            .map(|c| {
                if c.contains(',') || c.contains('"') || c.contains('\n') {
                    format!("\"{}\"", c.replace('"', "\"\""))
                } else {
                    c.clone()
                }
            })
            .collect();
        out.push_str(&escaped.join(","));
        out.push('\n');
    }
    out
}
