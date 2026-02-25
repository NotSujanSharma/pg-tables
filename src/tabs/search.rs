//! Advanced Search tab — find tables, views, and columns by various criteria.

use crate::components::{loading_ui, section_header};
use crate::db;
use crate::style;
use eframe::egui;
use sqlx::PgPool;
use std::sync::{Arc, Mutex};

type Pending<T> = Arc<Mutex<Option<T>>>;

/// Available search modes.
#[derive(PartialEq, Clone, Copy, Debug)]
enum SearchMode {
    ColumnsGlobal,
    TablesWithColumn,
    TablesWithoutColumn,
    ColumnsByType,
    TablesWithoutPK,
    TablesWithoutIndexes,
    ForeignKeyRefs,
    DuplicateIndexes,
    UnusedIndexes,
    NullableNoDefault,
    LargestTables,
    TablesByRowCount,
}

impl SearchMode {
    const ALL: &'static [SearchMode] = &[
        SearchMode::ColumnsGlobal,
        SearchMode::TablesWithColumn,
        SearchMode::TablesWithoutColumn,
        SearchMode::ColumnsByType,
        SearchMode::ForeignKeyRefs,
        SearchMode::TablesWithoutPK,
        SearchMode::TablesWithoutIndexes,
        SearchMode::DuplicateIndexes,
        SearchMode::UnusedIndexes,
        SearchMode::NullableNoDefault,
        SearchMode::LargestTables,
        SearchMode::TablesByRowCount,
    ];

    fn label(self) -> &'static str {
        match self {
            Self::ColumnsGlobal => "🔍  Find Columns Globally",
            Self::TablesWithColumn => "✅  Tables/Views WITH Column",
            Self::TablesWithoutColumn => "❌  Tables/Views WITHOUT Column",
            Self::ColumnsByType => "🏷  Columns by Data Type",
            Self::ForeignKeyRefs => "🔗  Foreign Key References To…",
            Self::TablesWithoutPK => "🚫  Tables Without Primary Key",
            Self::TablesWithoutIndexes => "📑  Tables Without Indexes",
            Self::DuplicateIndexes => "♊  Duplicate Indexes",
            Self::UnusedIndexes => "💤  Unused Indexes",
            Self::NullableNoDefault => "⚠  Nullable Columns (No Default)",
            Self::LargestTables => "📊  Largest Tables (by size)",
            Self::TablesByRowCount => "📈  Tables by Row Count Range",
        }
    }

    fn description(self) -> &'static str {
        match self {
            Self::ColumnsGlobal => "Find all columns matching a name pattern across every table and view.",
            Self::TablesWithColumn => "Find tables or views that contain columns matching a name pattern.",
            Self::TablesWithoutColumn => "Find tables or views that do NOT contain a column matching a pattern.",
            Self::ColumnsByType => "Find columns of a specific data type (e.g. jsonb, uuid, timestamp).",
            Self::ForeignKeyRefs => "Find all foreign keys that reference a specific target table.",
            Self::TablesWithoutPK => "List all tables that have no primary key defined.",
            Self::TablesWithoutIndexes => "List all tables with no indexes at all.",
            Self::DuplicateIndexes => "Find indexes that cover the same columns (redundant).",
            Self::UnusedIndexes => "Find indexes with very few scans (wasting space).",
            Self::NullableNoDefault => "Find nullable columns that have no default — potential data issues.",
            Self::LargestTables => "Show the largest tables ordered by total disk size.",
            Self::TablesByRowCount => "Find tables with approximate row counts in a given range.",
        }
    }

    fn needs_pattern(self) -> bool {
        matches!(
            self,
            Self::ColumnsGlobal
                | Self::TablesWithColumn
                | Self::TablesWithoutColumn
                | Self::ColumnsByType
                | Self::ForeignKeyRefs
        )
    }

    fn needs_relation_kind(self) -> bool {
        matches!(
            self,
            Self::TablesWithColumn | Self::TablesWithoutColumn | Self::ColumnsByType
        )
    }

    fn needs_row_range(self) -> bool {
        matches!(self, Self::TablesByRowCount)
    }

    fn pattern_hint(self) -> &'static str {
        match self {
            Self::ColumnsGlobal => "Column name pattern (e.g. %email%)",
            Self::TablesWithColumn => "Column name pattern (e.g. %created_at%)",
            Self::TablesWithoutColumn => "Column name pattern (e.g. %updated_at%)",
            Self::ColumnsByType => "Data type pattern (e.g. %json%, uuid)",
            Self::ForeignKeyRefs => "Target table name (exact, e.g. users)",
            _ => "",
        }
    }
}

/// Relation kind filter for column-pattern searches.
#[derive(PartialEq, Clone, Copy)]
enum RelationKind {
    Both,
    Tables,
    Views,
}

impl RelationKind {
    fn label(self) -> &'static str {
        match self {
            Self::Both => "Tables & Views",
            Self::Tables => "Tables only",
            Self::Views => "Views only",
        }
    }
    fn value(self) -> &'static str {
        match self {
            Self::Both => "both",
            Self::Tables => "tables",
            Self::Views => "views",
        }
    }
}

/// State for the Advanced Search tab.
pub struct SearchState {
    mode: SearchMode,
    pattern: String,
    relation_kind: RelationKind,
    min_rows: String,
    max_rows: String,

    // results
    result: Option<db::SearchResult>,
    error: Option<String>,
    loading: bool,
    pending: Option<Pending<Result<db::SearchResult, String>>>,

    // result filter
    result_search: String,
}

impl Default for SearchState {
    fn default() -> Self {
        Self {
            mode: SearchMode::ColumnsGlobal,
            pattern: String::new(),
            relation_kind: RelationKind::Both,
            min_rows: "0".into(),
            max_rows: "1000000000".into(),
            result: None,
            error: None,
            loading: false,
            pending: None,
            result_search: String::new(),
        }
    }
}

impl SearchState {
    pub fn clear(&mut self) {
        self.result = None;
        self.error = None;
        self.loading = false;
        self.pending = None;
        self.result_search.clear();
    }

    fn run_search(
        &mut self,
        rt: &tokio::runtime::Runtime,
        pool: &PgPool,
        schema: &str,
    ) {
        let result: Pending<Result<db::SearchResult, String>> = Arc::new(Mutex::new(None));
        self.pending = Some(result.clone());
        self.loading = true;
        self.error = None;

        let pool = pool.clone();
        let schema = schema.to_string();
        let mode = self.mode;
        let pattern = self.pattern.clone();
        let rk = self.relation_kind.value().to_string();
        let min_rows: i64 = self.min_rows.parse().unwrap_or(0);
        let max_rows: i64 = self.max_rows.parse().unwrap_or(i64::MAX);

        rt.spawn(async move {
            let r = match mode {
                SearchMode::ColumnsGlobal => {
                    db::search_columns_globally(&pool, &schema, &pattern)
                        .await
                        .map_err(|e| e.to_string())
                }
                SearchMode::TablesWithColumn => {
                    db::search_relations_with_column(&pool, &schema, &pattern, &rk)
                        .await
                        .map_err(|e| e.to_string())
                }
                SearchMode::TablesWithoutColumn => {
                    db::search_relations_without_column(&pool, &schema, &pattern, &rk)
                        .await
                        .map_err(|e| e.to_string())
                }
                SearchMode::ColumnsByType => {
                    db::search_by_column_type(&pool, &schema, &pattern, &rk)
                        .await
                        .map_err(|e| e.to_string())
                }
                SearchMode::TablesWithoutPK => {
                    db::search_tables_without_pk(&pool, &schema)
                        .await
                        .map_err(|e| e.to_string())
                }
                SearchMode::TablesWithoutIndexes => {
                    db::search_tables_without_indexes(&pool, &schema)
                        .await
                        .map_err(|e| e.to_string())
                }
                SearchMode::ForeignKeyRefs => {
                    db::search_fk_references(&pool, &schema, &pattern)
                        .await
                        .map_err(|e| e.to_string())
                }
                SearchMode::DuplicateIndexes => {
                    db::search_duplicate_indexes(&pool, &schema)
                        .await
                        .map_err(|e| e.to_string())
                }
                SearchMode::UnusedIndexes => {
                    db::search_unused_indexes(&pool, &schema)
                        .await
                        .map_err(|e| e.to_string())
                }
                SearchMode::NullableNoDefault => {
                    db::search_nullable_no_default(&pool, &schema)
                        .await
                        .map_err(|e| e.to_string())
                }
                SearchMode::LargestTables => {
                    db::search_largest_tables(&pool, &schema)
                        .await
                        .map_err(|e| e.to_string())
                }
                SearchMode::TablesByRowCount => {
                    db::search_tables_by_row_count(&pool, &schema, min_rows, max_rows)
                        .await
                        .map_err(|e| e.to_string())
                }
            };
            *result.lock().unwrap() = Some(r);
        });
    }

    fn poll(&mut self, ctx: &egui::Context) {
        if !self.loading {
            return;
        }
        ctx.request_repaint();
        let done = self
            .pending
            .as_ref()
            .and_then(|p| p.try_lock().ok())
            .and_then(|mut g| g.take());
        if let Some(r) = done {
            match r {
                Ok(sr) => {
                    self.result = Some(sr);
                    self.error = None;
                }
                Err(e) => {
                    self.error = Some(e);
                    self.result = None;
                }
            }
            self.loading = false;
            self.pending = None;
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

        // ── Search criteria panel ─────────────────────────────────────────
        section_header(ui, "Advanced Search", 0, "");
        ui.add_space(2.0);

        // Mode selector
        ui.horizontal(|ui| {
            ui.colored_label(style::COLOR_MUTED, "Search mode:");
            egui::ComboBox::from_id_salt("search_mode")
                .selected_text(self.mode.label())
                .width(320.0)
                .show_ui(ui, |ui| {
                    for &m in SearchMode::ALL {
                        ui.selectable_value(&mut self.mode, m, m.label());
                    }
                });
        });

        // Description
        ui.add_space(2.0);
        ui.colored_label(style::COLOR_MUTED, self.mode.description());
        ui.add_space(4.0);

        // Inputs row
        let mut trigger_search = false;
        ui.horizontal(|ui| {
            if self.mode.needs_pattern() {
                ui.add(
                    egui::TextEdit::singleline(&mut self.pattern)
                        .hint_text(self.mode.pattern_hint())
                        .desired_width(300.0),
                );
            }

            if self.mode.needs_relation_kind() {
                egui::ComboBox::from_id_salt("relation_kind_filter")
                    .selected_text(self.relation_kind.label())
                    .width(140.0)
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut self.relation_kind,
                            RelationKind::Both,
                            RelationKind::Both.label(),
                        );
                        ui.selectable_value(
                            &mut self.relation_kind,
                            RelationKind::Tables,
                            RelationKind::Tables.label(),
                        );
                        ui.selectable_value(
                            &mut self.relation_kind,
                            RelationKind::Views,
                            RelationKind::Views.label(),
                        );
                    });
            }

            if self.mode.needs_row_range() {
                ui.colored_label(style::COLOR_MUTED, "Min:");
                ui.add(
                    egui::TextEdit::singleline(&mut self.min_rows)
                        .desired_width(90.0),
                );
                ui.colored_label(style::COLOR_MUTED, "Max:");
                ui.add(
                    egui::TextEdit::singleline(&mut self.max_rows)
                        .desired_width(90.0),
                );
            }

            let can_search = if self.mode.needs_pattern() {
                !self.pattern.trim().is_empty()
            } else {
                true
            };

            let btn_text = if self.loading {
                "⏳  Searching…"
            } else {
                "🔍  Search"
            };
            if ui
                .add_enabled(
                    can_search && !self.loading,
                    egui::Button::new(egui::RichText::new(btn_text).size(13.0))
                        .min_size(egui::vec2(100.0, 28.0)),
                )
                .clicked()
            {
                trigger_search = true;
            }
        });

        if trigger_search {
            self.result_search.clear();
            self.run_search(rt, pool, schema);
        }

        ui.add_space(4.0);
        ui.separator();
        ui.add_space(2.0);

        // ── Results panel ─────────────────────────────────────────────────
        if self.loading {
            loading_ui(ui, "Searching");
            return;
        }

        if let Some(err) = &self.error {
            egui::Frame::NONE
                .inner_margin(egui::Margin::symmetric(12, 8))
                .corner_radius(6.0)
                .fill(egui::Color32::from_rgb(60, 24, 24))
                .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(140, 50, 50)))
                .show(ui, |ui| {
                    ui.colored_label(style::COLOR_ERROR, format!("⚠  {err}"));
                });
            return;
        }

        if let Some(sr) = self.result.clone() {
            let row_count = sr.rows.len();
            let copy_text = self.results_to_text(&sr);

            // Result header with count and filter
            ui.horizontal(|ui| {
                ui.add(egui::Label::new(
                    egui::RichText::new("Results").strong().size(13.5),
                ));
                ui.colored_label(
                    style::COLOR_ACCENT,
                    format!("{row_count} rows"),
                );

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    // Copy results button
                    if !sr.rows.is_empty() {
                        if ui
                            .add(egui::Button::new("📋 Copy").min_size(egui::vec2(70.0, 24.0)))
                            .clicked()
                        {
                            ui.ctx().copy_text(copy_text.clone());
                        }
                    }

                    // Inline filter for results
                    if row_count > 5 {
                        let resp = ui.add(
                            egui::TextEdit::singleline(&mut self.result_search)
                                .hint_text("Filter results…")
                                .desired_width(180.0),
                        );
                        if !self.result_search.is_empty() {
                            if ui
                                .add(
                                    egui::Button::new(
                                        egui::RichText::new("✕").color(style::COLOR_MUTED),
                                    )
                                    .frame(false),
                                )
                                .clicked()
                            {
                                self.result_search.clear();
                                resp.request_focus();
                            }
                        }
                        ui.colored_label(style::COLOR_MUTED, "🔍");
                    }
                });
            });
            ui.add_space(2.0);

            if sr.rows.is_empty() {
                ui.centered_and_justified(|ui| {
                    ui.colored_label(style::COLOR_MUTED, "No results found.");
                });
            } else {
                let filter_lower = self.result_search.to_lowercase();
                Self::draw_result_table(ui, &sr, &filter_lower);
            }
        } else {
            // No search run yet
            ui.add_space(30.0);
            ui.vertical_centered(|ui| {
                ui.colored_label(
                    style::COLOR_MUTED,
                    "Choose a search mode and click Search to begin.",
                );
            });
        }
    }

    fn draw_result_table(
        ui: &mut egui::Ui,
        sr: &db::SearchResult,
        filter: &str,
    ) {
        egui::ScrollArea::both()
            .id_salt("search_results_scroll")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                egui::Grid::new("search_results_grid")
                    .num_columns(sr.columns.len())
                    .spacing([16.0, 4.0])
                    .striped(true)
                    .show(ui, |ui| {
                        // Header row
                        for col in &sr.columns {
                            ui.colored_label(style::COLOR_HEADER, col);
                        }
                        ui.end_row();

                        // Data rows
                        let mut shown = 0;
                        for row in &sr.rows {
                            // Apply inline filter
                            if !filter.is_empty() {
                                let matches = row
                                    .cells
                                    .iter()
                                    .any(|c| c.to_lowercase().contains(filter));
                                if !matches {
                                    continue;
                                }
                            }
                            for (i, cell) in row.cells.iter().enumerate() {
                                // Color the first column as accent
                                if i == 0 {
                                    ui.monospace(cell);
                                } else {
                                    ui.label(cell);
                                }
                            }
                            ui.end_row();
                            shown += 1;
                        }

                        if shown == 0 && !filter.is_empty() {
                            for _ in &sr.columns {
                                ui.label("");
                            }
                            ui.end_row();
                        }
                    });

                if !filter.is_empty() {
                    let visible = sr
                        .rows
                        .iter()
                        .filter(|r| r.cells.iter().any(|c| c.to_lowercase().contains(filter)))
                        .count();
                    if visible == 0 {
                        ui.add_space(8.0);
                        ui.centered_and_justified(|ui| {
                            ui.colored_label(style::COLOR_MUTED, "No results match filter.");
                        });
                    }
                }
            });
    }

    fn results_to_text(&self, sr: &db::SearchResult) -> String {
        let mut out = String::new();
        // Header
        out.push_str(&sr.columns.join("\t"));
        out.push('\n');
        // Rows
        for row in &sr.rows {
            out.push_str(&row.cells.join("\t"));
            out.push('\n');
        }
        out
    }
}
