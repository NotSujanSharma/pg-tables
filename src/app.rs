use crate::db;
use eframe::egui;
use sqlx::PgPool;
use std::collections::HashSet;
use std::sync::{Arc, Mutex};

// ── Shared async result wrapper ──────────────────────────────────────────────
type Shared<T> = Arc<Mutex<Option<T>>>;

fn shared_none<T>() -> Shared<T> {
    Arc::new(Mutex::new(None))
}

// ── Tabs ─────────────────────────────────────────────────────────────────────
#[derive(PartialEq, Clone, Copy)]
enum Tab {
    Tables,
    CreateScript,
    Views,
}

// ── App State ────────────────────────────────────────────────────────────────
pub struct PgTablesApp {
    // tokio runtime for async operations
    rt: tokio::runtime::Runtime,

    // connection form
    host: String,
    port: String,
    user: String,
    password: String,
    dbname: String,
    connecting: bool,
    conn_error: Option<String>,

    // active connection
    pool: Option<PgPool>,

    // schemas
    schemas: Vec<String>,
    selected_schema: String,
    // tab
    tab: Tab,

    // tables tab
    tables: Vec<String>,
    table_search: String,

    // create script tab
    selected_tables: HashSet<String>,
    script_output: String,
    generating_script: bool,

    // views tab
    views: Vec<(String, String)>, // (name, kind)
    selected_views: HashSet<String>,
    view_deps_output: String,
    loading_deps: bool,
}

impl PgTablesApp {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        Self {
            rt: tokio::runtime::Runtime::new().expect("Failed to create tokio runtime"),
            host: "localhost".into(),
            port: "5432".into(),
            user: "postgres".into(),
            password: String::new(),
            dbname: "postgres".into(),
            connecting: false,
            conn_error: None,
            pool: None,
            schemas: vec![],
            selected_schema: "public".into(),
            tab: Tab::Tables,
            tables: vec![],
            table_search: String::new(),
            selected_tables: HashSet::new(),
            script_output: String::new(),
            generating_script: false,
            views: vec![],
            selected_views: HashSet::new(),
            view_deps_output: String::new(),
            loading_deps: false,
        }
    }

    // ── Async helpers (block_on from sync egui context) ──────────────────────

    fn do_connect(&mut self) {
        self.connecting = true;
        self.conn_error = None;

        let result: Shared<Result<PgPool, String>> = shared_none();
        let res = result.clone();
        let (host, port, user, pw, db) = (
            self.host.clone(),
            self.port.clone(),
            self.user.clone(),
            self.password.clone(),
            self.dbname.clone(),
        );

        self.rt.spawn(async move {
            let r = db::connect(&host, &port, &user, &pw, &db)
                .await
                .map_err(|e| e.to_string());
            *res.lock().unwrap() = Some(r);
        });

        // Poll until done (quick, connect is fast or errors fast)
        loop {
            std::thread::sleep(std::time::Duration::from_millis(50));
            if let Some(r) = result.lock().unwrap().take() {
                match r {
                    Ok(pool) => {
                        self.pool = Some(pool);
                        self.connecting = false;
                        self.load_schemas();
                        return;
                    }
                    Err(e) => {
                        self.conn_error = Some(e);
                        self.connecting = false;
                        return;
                    }
                }
            }
        }
    }

    fn load_schemas(&mut self) {
        if let Some(pool) = &self.pool {
            let pool = pool.clone();
            match self.rt.block_on(db::list_schemas(&pool)) {
                Ok(schemas) => {
                    if !schemas.contains(&self.selected_schema) {
                        if let Some(first) = schemas.first() {
                            self.selected_schema = first.clone();
                        }
                    }
                    self.schemas = schemas;
                }
                Err(e) => {
                    self.schemas = vec!["public".into()];
                    self.conn_error = Some(format!("Failed to load schemas: {e}"));
                }
            }
            self.reload_data();
        }
    }

    fn reload_data(&mut self) {
        if let Some(pool) = &self.pool {
            let pool = pool.clone();
            let schema = self.selected_schema.clone();

            self.tables = self
                .rt
                .block_on(db::list_tables(&pool, &schema))
                .unwrap_or_default();
            self.views = self
                .rt
                .block_on(db::list_views(&pool, &schema))
                .unwrap_or_default();

            // Clear selections when schema changes
            self.selected_tables.clear();
            self.selected_views.clear();
            self.script_output.clear();
            self.view_deps_output.clear();
        }
    }

    fn generate_scripts(&mut self) {
        if let Some(pool) = &self.pool {
            let pool = pool.clone();
            let schema = self.selected_schema.clone();
            let selected: Vec<String> = {
                let mut v: Vec<String> = self.selected_tables.iter().cloned().collect();
                v.sort();
                v
            };

            self.generating_script = true;
            let mut output = String::new();

            for table in &selected {
                match self
                    .rt
                    .block_on(db::generate_create_script(&pool, &schema, table))
                {
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

            self.script_output = output;
            self.generating_script = false;
        }
    }

    fn load_view_deps(&mut self) {
        if let Some(pool) = &self.pool {
            let pool = pool.clone();
            let schema = self.selected_schema.clone();
            let selected: Vec<String> = {
                let mut v: Vec<String> = self.selected_views.iter().cloned().collect();
                v.sort();
                v
            };

            self.loading_deps = true;
            let mut output = String::new();

            for view in &selected {
                match self
                    .rt
                    .block_on(db::view_dependencies(&pool, &schema, view))
                {
                    Ok((deps, definition)) => {
                        if !output.is_empty() {
                            output.push_str("\n");
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

            self.view_deps_output = output;
            self.loading_deps = false;
        }
    }

    fn disconnect(&mut self) {
        self.pool = None;
        self.schemas.clear();
        self.tables.clear();
        self.views.clear();
        self.selected_tables.clear();
        self.selected_views.clear();
        self.script_output.clear();
        self.view_deps_output.clear();
        self.conn_error = None;
    }

    // ── UI Drawing ──────────────────────────────────────────────────────────

    fn draw_login(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(60.0);
                ui.heading("🐘 PG Tables");
                ui.add_space(8.0);
                ui.label("Connect to your PostgreSQL database");
                ui.add_space(24.0);

                let panel_width = 360.0;
                ui.allocate_ui(egui::vec2(panel_width, 0.0), |ui| {
                    egui::Grid::new("login_grid")
                        .num_columns(2)
                        .spacing([12.0, 8.0])
                        .show(ui, |ui| {
                            ui.label("Host:");
                            ui.add(
                                egui::TextEdit::singleline(&mut self.host).desired_width(240.0),
                            );
                            ui.end_row();

                            ui.label("Port:");
                            ui.add(
                                egui::TextEdit::singleline(&mut self.port).desired_width(240.0),
                            );
                            ui.end_row();

                            ui.label("User:");
                            ui.add(
                                egui::TextEdit::singleline(&mut self.user).desired_width(240.0),
                            );
                            ui.end_row();

                            ui.label("Password:");
                            ui.add(
                                egui::TextEdit::singleline(&mut self.password)
                                    .password(true)
                                    .desired_width(240.0),
                            );
                            ui.end_row();

                            ui.label("Database:");
                            ui.add(
                                egui::TextEdit::singleline(&mut self.dbname).desired_width(240.0),
                            );
                            ui.end_row();
                        });
                });

                ui.add_space(16.0);

                let btn = ui.add_enabled(
                    !self.connecting,
                    egui::Button::new(if self.connecting {
                        "Connecting..."
                    } else {
                        "🔌 Connect"
                    })
                    .min_size(egui::vec2(120.0, 32.0)),
                );
                if btn.clicked() {
                    self.do_connect();
                }

                if let Some(err) = &self.conn_error {
                    ui.add_space(12.0);
                    ui.colored_label(egui::Color32::from_rgb(255, 80, 80), format!("⚠ {err}"));
                }
            });
        });
    }

    fn draw_main(&mut self, ctx: &egui::Context) {
        // ── Top bar ──────────────────────────────────────────────────────────
        egui::TopBottomPanel::top("top_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("🐘 PG Tables");
                ui.separator();

                ui.label("Schema:");
                let prev_schema = self.selected_schema.clone();
                egui::ComboBox::from_id_salt("schema_selector")
                    .selected_text(&self.selected_schema)
                    .show_ui(ui, |ui| {
                        for s in &self.schemas {
                            ui.selectable_value(&mut self.selected_schema, s.clone(), s);
                        }
                    });
                if self.selected_schema != prev_schema {
                    self.reload_data();
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("⏏ Disconnect").clicked() {
                        self.disconnect();
                    }
                    ui.label(format!(
                        "{}@{}:{}/{}",
                        self.user, self.host, self.port, self.dbname
                    ));
                });
            });
        });

        // ── Tab bar ──────────────────────────────────────────────────────────
        egui::TopBottomPanel::top("tab_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.tab, Tab::Tables, "📋 Tables");
                ui.selectable_value(&mut self.tab, Tab::CreateScript, "📝 Create Script");
                ui.selectable_value(&mut self.tab, Tab::Views, "👁 Views");
            });
        });

        // ── Content ──────────────────────────────────────────────────────────
        egui::CentralPanel::default().show(ctx, |ui| match self.tab {
            Tab::Tables => self.draw_tables_tab(ui),
            Tab::CreateScript => self.draw_create_script_tab(ui),
            Tab::Views => self.draw_views_tab(ui),
        });
    }

    fn draw_tables_tab(&mut self, ui: &mut egui::Ui) {
        ui.heading("Tables");
        ui.add_space(4.0);

        ui.horizontal(|ui| {
            ui.label("🔍");
            ui.add(
                egui::TextEdit::singleline(&mut self.table_search)
                    .hint_text("Search tables...")
                    .desired_width(300.0),
            );
            ui.label(format!("{} table(s)", self.tables.len()));
        });

        ui.add_space(4.0);
        ui.separator();

        let search = self.table_search.to_lowercase();

        egui::ScrollArea::vertical().show(ui, |ui| {
            let mut count = 0u32;
            for name in &self.tables {
                if !search.is_empty() && !name.to_lowercase().contains(&search) {
                    continue;
                }
                count += 1;
                ui.horizontal(|ui| {
                    ui.label(format!("{count}."));
                    ui.monospace(name);
                });
            }
            if count == 0 {
                ui.label("No tables match your search.");
            }
        });
    }

    fn draw_create_script_tab(&mut self, ui: &mut egui::Ui) {
        ui.heading("Generate CREATE Scripts");
        ui.add_space(4.0);

        // Two-panel layout: left = table selection, right = output
        let available = ui.available_size();

        ui.horizontal(|ui| {
            // ── Left panel: table selection ───────────────────────────────
            ui.vertical(|ui| {
                ui.set_width(available.x * 0.35);
                ui.label("Select tables:");
                ui.add_space(4.0);

                ui.horizontal(|ui| {
                    if ui.button("Select All").clicked() {
                        for t in &self.tables {
                            self.selected_tables.insert(t.clone());
                        }
                    }
                    if ui.button("Clear").clicked() {
                        self.selected_tables.clear();
                    }
                });
                ui.add_space(4.0);

                egui::ScrollArea::vertical()
                    .id_salt("table_select_scroll")
                    .show(ui, |ui| {
                        for name in &self.tables {
                            let mut checked = self.selected_tables.contains(name);
                            if ui.checkbox(&mut checked, name).changed() {
                                if checked {
                                    self.selected_tables.insert(name.clone());
                                } else {
                                    self.selected_tables.remove(name);
                                }
                            }
                        }
                    });
            });

            ui.separator();

            // ── Right panel: output ──────────────────────────────────────
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    let btn_text = if self.generating_script {
                        "Generating..."
                    } else {
                        "⚙ Generate"
                    };
                    if ui
                        .add_enabled(
                            !self.selected_tables.is_empty() && !self.generating_script,
                            egui::Button::new(btn_text),
                        )
                        .clicked()
                    {
                        self.generate_scripts();
                    }

                    if !self.script_output.is_empty() {
                        if ui.button("📋 Copy").clicked() {
                            ui.ctx().copy_text(self.script_output.clone());
                        }
                        if ui.button("💾 Save").clicked() {
                            if let Some(path) = rfd::FileDialog::new()
                                .set_file_name("create_tables.sql")
                                .save_file()
                            {
                                let _ = std::fs::write(path, &self.script_output);
                            }
                        }
                    }
                });
                ui.add_space(4.0);

                egui::ScrollArea::vertical()
                    .id_salt("script_output_scroll")
                    .show(ui, |ui| {
                        if self.script_output.is_empty() {
                            ui.label("Select tables and click Generate.");
                        } else {
                            ui.add(
                                egui::TextEdit::multiline(&mut self.script_output.as_str())
                                    .font(egui::TextStyle::Monospace)
                                    .desired_width(f32::INFINITY),
                            );
                        }
                    });
            });
        });
    }

    fn draw_views_tab(&mut self, ui: &mut egui::Ui) {
        ui.heading("Views & Dependencies");
        ui.add_space(4.0);

        let available = ui.available_size();

        ui.horizontal(|ui| {
            // ── Left panel: view selection ────────────────────────────────
            ui.vertical(|ui| {
                ui.set_width(available.x * 0.35);
                ui.label("Select views:");
                ui.add_space(4.0);

                ui.horizontal(|ui| {
                    if ui.button("Select All").clicked() {
                        for (name, _) in &self.views {
                            self.selected_views.insert(name.clone());
                        }
                    }
                    if ui.button("Clear").clicked() {
                        self.selected_views.clear();
                    }
                });
                ui.add_space(4.0);

                egui::ScrollArea::vertical()
                    .id_salt("view_select_scroll")
                    .show(ui, |ui| {
                        for (name, kind) in &self.views {
                            let mut checked = self.selected_views.contains(name);
                            let label = format!("{name}  ({kind})");
                            if ui.checkbox(&mut checked, label).changed() {
                                if checked {
                                    self.selected_views.insert(name.clone());
                                } else {
                                    self.selected_views.remove(name);
                                }
                            }
                        }
                        if self.views.is_empty() {
                            ui.label("No views in this schema.");
                        }
                    });
            });

            ui.separator();

            // ── Right panel: dependency output ───────────────────────────
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    let btn_text = if self.loading_deps {
                        "Loading..."
                    } else {
                        "🔍 Show Dependencies"
                    };
                    if ui
                        .add_enabled(
                            !self.selected_views.is_empty() && !self.loading_deps,
                            egui::Button::new(btn_text),
                        )
                        .clicked()
                    {
                        self.load_view_deps();
                    }

                    if !self.view_deps_output.is_empty() {
                        if ui.button("📋 Copy").clicked() {
                            ui.ctx().copy_text(self.view_deps_output.clone());
                        }
                    }
                });
                ui.add_space(4.0);

                egui::ScrollArea::vertical()
                    .id_salt("deps_output_scroll")
                    .show(ui, |ui| {
                        if self.view_deps_output.is_empty() {
                            ui.label("Select views and click Show Dependencies.");
                        } else {
                            ui.add(
                                egui::TextEdit::multiline(&mut self.view_deps_output.as_str())
                                    .font(egui::TextStyle::Monospace)
                                    .desired_width(f32::INFINITY),
                            );
                        }
                    });
            });
        });
    }
}

impl eframe::App for PgTablesApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if self.pool.is_none() {
            self.draw_login(ctx);
        } else {
            self.draw_main(ctx);
        }
    }
}
