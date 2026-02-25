//! Fake Data tab — select tables, configure row count, generate & export CSV.

use crate::ui::components::{
    checkbox_list, filter_upload_row, loading_ui, output_actions, output_panel, search_bar,
    section_header, selection_toolbar,
};
use crate::config::style;
use crate::db::{self, FakeColumnInfo};
use crate::faker;
use eframe::egui;
use sqlx::PgPool;
use std::collections::HashSet;
use std::sync::{Arc, Mutex};

// ── State ─────────────────────────────────────────────────────────────────────

/// Per-table column metadata cache — populated the first time a table is
/// selected or when the schema/connection changes.
type ColCache = Arc<Mutex<std::collections::HashMap<String, Vec<FakeColumnInfo>>>>;

pub struct FakeDataState {
    // ── Left panel ────────────────────────────────────────────────────────
    pub search: String,
    pub selected: HashSet<String>,
    pub filter_list: Option<Vec<String>>,

    // ── Row count ─────────────────────────────────────────────────────────
    pub row_count_str: String,

    // ── Output ────────────────────────────────────────────────────────────
    pub output: String,
    pub generating: bool,

    // ── Internals ─────────────────────────────────────────────────────────
    /// Column metadata cache – keyed by table name.
    col_cache: ColCache,
    /// Channel for async generation result.
    pending: Option<Arc<Mutex<Option<String>>>>,
}

impl Default for FakeDataState {
    fn default() -> Self {
        Self {
            search: String::new(),
            selected: HashSet::new(),
            filter_list: None,
            row_count_str: "10".into(),
            output: String::new(),
            generating: false,
            col_cache: Arc::new(Mutex::new(std::collections::HashMap::new())),
            pending: None,
        }
    }
}

impl FakeDataState {
    pub fn clear(&mut self) {
        self.search.clear();
        self.selected.clear();
        self.output.clear();
        self.filter_list = None;
        self.generating = false;
        self.pending = None;
        self.col_cache.lock().unwrap().clear();
    }

    // ── Async generation ──────────────────────────────────────────────────

    /// Kick off async fake-data generation — does not block the UI thread.
    pub fn start_generate(
        &mut self,
        rt: &tokio::runtime::Runtime,
        pool: &PgPool,
        schema: &str,
    ) {
        let row_count: usize = self
            .row_count_str
            .trim()
            .parse()
            .unwrap_or(10)
            .clamp(1, 10_000);

        let result: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
        self.pending = Some(result.clone());
        self.generating = true;
        self.output.clear();

        let pool = pool.clone();
        let schema = schema.to_string();
        let mut tables: Vec<String> = self.selected.iter().cloned().collect();
        tables.sort();
        let col_cache = self.col_cache.clone();

        rt.spawn(async move {
            let mut all_csv = String::new();

            for table in &tables {
                // Try cache first, then fetch from DB
                let cols_opt = col_cache.lock().unwrap().get(table).cloned();
                let cols = match cols_opt {
                    Some(c) => c,
                    None => {
                        match db::fetch_fake_columns(&pool, &schema, table).await {
                            Ok(c) => {
                                col_cache.lock().unwrap().insert(table.clone(), c.clone());
                                c
                            }
                            Err(e) => {
                                all_csv.push_str(&format!(
                                    "# Error fetching columns for {table}: {e}\n"
                                ));
                                continue;
                            }
                        }
                    }
                };

                if cols.is_empty() {
                    all_csv.push_str(&format!("# Table '{table}' has no columns.\n"));
                    continue;
                }

                // Section header comment
                if !all_csv.is_empty() {
                    all_csv.push('\n');
                }
                all_csv.push_str(&format!("# Table: {table}  ({} rows)\n", row_count));

                // Generate CSV
                let csv = faker::generate_csv(&cols, row_count);
                all_csv.push_str(&csv);
            }

            *result.lock().unwrap() = Some(all_csv);
        });
    }

    /// Poll the async task every frame while generating.
    fn poll(&mut self, ctx: &egui::Context) {
        if self.generating {
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
    }

    // ── Draw ──────────────────────────────────────────────────────────────

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

                // Snapshot filter for count + checkbox_list
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
                    "fake_table_select",
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

            // ── Right: configuration + output ─────────────────────────────
            ui.vertical(|ui| {
                section_header(ui, "Generated Fake Data (CSV)", 0, "");
                ui.add_space(3.0);

                // Row count input
                ui.horizontal(|ui| {
                    ui.label("Rows per table:");
                    let field = ui.add(
                        egui::TextEdit::singleline(&mut self.row_count_str)
                            .desired_width(70.0)
                            .hint_text("10"),
                    );
                    // Keep only digits
                    if field.changed() {
                        self.row_count_str.retain(|c| c.is_ascii_digit());
                        if self.row_count_str.is_empty() {
                            self.row_count_str = "10".into();
                        }
                    }
                    let rows: usize =
                        self.row_count_str.parse().unwrap_or(10).clamp(1, 10_000);
                    ui.colored_label(
                        style::COLOR_MUTED,
                        format!("(max 10,000 — will generate {rows})"),
                    );
                });
                ui.add_space(4.0);

                let can = !self.selected.is_empty();
                let output_ref = self.output.clone();
                let acted = output_actions(
                    ui,
                    "🎲  Generate Fake Data",
                    can,
                    self.generating,
                    &output_ref,
                    Some("fake_data.csv"),
                );
                if acted {
                    self.start_generate(rt, pool, schema);
                }

                ui.add_space(4.0);

                if self.generating {
                    loading_ui(ui, "Generating fake data");
                } else {
                    let output = self.output.clone();
                    output_panel(
                        ui,
                        "fake_data_output",
                        &output,
                        "Select tables on the left, set row count, then click 🎲  Generate Fake Data.",
                    );
                }
            });
        });
    }
}
