//! Create Script tab — select tables and generate DDL.

use crate::components::{checkbox_list, output_actions, output_panel, search_bar, section_header, selection_toolbar};
use crate::db;
use crate::style;
use eframe::egui;
use sqlx::PgPool;
use std::collections::HashSet;

/// State for the Create Script tab.
pub struct CreateScriptState {
    pub search: String,
    pub selected: HashSet<String>,
    pub output: String,
    pub generating: bool,
}

impl Default for CreateScriptState {
    fn default() -> Self {
        Self {
            search: String::new(),
            selected: HashSet::new(),
            output: String::new(),
            generating: false,
        }
    }
}

impl CreateScriptState {
    pub fn clear(&mut self) {
        self.search.clear();
        self.selected.clear();
        self.output.clear();
    }

    pub fn generate(
        &mut self,
        rt: &tokio::runtime::Runtime,
        pool: &PgPool,
        schema: &str,
    ) {
        let mut selected: Vec<String> = self.selected.iter().cloned().collect();
        selected.sort();
        self.generating = true;
        let mut output = String::new();

        for table in &selected {
            match rt.block_on(db::generate_create_script(pool, schema, table)) {
                Ok(script) => {
                    if !output.is_empty() {
                        output.push_str("\n\n");
                    }
                    output.push_str(&script);
                }
                Err(e) => {
                    output.push_str(&format!("-- Error for {table}: {e}\n"));
                }
            }
        }

        self.output = output;
        self.generating = false;
    }

    pub fn draw(
        &mut self,
        ui: &mut egui::Ui,
        tables: &[String],
        rt: &tokio::runtime::Runtime,
        pool: &PgPool,
        schema: &str,
    ) {
        let available = ui.available_size();
        let left_width = (available.x * style::PANEL_LEFT_RATIO).max(200.0);

        ui.horizontal(|ui| {
            // Left: table selection
            ui.vertical(|ui| {
                ui.set_width(left_width);
                ui.set_min_height(available.y);

                section_header(ui, "Select Tables", tables.len(), "total");
                ui.add_space(2.0);
                search_bar(ui, &mut self.search, "Filter tables...");
                ui.add_space(2.0);

                let (select_all, clear) = selection_toolbar(ui, self.selected.len());
                if select_all {
                    let s = self.search.to_lowercase();
                    for t in tables {
                        if s.is_empty() || t.to_lowercase().contains(&s) {
                            self.selected.insert(t.clone());
                        }
                    }
                }
                if clear {
                    self.selected.clear();
                }
                ui.add_space(2.0);

                let items: Vec<(String, String)> =
                    tables.iter().map(|t| (t.clone(), String::new())).collect();
                let toggles = checkbox_list(ui, "script_table_select", &items, &self.selected, &self.search);
                for (name, checked) in toggles {
                    if checked {
                        self.selected.insert(name);
                    } else {
                        self.selected.remove(&name);
                    }
                }
            });

            ui.separator();

            // Right: output
            ui.vertical(|ui| {
                section_header(ui, "Generated DDL", 0, "");
                ui.add_space(2.0);

                let can = !self.selected.is_empty();
                let output_ref = self.output.clone();
                let acted = output_actions(
                    ui,
                    "⚙ Generate",
                    can,
                    self.generating,
                    &output_ref,
                    Some("create_tables.sql"),
                );
                if acted {
                    self.generate(rt, pool, schema);
                }

                ui.add_space(4.0);
                let output = self.output.clone();
                output_panel(ui, "script_output", &output, "Select tables and click ⚙ Generate.");
            });
        });
    }
}
