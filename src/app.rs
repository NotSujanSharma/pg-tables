//! Main application state and routing — delegates to tab modules.

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
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
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
                ui.add_space(80.0);

                ui.heading(egui::RichText::new("🐘 PG Tables").size(28.0));
                ui.add_space(4.0);
                ui.colored_label(style::COLOR_MUTED, "Connect to your PostgreSQL database");
                ui.add_space(30.0);

                ui.allocate_ui(egui::vec2(style::LOGIN_PANEL_WIDTH, 0.0), |ui| {
                    egui::Frame::NONE
                        .inner_margin(20.0)
                        .corner_radius(8.0)
                        .stroke(egui::Stroke::new(1.0, egui::Color32::from_gray(60)))
                        .show(ui, |ui| {
                            egui::Grid::new("login_grid")
                                .num_columns(2)
                                .spacing([12.0, 10.0])
                                .show(ui, |ui| {
                                    ui.label("Host:");
                                    ui.add(egui::TextEdit::singleline(&mut self.host)
                                        .desired_width(style::LOGIN_FIELD_WIDTH));
                                    ui.end_row();

                                    ui.label("Port:");
                                    ui.add(egui::TextEdit::singleline(&mut self.port)
                                        .desired_width(style::LOGIN_FIELD_WIDTH));
                                    ui.end_row();

                                    ui.label("User:");
                                    ui.add(egui::TextEdit::singleline(&mut self.user)
                                        .desired_width(style::LOGIN_FIELD_WIDTH));
                                    ui.end_row();

                                    ui.label("Password:");
                                    ui.add(egui::TextEdit::singleline(&mut self.password)
                                        .password(true)
                                        .desired_width(style::LOGIN_FIELD_WIDTH));
                                    ui.end_row();

                                    ui.label("Database:");
                                    ui.add(egui::TextEdit::singleline(&mut self.dbname)
                                        .desired_width(style::LOGIN_FIELD_WIDTH));
                                    ui.end_row();
                                });

                            ui.add_space(10.0);
                            ui.checkbox(&mut self.remember_session, "Remember credentials");
                            ui.add_space(12.0);

                            ui.vertical_centered(|ui| {
                                let btn = ui.add_enabled(
                                    !self.connecting,
                                    egui::Button::new(
                                        egui::RichText::new(if self.connecting {
                                            "⏳ Connecting..."
                                        } else {
                                            "🔌 Connect"
                                        })
                                        .size(15.0),
                                    )
                                    .min_size(egui::vec2(140.0, 34.0)),
                                );
                                if btn.clicked() {
                                    self.do_connect();
                                }
                            });
                        });
                });

                if let Some(err) = &self.conn_error {
                    ui.add_space(14.0);
                    ui.colored_label(style::COLOR_ERROR, format!("⚠ {err}"));
                }
            });
        });
    }

    fn draw_main(&mut self, ctx: &egui::Context) {
        // Top bar
        egui::TopBottomPanel::top("top_bar")
            .frame(egui::Frame::side_top_panel(&ctx.style()).inner_margin(8.0))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("🐘 PG Tables").size(18.0).strong());
                    ui.separator();

                    ui.label("Schema:");
                    let prev_schema = self.selected_schema.clone();
                    egui::ComboBox::from_id_salt("schema_selector")
                        .selected_text(&self.selected_schema)
                        .width(140.0)
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
                        ui.separator();
                        ui.colored_label(
                            style::COLOR_MUTED,
                            format!("{}@{}:{}/{}", self.user, self.host, self.port, self.dbname),
                        );
                    });
                });
            });

        // Tab bar
        egui::TopBottomPanel::top("tab_bar")
            .frame(egui::Frame::side_top_panel(&ctx.style()).inner_margin(egui::Margin::symmetric(8, 4)))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut self.tab, Tab::Tables, "📋 Tables");
                    ui.add_space(4.0);
                    ui.selectable_value(&mut self.tab, Tab::CreateScript, "📝 Create Script");
                    ui.add_space(4.0);
                    ui.selectable_value(&mut self.tab, Tab::Views, "👁 Views");
                    ui.add_space(4.0);
                    ui.selectable_value(&mut self.tab, Tab::Query, "⚡ SQL Query");
                });
            });

        // Status bar
        egui::TopBottomPanel::bottom("status_bar")
            .frame(egui::Frame::side_top_panel(&ctx.style()).inner_margin(4.0))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.colored_label(
                        style::COLOR_MUTED,
                        format!(
                            "Schema: {} │ {} tables │ {} views",
                            self.selected_schema,
                            self.tables_state.tables.len(),
                            self.views_state.views.len(),
                        ),
                    );
                });
            });

        // Content — delegate to tab modules
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.spacing_mut().item_spacing.y = style::SPACING;

            // We need pool reference for tabs
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
                    self.create_script_state.draw(ui, &tables, &self.rt, &pool, &schema);
                }
                Tab::Views => {
                    self.views_state.draw(ui, &self.rt, &pool, &schema);
                }
                Tab::Query => {
                    self.query_state.draw(ui, &self.rt, &pool);
                }
            }
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
