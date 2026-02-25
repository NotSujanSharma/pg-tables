//! Views tab — select views and show dependencies.

use crate::ui::components::{
    checkbox_list, filter_upload_row, loading_ui, output_actions, output_panel, search_bar,
    section_header, selection_toolbar,
};
use crate::config::style;
use crate::db;
use eframe::egui;
use sqlx::PgPool;
use std::collections::HashSet;
use std::sync::{Arc, Mutex};

/// State for the Views tab.
pub struct ViewsState {
    pub views: Vec<(String, String)>, // (name, kind)
    pub search: String,
    pub selected: HashSet<String>,
    pub output: String,
    pub loading: bool,       // loading deps
    pub loading_views: bool, // loading view list

    /// Optional name-list uploaded from file — filters the view list.
    pub filter_list: Option<Vec<String>>,

    pending_views: Option<Arc<Mutex<Option<Vec<(String, String)>>>>>,
    pending_deps: Option<Arc<Mutex<Option<String>>>>,
}

impl Default for ViewsState {
    fn default() -> Self {
        Self {
            views: vec![],
            search: String::new(),
            selected: HashSet::new(),
            output: String::new(),
            loading: false,
            loading_views: false,
            filter_list: None,
            pending_views: None,
            pending_deps: None,
        }
    }
}

impl ViewsState {
    pub fn clear(&mut self) {
        self.views.clear();
        self.search.clear();
        self.selected.clear();
        self.output.clear();
        self.loading = false;
        self.loading_views = false;
        self.filter_list = None;
        self.pending_views = None;
        self.pending_deps = None;
    }

    /// Non-blocking: spawn view list load.
    pub fn load_views(&mut self, rt: &tokio::runtime::Runtime, pool: &PgPool, schema: &str) {
        let result: Arc<Mutex<Option<Vec<(String, String)>>>> = Arc::new(Mutex::new(None));
        self.pending_views = Some(result.clone());
        self.loading_views = true;
        self.views.clear();
        self.selected.clear();
        self.output.clear();
        self.search.clear();

        let pool = pool.clone();
        let schema = schema.to_string();
        rt.spawn(async move {
            let views = db::list_views(&pool, &schema).await.unwrap_or_default();
            *result.lock().unwrap() = Some(views);
        });
    }

    /// Non-blocking: spawn dependency analysis.
    pub fn start_load_deps(
        &mut self,
        rt: &tokio::runtime::Runtime,
        pool: &PgPool,
        schema: &str,
    ) {
        let result: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
        self.pending_deps = Some(result.clone());
        self.loading = true;
        self.output.clear();

        let pool = pool.clone();
        let schema = schema.to_string();
        let mut selected: Vec<String> = self.selected.iter().cloned().collect();
        selected.sort();

        rt.spawn(async move {
            let mut output = String::new();
            for view in &selected {
                match db::view_dependencies(&pool, &schema, view).await {
                    Ok((deps, definition)) => {
                        if !output.is_empty() {
                            output.push('\n');
                        }
                        output.push_str(&format!(
                            "═══ Dependencies for '{schema}.{view}' ═══\n\n"
                        ));
                        if deps.is_empty() {
                            output.push_str(
                                "  No dependencies (view may reference only literals or functions).\n",
                            );
                        } else {
                            output.push_str(&format!(
                                "  {:<5} {:<22} {}\n",
                                "#", "Kind", "Object"
                            ));
                            output.push_str(&format!("  {}\n", "─".repeat(55)));
                            for (i, dep) in deps.iter().enumerate() {
                                let qualified = if dep.schema == schema {
                                    dep.name.clone()
                                } else {
                                    format!("{}.{}", dep.schema, dep.name)
                                };
                                output.push_str(&format!(
                                    "  {:<5} {:<22} {}\n",
                                    i + 1,
                                    dep.kind,
                                    qualified
                                ));
                            }
                        }
                        if let Some(def) = definition {
                            output.push_str(&format!(
                                "\n  -- View definition --\n  {}\n",
                                def.replace('\n', "\n  ")
                            ));
                        }
                    }
                    Err(e) => {
                        output.push_str(&format!("-- Error for {view}: {e}\n"));
                    }
                }
            }
            *result.lock().unwrap() = Some(output);
        });
    }

    /// Poll pending async results — call at top of `draw`.
    fn poll(&mut self, ctx: &egui::Context) {
        if self.loading_views {
            ctx.request_repaint();
            let done = {
                let r = self
                    .pending_views
                    .as_ref()
                    .and_then(|p| p.try_lock().ok())
                    .and_then(|mut g| g.take());
                r
            };
            if let Some(views) = done {
                self.views = views;
                self.loading_views = false;
                self.pending_views = None;
            }
        }
        if self.loading {
            ctx.request_repaint();
            let done = {
                let r = self
                    .pending_deps
                    .as_ref()
                    .and_then(|p| p.try_lock().ok())
                    .and_then(|mut g| g.take());
                r
            };
            if let Some(output) = done {
                self.output = output;
                self.loading = false;
                self.pending_deps = None;
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
        let left_width = (available.x * style::PANEL_LEFT_RATIO).max(210.0);

        ui.horizontal(|ui| {
            // ── Left: view selection ──────────────────────────────────────
            ui.vertical(|ui| {
                ui.set_width(left_width);
                ui.set_min_height(available.y);

                let filter_snap: Option<HashSet<String>> = self
                    .filter_list
                    .as_ref()
                    .map(|l| l.iter().cloned().collect());
                let filter_set = &filter_snap;

                let visible_count = self
                    .views
                    .iter()
                    .filter(|(n, _)| {
                        let s = self.search.to_lowercase();
                        let text_ok = s.is_empty() || n.to_lowercase().contains(&s);
                        let filter_ok = filter_set
                            .as_ref()
                            .map(|fs| fs.contains(n.as_str()))
                            .unwrap_or(true);
                        text_ok && filter_ok
                    })
                    .count();

                section_header(ui, "Select Views", visible_count, "shown");
                ui.add_space(3.0);
                search_bar(ui, &mut self.search, "Filter views…");
                ui.add_space(2.0);

                // Filter upload row
                let filter_changed = filter_upload_row(ui, &mut self.filter_list);
                if filter_changed {
                    if let Some(list) = &self.filter_list {
                        let allowed: HashSet<&str> =
                            list.iter().map(|s| s.as_str()).collect();
                        self.selected.retain(|v| allowed.contains(v.as_str()));
                    }
                }
                ui.add_space(2.0);

                let (sel_all, desel_all) = selection_toolbar(ui, self.selected.len());
                if sel_all {
                    let s = self.search.to_lowercase();
                    for (name, _) in &self.views {
                        let text_ok = s.is_empty() || name.to_lowercase().contains(&s);
                        let filter_ok = filter_snap
                            .as_ref()
                            .map(|fs| fs.contains(name.as_str()))
                            .unwrap_or(true);
                        if text_ok && filter_ok {
                            self.selected.insert(name.clone());
                        }
                    }
                }
                if desel_all {
                    self.selected.clear();
                }
                ui.add_space(2.0);

                if self.loading_views {
                    loading_ui(ui, "Loading views");
                } else {
                    let items: Vec<(String, String)> = self
                        .views
                        .iter()
                        .map(|(n, k)| (n.clone(), k.clone()))
                        .collect();
                    let filter_slice = self.filter_list.as_deref();
                    let toggles = checkbox_list(
                        ui,
                        "view_select",
                        &items,
                        &self.selected,
                        &self.search,
                        filter_slice,
                    );
                    for (name, checked) in toggles {
                        if checked {
                            self.selected.insert(name);
                        } else {
                            self.selected.remove(&name);
                        }
                    }
                }
            });

            ui.separator();

            // ── Right: dependency output ──────────────────────────────────
            ui.vertical(|ui| {
                section_header(ui, "Dependencies", 0, "");
                ui.add_space(3.0);

                let can = !self.selected.is_empty();
                let output_ref = self.output.clone();
                let acted = output_actions(
                    ui,
                    "🔍  Show Dependencies",
                    can,
                    self.loading,
                    &output_ref,
                    Some("view_dependencies.txt"),
                );
                if acted {
                    self.start_load_deps(rt, pool, schema);
                }

                ui.add_space(4.0);

                if self.loading {
                    loading_ui(ui, "Analysing dependencies");
                } else {
                    let output = self.output.clone();
                    output_panel(
                        ui,
                        "deps_output",
                        &output,
                        "Select views on the left, then click 🔍  Show Dependencies.",
                    );
                }
            });
        });
    }
}
