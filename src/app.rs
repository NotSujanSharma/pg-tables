//! Main application state and routing — delegates to tab modules.

use crate::components;
use crate::db;
use crate::session::Session;
use crate::style;
use crate::tabs;
use eframe::egui;
use sqlx::PgPool;
use std::sync::{Arc, Mutex};

type Shared<T> = Arc<Mutex<Option<T>>>;
fn shared_none<T>() -> Shared<T> {
    Arc::new(Mutex::new(None))
}

#[derive(PartialEq, Clone, Copy)]
enum Tab {
    Tables,
    CreateScript,
    Views,
    Query,
}

pub struct PgTablesApp {
    rt: tokio::runtime::Runtime,

    // connection form
    host: String,
    port: String,
    user: String,
    password: String,
    dbname: String,
    connecting: bool,
    conn_error: Option<String>,
    remember_session: bool,

    // active connection
    pool: Option<PgPool>,

    // schemas
    schemas: Vec<String>,
    selected_schema: String,

    // navigation
    tab: Tab,

    // tab states (each tab owns its own data)
    tables_state: tabs::tables::TablesState,
    create_script_state: tabs::create_script::CreateScriptState,
    views_state: tabs::views::ViewsState,
    query_state: tabs::query::QueryState,
}

// ── Construction & session ───────────────────────────────────────────────────
impl PgTablesApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        style::setup_visuals(&cc.egui_ctx);
        let session = Session::load().unwrap_or_default();
        let has_session = Session::load().is_some();

        Self {
            rt: tokio::runtime::Runtime::new().expect("Failed to create tokio runtime"),
            host: session.host,
            port: session.port,
            user: session.user,
            password: session.password,
            dbname: session.dbname,
            connecting: false,
            conn_error: None,
            remember_session: has_session,
            pool: None,
            schemas: vec![],
            selected_schema: session.schema,
            tab: Tab::Tables,
            tables_state: Default::default(),
            create_script_state: Default::default(),
            views_state: Default::default(),
            query_state: Default::default(),
        }
    }

    fn save_session(&self) {
        if self.remember_session {
            let s = Session {
                host: self.host.clone(),
                port: self.port.clone(),
                user: self.user.clone(),
                password: self.password.clone(),
                dbname: self.dbname.clone(),
                schema: self.selected_schema.clone(),
            };
            s.save();
        } else {
            Session::clear();
        }
    }
}

// ── Database operations ──────────────────────────────────────────────────────
impl PgTablesApp {
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

        loop {
            std::thread::sleep(std::time::Duration::from_millis(50));
            if let Some(r) = result.lock().unwrap().take() {
                match r {
                    Ok(pool) => {
                        self.pool = Some(pool);
                        self.connecting = false;
                        self.save_session();
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
                    self.conn_error = Some(format!("Schema load error: {e}"));
                }
            }
            self.reload_data();
        }
    }

    fn reload_data(&mut self) {
        if let Some(pool) = &self.pool {
            let schema = self.selected_schema.clone();
            // Both load_tables and load_views are now non-blocking (spawn async)
            self.tables_state.load_tables(&self.rt, pool, &schema);
            self.views_state.load_views(&self.rt, pool, &schema);
            self.create_script_state.clear();
            self.query_state.result = None;
            self.query_state.error = None;
            self.save_session();
        }
    }

    fn disconnect(&mut self) {
        self.pool = None;
        self.schemas.clear();
        self.tables_state.clear();
        self.create_script_state.clear();
        self.views_state.clear();
        self.query_state.clear();
        self.conn_error = None;
    }
}

// ── UI Drawing ───────────────────────────────────────────────────────────────
impl PgTablesApp {
    fn draw_login(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(70.0);

                ui.add(egui::Label::new(
                    egui::RichText::new("🐘  PG Tables")
                        .size(30.0)
                        .strong()
                        .color(style::COLOR_ACCENT),
                ));
                ui.add_space(4.0);
                ui.colored_label(
                    style::COLOR_MUTED,
                    "Connect to your PostgreSQL database",
                );
                ui.add_space(28.0);

                ui.allocate_ui(egui::vec2(style::LOGIN_PANEL_WIDTH, 0.0), |ui| {
                    egui::Frame::NONE
                        .inner_margin(egui::Margin::same(24))
                        .corner_radius(10.0)
                        .fill(egui::Color32::from_rgb(32, 32, 40))
                        .stroke(egui::Stroke::new(
                            1.0,
                            egui::Color32::from_rgb(55, 58, 75),
                        ))
                        .show(ui, |ui| {
                            egui::Grid::new("login_grid")
                                .num_columns(2)
                                .spacing([14.0, 10.0])
                                .show(ui, |ui| {
                                    ui.label("Host:");
                                    ui.add(
                                        egui::TextEdit::singleline(&mut self.host)
                                            .desired_width(style::LOGIN_FIELD_WIDTH)
                                            .hint_text("localhost"),
                                    );
                                    ui.end_row();

                                    ui.label("Port:");
                                    ui.add(
                                        egui::TextEdit::singleline(&mut self.port)
                                            .desired_width(style::LOGIN_FIELD_WIDTH)
                                            .hint_text("5432"),
                                    );
                                    ui.end_row();

                                    ui.label("User:");
                                    ui.add(
                                        egui::TextEdit::singleline(&mut self.user)
                                            .desired_width(style::LOGIN_FIELD_WIDTH),
                                    );
                                    ui.end_row();

                                    ui.label("Password:");
                                    ui.add(
                                        egui::TextEdit::singleline(&mut self.password)
                                            .password(true)
                                            .desired_width(style::LOGIN_FIELD_WIDTH),
                                    );
                                    ui.end_row();

                                    ui.label("Database:");
                                    ui.add(
                                        egui::TextEdit::singleline(&mut self.dbname)
                                            .desired_width(style::LOGIN_FIELD_WIDTH),
                                    );
                                    ui.end_row();
                                });

                            ui.add_space(12.0);
                            ui.separator();
                            ui.add_space(10.0);

                            ui.horizontal(|ui| {
                                ui.checkbox(
                                    &mut self.remember_session,
                                    "Remember credentials",
                                );
                            });
                            ui.add_space(14.0);

                            ui.vertical_centered(|ui| {
                                let label = if self.connecting {
                                    "⏳  Connecting…"
                                } else {
                                    "🔌  Connect"
                                };
                                let btn = ui.add_enabled(
                                    !self.connecting,
                                    egui::Button::new(
                                        egui::RichText::new(label).size(14.5),
                                    )
                                    .min_size(egui::vec2(160.0, 36.0)),
                                );
                                if btn.clicked() {
                                    self.do_connect();
                                }
                            });
                        });
                });

                if let Some(err) = &self.conn_error.clone() {
                    ui.add_space(16.0);
                    egui::Frame::NONE
                        .inner_margin(egui::Margin::symmetric(12, 8))
                        .corner_radius(6.0)
                        .fill(egui::Color32::from_rgb(60, 24, 24))
                        .stroke(egui::Stroke::new(
                            1.0,
                            egui::Color32::from_rgb(140, 50, 50),
                        ))
                        .show(ui, |ui| {
                            ui.colored_label(
                                style::COLOR_ERROR,
                                format!("⚠  {err}"),
                            );
                        });
                }
            });
        });
    }

    fn draw_main(&mut self, ctx: &egui::Context) {
        // ── Top bar ───────────────────────────────────────────────────────
        egui::TopBottomPanel::top("top_bar")
            .frame(
                egui::Frame::side_top_panel(&ctx.style())
                    .inner_margin(egui::Margin::symmetric(10, 7)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.add(egui::Label::new(
                        egui::RichText::new("🐘  PG Tables")
                            .size(17.0)
                            .strong()
                            .color(style::COLOR_ACCENT),
                    ));
                    ui.separator();

                    ui.colored_label(style::COLOR_MUTED, "Schema:");
                    let prev_schema = self.selected_schema.clone();
                    egui::ComboBox::from_id_salt("schema_selector")
                        .selected_text(&self.selected_schema)
                        .width(150.0)
                        .show_ui(ui, |ui| {
                            for s in &self.schemas {
                                ui.selectable_value(
                                    &mut self.selected_schema,
                                    s.clone(),
                                    s,
                                );
                            }
                        });
                    if self.selected_schema != prev_schema {
                        self.reload_data();
                    }

                    ui.with_layout(
                        egui::Layout::right_to_left(egui::Align::Center),
                        |ui| {
                            if ui
                                .add(
                                    egui::Button::new("⏏  Disconnect")
                                        .min_size(egui::vec2(0.0, 26.0)),
                                )
                                .clicked()
                            {
                                self.disconnect();
                            }
                            ui.separator();
                            ui.colored_label(
                                style::COLOR_MUTED,
                                format!(
                                    "{}@{}:{}/{}",
                                    self.user, self.host, self.port, self.dbname
                                ),
                            );
                        },
                    );
                });
            });

        // ── Tab bar ───────────────────────────────────────────────────────
        egui::TopBottomPanel::top("tab_bar")
            .frame(
                egui::Frame::side_top_panel(&ctx.style())
                    .inner_margin(egui::Margin::symmetric(10, 4)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let tabs = [
                        (Tab::Tables,       "📋  Tables"),
                        (Tab::CreateScript, "📝  Create Script"),
                        (Tab::Views,        "👁  Views"),
                        (Tab::Query,        "⚡  SQL Query"),
                    ];
                    for (tab, label) in tabs {
                        ui.selectable_value(&mut self.tab, tab, label);
                        ui.add_space(2.0);
                    }
                });
            });

        // ── Status bar ────────────────────────────────────────────────────
        egui::TopBottomPanel::bottom("status_bar")
            .frame(
                egui::Frame::side_top_panel(&ctx.style())
                    .inner_margin(egui::Margin::symmetric(10, 4)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let tables_str = if self.tables_state.loading_tables {
                        "loading…".to_string()
                    } else {
                        self.tables_state.tables.len().to_string()
                    };
                    let views_str = if self.views_state.loading_views {
                        "loading…".to_string()
                    } else {
                        self.views_state.views.len().to_string()
                    };
                    ui.colored_label(
                        style::COLOR_MUTED,
                        format!(
                            "Schema: {}  ·  {} tables  ·  {} views",
                            self.selected_schema, tables_str, views_str,
                        ),
                    );
                    // Repaint while loading so status updates
                    if self.tables_state.loading_tables || self.views_state.loading_views {
                        ctx.request_repaint();
                    }
                });
            });

        // ── Content panel ─────────────────────────────────────────────────
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.spacing_mut().item_spacing.y = style::SPACING;

            let pool = match &self.pool {
                Some(p) => p.clone(),
                None => return,
            };
            let schema = self.selected_schema.clone();

            match self.tab {
                Tab::Tables => {
                    self.tables_state.draw(ui, &self.rt, &pool, &schema);
                }
                Tab::CreateScript => {
                    let tables = self.tables_state.tables.clone();
                    self.create_script_state
                        .draw(ui, &tables, &self.rt, &pool, &schema);
                }
                Tab::Views => {
                    self.views_state.draw(ui, &self.rt, &pool, &schema);
                }
                Tab::Query => {
                    self.query_state.draw(ui, &self.rt, &pool);
                }
            }
        });

        // ── Global loading modal (shown when tables + views both loading) ─
        if self.tables_state.loading_tables && self.views_state.loading_views {
            components::loading_modal(ctx, "Loading schema data");
        }
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
