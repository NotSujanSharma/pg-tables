//! Create Script tab — select tables and generate DDL.

use crate::components::{
    checkbox_list, filter_upload_row, loading_ui, output_actions, output_panel, search_bar,
    section_header, selection_toolbar,
};
use crate::db;
use crate::style;
use eframe::egui;
use sqlx::PgPool;
use std::collections::HashSet;
use std::sync::{Arc, Mutex};

/// State for the Create Script tab.
pub struct CreateScriptState {
    pub search: String,
    pub selected: HashSet<String>,
    pub output: String,
    pub generating: bool,

    /// Optional name-list uploaded from file — filters the table list.
    pub filter_list: Option<Vec<String>>,

    pending: Option<Arc<Mutex<Option<String>>>>,
}

impl Default for CreateScriptState {
    fn default() -> Self {
        Self {
            search: String::new(),
            selected: HashSet::new(),
            output: String::new(),
            generating: false,
            filter_list: None,
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

    /// Kick off async DDL generation — does not block the UI thread.
    pub fn start_generate(
        &mut self,
        rt: &tokio::runtime::Runtime,
        pool: &PgPool,
        schema: &str,
    ) {
        let result: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
        self.pending = Some(result.clone());
        self.generating = true;
        self.output.clear();

        let pool = pool.clone();
        let schema = schema.to_string();
        let mut selected: Vec<String> = self.selected.iter().cloned().collect();
        selected.sort();

        rt.spawn(async move {
            let mut output = String::new();
            for table in &selected {
                match db::generate_create_script(&pool, &schema, table).await {
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
            *result.lock().unwrap() = Some(output);
        });
    }

    /// Poll the pending async result — call every frame while generating.
    fn poll(&mut self, ctx: &egui::Context) {
        if self.generating {
            ctx.request_repaint();
            let done = {
                let r = self.pending.as_ref()
                    .and_then(|p| p.try_lock().ok())
                    .and_then(|mut g| g.take());
                r
            };
            if let Some(output) = done {
                self.output = output;
                self.generating = false;
                self.pending = None;
            }
        }
    }

    pub fn draw(
        &mut self,
        ui: &mut egui::Ui,
        tables: &[String],
        rt: &tokio::runtime::Runtime,
        pool: &PgPool,
        schema: &str,
    ) {
        self.poll(ui.ctx());

        let available = ui.available_size();
        let left_width = (available.x * style::PANEL_LEFT_RATIO).max(210.0);

        ui.horizontal(|ui| {
            // ── Left: table selection ─────────────────────────────────────
            ui.vertical(|ui| {
                ui.set_width(left_width);
                ui.set_min_height(available.y);

                // Count after filter + search — clone so we can still mutably
                // borrow self.filter_list later for the upload row.
                let filter_snap: Option<HashSet<String>> = self
                    .filter_list
                    .as_ref()
                    .map(|l| l.iter().cloned().collect());
                let filter_set = &filter_snap;

                let visible_count = tables
                    .iter()
                    .filter(|t| {
                        let s = self.search.to_lowercase();
                        let text_ok = s.is_empty() || t.to_lowercase().contains(&s);
                        let filter_ok = filter_set
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

                // Filter upload row
                let filter_changed = filter_upload_row(ui, &mut self.filter_list);
                if filter_changed {
                    // Clear selection that no longer matches the new filter
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
                // filter_snap is already dropped (not held), safe to re-borrow filter_list
                let filter_for_list = self.filter_list.as_deref();
                let toggles =
                    checkbox_list(ui, "script_table_select", &items, &self.selected, &self.search, filter_for_list);
                for (name, checked) in toggles {
                    if checked {
                        self.selected.insert(name);
                    } else {
                        self.selected.remove(&name);
                    }
                }
            });

            ui.separator();

            // ── Right: output ─────────────────────────────────────────────
            ui.vertical(|ui| {
                section_header(ui, "Generated DDL", 0, "");
                ui.add_space(3.0);

                let can = !self.selected.is_empty();
                let output_ref = self.output.clone();
                let acted = output_actions(
                    ui,
                    "⚙  Generate Script",
                    can,
                    self.generating,
                    &output_ref,
                    Some("create_tables.sql"),
                );
                if acted {
                    self.start_generate(rt, pool, schema);
                }

                ui.add_space(4.0);

                if self.generating {
                    loading_ui(ui, "Generating DDL");
                } else {
                    let output = self.output.clone();
                    output_panel(
                        ui,
                        "script_output",
                        &output,
                        "Select tables on the left, then click ⚙  Generate Script.",
                    );
                }
            });
        });
    }
}

