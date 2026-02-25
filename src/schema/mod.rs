//! Schema format converters — turn `TableMeta` into DDL for various databases.
//!
//! Each format maps PostgreSQL types to the target dialect and emits syntactically
//! correct `CREATE TABLE` statements. Adding a new format is a matter of adding
//! a new variant in [`SchemaFormat`], a converter in [`dialects`], and a match arm
//! in [`map_type`].

mod dialects;

use crate::db::{MetaColumn, TableMeta};

// ── Public format enum ───────────────────────────────────────────────────────

/// Supported output formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SchemaFormat {
    Postgres,
    Oracle,
    MySQL,
    SQLServer,
    Databricks,
    SQLite,
    Snowflake,
}

impl SchemaFormat {
    /// All available formats (for UI iteration).
    pub const ALL: &'static [SchemaFormat] = &[
        SchemaFormat::Postgres,
        SchemaFormat::Oracle,
        SchemaFormat::MySQL,
        SchemaFormat::SQLServer,
        SchemaFormat::Databricks,
        SchemaFormat::SQLite,
        SchemaFormat::Snowflake,
    ];

    /// Human-readable label.
    pub fn label(self) -> &'static str {
        match self {
            Self::Postgres => "PostgreSQL",
            Self::Oracle => "Oracle",
            Self::MySQL => "MySQL",
            Self::SQLServer => "SQL Server",
            Self::Databricks => "Databricks (Delta)",
            Self::SQLite => "SQLite",
            Self::Snowflake => "Snowflake",
        }
    }

    /// File extension for saving.
    pub fn file_ext(self) -> &'static str {
        match self {
            Self::Databricks => "sql",
            _ => "sql",
        }
    }

    /// Statement terminator.
    fn terminator(self) -> &'static str {
        match self {
            Self::Oracle => ";",
            Self::Databricks => "",
            _ => ";",
        }
    }

    /// Whether this dialect supports inline CONSTRAINT names.
    fn supports_named_constraints(self) -> bool {
        matches!(
            self,
            Self::Postgres | Self::Oracle | Self::MySQL | Self::SQLServer | Self::Snowflake
        )
    }

    /// Whether this dialect supports CHECK constraints.
    fn supports_check(self) -> bool {
        !matches!(self, Self::Databricks)
    }

    /// Whether this dialect supports foreign keys.
    fn supports_fk(self) -> bool {
        !matches!(self, Self::Databricks)
    }

    /// Whether DEFAULT clauses are supported.
    fn supports_default(self) -> bool {
        !matches!(self, Self::Databricks)
    }

    /// Column quoting style.
    fn quote(self, name: &str) -> String {
        match self {
            Self::MySQL => format!("`{name}`"),
            Self::SQLServer => format!("[{name}]"),
            _ => name.to_string(),
        }
    }

    /// Qualified table reference: `schema.table`.
    fn qualified_table(self, schema: &str, table: &str) -> String {
        match self {
            Self::SQLite | Self::Databricks => self.quote(table),
            _ => format!("{}.{}", self.quote(schema), self.quote(table)),
        }
    }
}

// ── Type mapping ─────────────────────────────────────────────────────────────

/// Map a PostgreSQL column to the target type string.
fn map_type(col: &MetaColumn, fmt: SchemaFormat) -> String {
    let pg = crate::db::pg_type(
        &col.data_type,
        &col.udt_name,
        col.char_max,
        col.num_prec,
        col.num_scale,
    );

    match fmt {
        SchemaFormat::Postgres => pg,
        SchemaFormat::Oracle => dialects::pg_to_oracle(&pg, col),
        SchemaFormat::MySQL => dialects::pg_to_mysql(&pg, col),
        SchemaFormat::SQLServer => dialects::pg_to_sqlserver(&pg, col),
        SchemaFormat::Databricks => dialects::pg_to_databricks(&pg, col),
        SchemaFormat::SQLite => dialects::pg_to_sqlite(&pg, col),
        SchemaFormat::Snowflake => dialects::pg_to_snowflake(&pg, col),
    }
}

// ── DDL emission ─────────────────────────────────────────────────────────────

/// Render a single table's DDL in the given format.
pub fn format_table(meta: &TableMeta, fmt: SchemaFormat) -> String {
    if meta.columns.is_empty() {
        return format!(
            "-- Table '{}.{}' not found or has no columns.",
            meta.schema, meta.name
        );
    }

    let table_ref = fmt.qualified_table(&meta.schema, &meta.name);
    let mut parts: Vec<String> = Vec::new();

    // ── Columns ──────────────────────────────────────────────────────────
    for col in &meta.columns {
        let col_name = fmt.quote(&col.name);
        let type_str = map_type(col, fmt);
        let mut line = format!("    {col_name} {type_str}");
        if fmt.supports_default() {
            if let Some(d) = &col.column_default {
                let default_str = translate_default(d, fmt);
                line.push_str(&format!(" DEFAULT {default_str}"));
            }
        }
        if !col.is_nullable && !meta.pk_columns.contains(&col.name) {
            line.push_str(" NOT NULL");
        }
        parts.push(line);
    }

    // ── Primary key ──────────────────────────────────────────────────────
    if !meta.pk_columns.is_empty() {
        let cols = meta
            .pk_columns
            .iter()
            .map(|c| fmt.quote(c))
            .collect::<Vec<_>>()
            .join(", ");
        parts.push(format!("    PRIMARY KEY ({cols})"));
    }

    // ── Unique constraints ───────────────────────────────────────────────
    for (cname, cols) in &meta.unique_constraints {
        let col_list = cols
            .iter()
            .map(|c| fmt.quote(c))
            .collect::<Vec<_>>()
            .join(", ");
        if fmt.supports_named_constraints() {
            parts.push(format!("    CONSTRAINT {cname} UNIQUE ({col_list})"));
        } else {
            parts.push(format!("    UNIQUE ({col_list})"));
        }
    }

    // ── Foreign keys ─────────────────────────────────────────────────────
    if fmt.supports_fk() {
        for fk in &meta.foreign_keys {
            let src_cols = fk
                .columns
                .iter()
                .map(|c| fmt.quote(c))
                .collect::<Vec<_>>()
                .join(", ");
            let ref_table = if fk.foreign_schema == meta.schema {
                fmt.quote(&fk.foreign_table)
            } else {
                format!(
                    "{}.{}",
                    fmt.quote(&fk.foreign_schema),
                    fmt.quote(&fk.foreign_table)
                )
            };
            let tgt_cols = fk
                .foreign_columns
                .iter()
                .map(|c| fmt.quote(c))
                .collect::<Vec<_>>()
                .join(", ");

            let mut s = if fmt.supports_named_constraints() {
                format!(
                    "    CONSTRAINT {} FOREIGN KEY ({src_cols}) REFERENCES {ref_table} ({tgt_cols})",
                    fk.constraint_name
                )
            } else {
                format!("    FOREIGN KEY ({src_cols}) REFERENCES {ref_table} ({tgt_cols})")
            };
            if fk.update_rule != "NO ACTION" {
                s.push_str(&format!(" ON UPDATE {}", fk.update_rule));
            }
            if fk.delete_rule != "NO ACTION" {
                s.push_str(&format!(" ON DELETE {}", fk.delete_rule));
            }
            parts.push(s);
        }
    }

    // ── Check constraints ────────────────────────────────────────────────
    if fmt.supports_check() {
        for chk in &meta.check_constraints {
            if fmt.supports_named_constraints() {
                parts.push(format!(
                    "    CONSTRAINT {} CHECK {}",
                    chk.constraint_name, chk.check_clause
                ));
            } else {
                parts.push(format!("    CHECK {}", chk.check_clause));
            }
        }
    }

    let term = fmt.terminator();
    format!(
        "CREATE TABLE {table_ref} (\n{}\n){term}",
        parts.join(",\n")
    )
}

/// Render DDL for multiple tables, separated by blank lines.
pub fn format_tables(metas: &[TableMeta], fmt: SchemaFormat) -> String {
    let header = format!(
        "-- Generated for: {}\n-- Format: {}\n-- Tables: {}\n",
        if metas.is_empty() {
            "(none)"
        } else {
            &metas[0].schema
        },
        fmt.label(),
        metas.len(),
    );
    let body: Vec<String> = metas.iter().map(|m| format_table(m, fmt)).collect();
    format!("{header}\n{}", body.join("\n\n"))
}

// ── Default value translation ────────────────────────────────────────────────

/// Attempt a best-effort translation of PG default values to the target dialect.
fn translate_default(pg_default: &str, fmt: SchemaFormat) -> String {
    let d = pg_default.trim();
    match fmt {
        SchemaFormat::Postgres => d.to_string(),
        SchemaFormat::Oracle => {
            if d.starts_with("nextval(") {
                return "NULL".into();
            }
            d.replace("true", "1")
                .replace("false", "0")
                .replace("now()", "SYSDATE")
                .replace("CURRENT_TIMESTAMP", "SYSDATE")
        }
        SchemaFormat::MySQL => {
            if d.starts_with("nextval(") {
                return "NULL".into();
            }
            d.replace("true", "TRUE")
                .replace("false", "FALSE")
                .replace("now()", "CURRENT_TIMESTAMP")
        }
        SchemaFormat::SQLServer => {
            if d.starts_with("nextval(") {
                return "NULL".into();
            }
            d.replace("true", "1")
                .replace("false", "0")
                .replace("now()", "GETDATE()")
                .replace("CURRENT_TIMESTAMP", "GETDATE()")
        }
        SchemaFormat::Databricks => d.to_string(), // defaults not emitted
        SchemaFormat::SQLite => {
            if d.starts_with("nextval(") {
                return "NULL".into();
            }
            d.replace("now()", "CURRENT_TIMESTAMP")
        }
        SchemaFormat::Snowflake => {
            if d.starts_with("nextval(") {
                return "NULL".into();
            }
            d.replace("now()", "CURRENT_TIMESTAMP()")
                .replace("true", "TRUE")
                .replace("false", "FALSE")
        }
    }
}
