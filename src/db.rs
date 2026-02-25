use sqlx::postgres::PgPoolOptions;
use sqlx::{Column, PgPool, Row, TypeInfo};
use std::collections::BTreeMap;

/// Connect to PostgreSQL and return a connection pool.
pub async fn connect(
    host: &str,
    port: &str,
    user: &str,
    password: &str,
    dbname: &str,
) -> Result<PgPool, sqlx::Error> {
    let url = format!(
        "postgres://{}:{}@{}:{}/{}",
        user, password, host, port, dbname
    );
    PgPoolOptions::new()
        .max_connections(5)
        .connect(&url)
        .await
}

/// Fetch the list of available schemas.
pub async fn list_schemas(pool: &PgPool) -> Result<Vec<String>, sqlx::Error> {
    let rows: Vec<String> = sqlx::query_scalar(
        r#"
        SELECT schema_name
        FROM information_schema.schemata
        WHERE schema_name NOT IN ('pg_catalog', 'information_schema', 'pg_toast')
        ORDER BY schema_name
        "#,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// Fetch the list of tables in a schema.
pub async fn list_tables(pool: &PgPool, schema: &str) -> Result<Vec<String>, sqlx::Error> {
    let tables: Vec<String> = sqlx::query(
        r#"
        SELECT table_name
        FROM information_schema.tables
        WHERE table_schema = $1
          AND table_type = 'BASE TABLE'
        ORDER BY table_name
        "#,
    )
    .bind(schema)
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|row| row.get::<String, _>("table_name"))
    .collect();
    Ok(tables)
}

/// Fetch the list of views (regular + materialized) in a schema.
pub async fn list_views(pool: &PgPool, schema: &str) -> Result<Vec<(String, String)>, sqlx::Error> {
    let views: Vec<(String, String)> = sqlx::query(
        r#"
        SELECT relname AS view_name,
               CASE relkind WHEN 'v' THEN 'VIEW' ELSE 'MATERIALIZED VIEW' END AS kind
        FROM pg_class c
        JOIN pg_namespace n ON n.oid = c.relnamespace
        WHERE n.nspname = $1 AND c.relkind IN ('v', 'm')
        ORDER BY relkind, relname
        "#,
    )
    .bind(schema)
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|row| (row.get("view_name"), row.get("kind")))
    .collect();
    Ok(views)
}

/// Column info for a table.
#[derive(Clone)]
pub struct ColumnInfo {
    pub name: String,
    pub data_type: String,
    pub is_nullable: String,
    pub column_default: Option<String>,
}

/// Fetch column details for a table.
pub async fn list_columns(
    pool: &PgPool,
    schema: &str,
    table: &str,
) -> Result<Vec<ColumnInfo>, sqlx::Error> {
    let cols = sqlx::query(
        r#"
        SELECT column_name, data_type, udt_name,
               character_maximum_length,
               numeric_precision, numeric_scale,
               is_nullable, column_default
        FROM information_schema.columns
        WHERE table_schema = $1 AND table_name = $2
        ORDER BY ordinal_position
        "#,
    )
    .bind(schema)
    .bind(table)
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|row| {
        let data_type: String = row.get("data_type");
        let udt_name: String = row.get("udt_name");
        let char_max: Option<i32> = row.get("character_maximum_length");
        let num_prec: Option<i32> = row.get("numeric_precision");
        let num_scale: Option<i32> = row.get("numeric_scale");
        ColumnInfo {
            name: row.get("column_name"),
            data_type: pg_type(&data_type, &udt_name, char_max, num_prec, num_scale),
            is_nullable: row.get("is_nullable"),
            column_default: row.get("column_default"),
        }
    })
    .collect();
    Ok(cols)
}

/// Dependency info for a view.
#[derive(Clone)]
pub struct Dep {
    pub schema: String,
    pub name: String,
    pub kind: String,
}

/// Get the dependencies and definition of a view.
pub async fn view_dependencies(
    pool: &PgPool,
    schema: &str,
    view: &str,
) -> Result<(Vec<Dep>, Option<String>), sqlx::Error> {
    let rows: Vec<Dep> = sqlx::query(
        r#"
        SELECT DISTINCT
            dep_ns.nspname   AS dep_schema,
            dep_obj.relname  AS dep_name,
            CASE dep_obj.relkind
                WHEN 'r' THEN 'TABLE'
                WHEN 'v' THEN 'VIEW'
                WHEN 'm' THEN 'MATERIALIZED VIEW'
                ELSE dep_obj.relkind::text
            END AS dep_kind
        FROM pg_rewrite rw
        JOIN pg_depend d       ON d.objid     = rw.oid
        JOIN pg_class dep_obj  ON dep_obj.oid = d.refobjid
        JOIN pg_namespace dep_ns ON dep_ns.oid = dep_obj.relnamespace
        JOIN pg_class view_cls ON view_cls.oid = rw.ev_class
        JOIN pg_namespace view_ns ON view_ns.oid = view_cls.relnamespace
        WHERE view_cls.relname = $1
          AND view_ns.nspname  = $2
          AND dep_obj.relname != $1
          AND dep_obj.relkind IN ('r', 'v', 'm')
        ORDER BY dep_kind, dep_schema, dep_name
        "#,
    )
    .bind(view)
    .bind(schema)
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|row| Dep {
        schema: row.get("dep_schema"),
        name: row.get("dep_name"),
        kind: row.get("dep_kind"),
    })
    .collect();

    let def: Option<String> = sqlx::query_scalar(
        r#"
        SELECT pg_get_viewdef(c.oid, true)
        FROM pg_class c
        JOIN pg_namespace n ON n.oid = c.relnamespace
        WHERE n.nspname = $1 AND c.relname = $2
          AND c.relkind IN ('v', 'm')
        "#,
    )
    .bind(schema)
    .bind(view)
    .fetch_optional(pool)
    .await?
    .flatten();

    Ok((rows, def))
}

// ── Table metadata (used by schema generator) ────────────────────────────────

/// Raw column metadata extracted from `information_schema`.
#[derive(Clone, Debug)]
pub struct MetaColumn {
    pub name: String,
    pub data_type: String,     // raw info_schema data_type
    pub udt_name: String,
    pub char_max: Option<i32>,
    pub num_prec: Option<i32>,
    pub num_scale: Option<i32>,
    pub is_nullable: bool,
    pub column_default: Option<String>,
}

/// Foreign key constraint metadata.
#[derive(Clone, Debug)]
pub struct MetaFk {
    pub constraint_name: String,
    pub columns: Vec<String>,
    pub foreign_schema: String,
    pub foreign_table: String,
    pub foreign_columns: Vec<String>,
    pub update_rule: String,
    pub delete_rule: String,
}

/// Check constraint metadata.
#[derive(Clone, Debug)]
pub struct MetaCheck {
    pub constraint_name: String,
    pub check_clause: String,
}

/// Complete table metadata — everything needed to emit DDL in any format.
#[derive(Clone, Debug)]
pub struct TableMeta {
    pub schema: String,
    pub name: String,
    pub columns: Vec<MetaColumn>,
    pub pk_columns: Vec<String>,
    pub unique_constraints: BTreeMap<String, Vec<String>>,
    pub foreign_keys: Vec<MetaFk>,
    pub check_constraints: Vec<MetaCheck>,
}

/// Fetch full table metadata from PostgreSQL catalog.
pub async fn fetch_table_meta(
    pool: &PgPool,
    schema: &str,
    table: &str,
) -> Result<TableMeta, sqlx::Error> {
    // columns
    let columns: Vec<MetaColumn> = sqlx::query(
        r#"
        SELECT column_name, data_type, udt_name,
               character_maximum_length,
               numeric_precision, numeric_scale,
               is_nullable, column_default
        FROM information_schema.columns
        WHERE table_schema = $1 AND table_name = $2
        ORDER BY ordinal_position
        "#,
    )
    .bind(schema)
    .bind(table)
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|row| {
        let nullable: String = row.get("is_nullable");
        MetaColumn {
            name: row.get("column_name"),
            data_type: row.get("data_type"),
            udt_name: row.get("udt_name"),
            char_max: row.get("character_maximum_length"),
            num_prec: row.get("numeric_precision"),
            num_scale: row.get("numeric_scale"),
            is_nullable: nullable == "YES",
            column_default: row.get("column_default"),
        }
    })
    .collect();

    // primary key
    let pk_columns: Vec<String> = sqlx::query(
        r#"
        SELECT kcu.column_name
        FROM information_schema.table_constraints tc
        JOIN information_schema.key_column_usage kcu
          ON tc.constraint_name = kcu.constraint_name
         AND tc.table_schema    = kcu.table_schema
        WHERE tc.constraint_type = 'PRIMARY KEY'
          AND tc.table_schema = $1 AND tc.table_name = $2
        ORDER BY kcu.ordinal_position
        "#,
    )
    .bind(schema)
    .bind(table)
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|r| r.get::<String, _>("column_name"))
    .collect();

    // unique constraints
    let mut unique_constraints: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for row in sqlx::query(
        r#"
        SELECT tc.constraint_name, kcu.column_name
        FROM information_schema.table_constraints tc
        JOIN information_schema.key_column_usage kcu
          ON tc.constraint_name = kcu.constraint_name
         AND tc.table_schema    = kcu.table_schema
        WHERE tc.constraint_type = 'UNIQUE'
          AND tc.table_schema = $1 AND tc.table_name = $2
        ORDER BY tc.constraint_name, kcu.ordinal_position
        "#,
    )
    .bind(schema)
    .bind(table)
    .fetch_all(pool)
    .await?
    {
        unique_constraints
            .entry(row.get("constraint_name"))
            .or_default()
            .push(row.get("column_name"));
    }

    // foreign keys
    let mut fk_map: BTreeMap<String, MetaFk> = BTreeMap::new();
    for row in sqlx::query(
        r#"
        SELECT tc.constraint_name,
               kcu.column_name,
               ccu.table_schema  AS foreign_schema,
               ccu.table_name    AS foreign_table,
               ccu.column_name   AS foreign_column,
               rc.update_rule,
               rc.delete_rule
        FROM information_schema.table_constraints tc
        JOIN information_schema.key_column_usage kcu
          ON tc.constraint_name = kcu.constraint_name
         AND tc.table_schema    = kcu.table_schema
        JOIN information_schema.constraint_column_usage ccu
          ON ccu.constraint_name  = tc.constraint_name
         AND ccu.constraint_schema = tc.table_schema
        JOIN information_schema.referential_constraints rc
          ON rc.constraint_name  = tc.constraint_name
         AND rc.constraint_schema = tc.table_schema
        WHERE tc.constraint_type = 'FOREIGN KEY'
          AND tc.table_schema = $1 AND tc.table_name = $2
        ORDER BY tc.constraint_name, kcu.ordinal_position
        "#,
    )
    .bind(schema)
    .bind(table)
    .fetch_all(pool)
    .await?
    {
        let cname: String = row.get("constraint_name");
        let entry = fk_map.entry(cname.clone()).or_insert_with(|| MetaFk {
            constraint_name: cname,
            columns: vec![],
            foreign_schema: row.get("foreign_schema"),
            foreign_table: row.get("foreign_table"),
            foreign_columns: vec![],
            update_rule: row.get("update_rule"),
            delete_rule: row.get("delete_rule"),
        });
        entry.columns.push(row.get("column_name"));
        entry.foreign_columns.push(row.get("foreign_column"));
    }
    let foreign_keys: Vec<MetaFk> = fk_map.into_values().collect();

    // check constraints
    let check_constraints: Vec<MetaCheck> = sqlx::query(
        r#"
        SELECT tc.constraint_name, cc.check_clause
        FROM information_schema.table_constraints tc
        JOIN information_schema.check_constraints cc
          ON cc.constraint_name  = tc.constraint_name
         AND cc.constraint_schema = tc.table_schema
        WHERE tc.constraint_type = 'CHECK'
          AND tc.table_schema = $1 AND tc.table_name = $2
          AND tc.constraint_name NOT LIKE '%_not_null'
        ORDER BY tc.constraint_name
        "#,
    )
    .bind(schema)
    .bind(table)
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|row| MetaCheck {
        constraint_name: row.get("constraint_name"),
        check_clause: row.get("check_clause"),
    })
    .collect();

    Ok(TableMeta {
        schema: schema.to_string(),
        name: table.to_string(),
        columns,
        pk_columns,
        unique_constraints,
        foreign_keys,
        check_constraints,
    })
}

/// Generate the CREATE TABLE script as a string (PostgreSQL native format).
/// Kept for backward compatibility; delegates to `fetch_table_meta` + format.
#[allow(dead_code)]
pub async fn generate_create_script(
    pool: &PgPool,
    schema: &str,
    table: &str,
) -> Result<String, sqlx::Error> {
    let meta = fetch_table_meta(pool, schema, table).await?;
    if meta.columns.is_empty() {
        return Ok(format!(
            "-- Table '{schema}.{table}' not found or has no columns."
        ));
    }
    Ok(crate::schema_format::format_table(&meta, crate::schema_format::SchemaFormat::Postgres))
}

pub fn pg_type(
    data_type: &str,
    udt_name: &str,
    char_max: Option<i32>,
    num_prec: Option<i32>,
    num_scale: Option<i32>,
) -> String {
    match data_type {
        "character varying" => char_max
            .map(|n| format!("VARCHAR({n})"))
            .unwrap_or_else(|| "VARCHAR".into()),
        "character" => char_max
            .map(|n| format!("CHAR({n})"))
            .unwrap_or_else(|| "CHAR".into()),
        "numeric" | "decimal" => match (num_prec, num_scale) {
            (Some(p), Some(s)) if s > 0 => format!("NUMERIC({p}, {s})"),
            (Some(p), _) => format!("NUMERIC({p})"),
            _ => "NUMERIC".into(),
        },
        "integer" => "INTEGER".into(),
        "bigint" => "BIGINT".into(),
        "smallint" => "SMALLINT".into(),
        "real" => "REAL".into(),
        "double precision" => "DOUBLE PRECISION".into(),
        "boolean" => "BOOLEAN".into(),
        "text" => "TEXT".into(),
        "bytea" => "BYTEA".into(),
        "date" => "DATE".into(),
        "time without time zone" => "TIME".into(),
        "time with time zone" => "TIMETZ".into(),
        "timestamp without time zone" => "TIMESTAMP".into(),
        "timestamp with time zone" => "TIMESTAMPTZ".into(),
        "interval" => "INTERVAL".into(),
        "uuid" => "UUID".into(),
        "json" => "JSON".into(),
        "jsonb" => "JSONB".into(),
        "ARRAY" => format!("{}[]", udt_name.trim_start_matches('_')),
        "USER-DEFINED" => udt_name.into(),
        _ => data_type.to_uppercase(),
    }
}

/// Full column metadata used by the fake data generator.
#[derive(Clone, Debug)]
#[allow(dead_code)]
pub struct FakeColumnInfo {
    pub name: String,
    /// Human-friendly type string, e.g. `VARCHAR(50)`.
    pub data_type: String,
    /// Raw value from `information_schema.data_type`.
    pub raw_type: String,
    pub udt_name: String,
    pub char_max: Option<i32>,
    pub numeric_precision: Option<i32>,
    pub numeric_scale: Option<i32>,
    pub is_nullable: bool,
    pub column_default: Option<String>,
}

/// Fetch full column metadata for a table (used for fake data generation).
pub async fn fetch_fake_columns(
    pool: &PgPool,
    schema: &str,
    table: &str,
) -> Result<Vec<FakeColumnInfo>, sqlx::Error> {
    let rows = sqlx::query(
        r#"
        SELECT column_name, data_type, udt_name,
               character_maximum_length,
               numeric_precision, numeric_scale,
               is_nullable, column_default
        FROM information_schema.columns
        WHERE table_schema = $1 AND table_name = $2
        ORDER BY ordinal_position
        "#,
    )
    .bind(schema)
    .bind(table)
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|row| {
        let raw_type: String = row.get("data_type");
        let udt_name: String = row.get("udt_name");
        let char_max: Option<i32> = row.get("character_maximum_length");
        let num_prec: Option<i32> = row.get("numeric_precision");
        let num_scale: Option<i32> = row.get("numeric_scale");
        let is_nullable: String = row.get("is_nullable");
        FakeColumnInfo {
            data_type: pg_type(&raw_type, &udt_name, char_max, num_prec, num_scale),
            name: row.get("column_name"),
            raw_type,
            udt_name,
            char_max,
            numeric_precision: num_prec,
            numeric_scale: num_scale,
            is_nullable: is_nullable == "YES",
            column_default: row.get("column_default"),
        }
    })
    .collect();
    Ok(rows)
}

/// Result of an arbitrary SQL query.
#[derive(Clone, Default)]
pub struct QueryResult {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<String>>,
    pub affected: u64,
    pub is_select: bool,
}

/// Execute an arbitrary SQL statement and return results.
pub async fn execute_query(
    pool: &PgPool,
    sql: &str,
) -> Result<QueryResult, String> {
    let trimmed = sql.trim();
    let is_select = trimmed
        .split_whitespace()
        .next()
        .map(|w| w.eq_ignore_ascii_case("SELECT") || w.eq_ignore_ascii_case("WITH"))
        .unwrap_or(false);

    if is_select {
        let rows = sqlx::query(trimmed)
            .fetch_all(pool)
            .await
            .map_err(|e| e.to_string())?;

        if rows.is_empty() {
            return Ok(QueryResult {
                columns: vec![],
                rows: vec![],
                affected: 0,
                is_select: true,
            });
        }

        let columns: Vec<String> = rows[0]
            .columns()
            .iter()
            .map(|c| c.name().to_string())
            .collect();

        let result_rows: Vec<Vec<String>> = rows
            .iter()
            .map(|row| {
                row.columns()
                    .iter()
                    .enumerate()
                    .map(|(i, col)| {
                        let type_name = col.type_info().name();
                        match type_name {
                            "INT2" | "INT4" | "INT8" => row
                                .try_get::<i64, _>(i)
                                .map(|v| v.to_string())
                                .unwrap_or_else(|_| "NULL".into()),
                            "FLOAT4" | "FLOAT8" | "NUMERIC" => row
                                .try_get::<f64, _>(i)
                                .map(|v| v.to_string())
                                .unwrap_or_else(|_| "NULL".into()),
                            "BOOL" => row
                                .try_get::<bool, _>(i)
                                .map(|v| v.to_string())
                                .unwrap_or_else(|_| "NULL".into()),
                            _ => row
                                .try_get::<String, _>(i)
                                .unwrap_or_else(|_| "NULL".into()),
                        }
                    })
                    .collect()
            })
            .collect();

        Ok(QueryResult {
            columns,
            rows: result_rows,
            affected: 0,
            is_select: true,
        })
    } else {
        let result = sqlx::query(trimmed)
            .execute(pool)
            .await
            .map_err(|e| e.to_string())?;

        Ok(QueryResult {
            columns: vec![],
            rows: vec![],
            affected: result.rows_affected(),
            is_select: false,
        })
    }
}

// ── Advanced Search queries ──────────────────────────────────────────────────

/// Result row for advanced search.
#[derive(Clone, Debug)]
pub struct SearchResultRow {
    pub cells: Vec<String>,
}

/// A complete search result with column headers and rows.
#[derive(Clone, Debug, Default)]
pub struct SearchResult {
    pub columns: Vec<String>,
    pub rows: Vec<SearchResultRow>,
}

/// Find tables/views that HAVE columns matching a name pattern (SQL LIKE).
pub async fn search_relations_with_column(
    pool: &PgPool,
    schema: &str,
    column_pattern: &str,
    relation_kind: &str, // "tables", "views", "both"
) -> Result<SearchResult, sqlx::Error> {
    let kind_filter = match relation_kind {
        "tables" => "AND t.table_type = 'BASE TABLE'",
        "views" => "AND t.table_type IN ('VIEW')",
        _ => "",
    };
    let sql = format!(
        r#"
        SELECT DISTINCT t.table_name, t.table_type, c.column_name, c.data_type
        FROM information_schema.tables t
        JOIN information_schema.columns c
          ON c.table_schema = t.table_schema AND c.table_name = t.table_name
        WHERE t.table_schema = $1
          AND c.column_name LIKE $2
          {kind_filter}
        ORDER BY t.table_type, t.table_name, c.column_name
        "#,
    );
    let rows = sqlx::query(&sql)
        .bind(schema)
        .bind(column_pattern)
        .fetch_all(pool)
        .await?;
    Ok(SearchResult {
        columns: vec!["Table/View".into(), "Type".into(), "Column".into(), "Data Type".into()],
        rows: rows
            .iter()
            .map(|r| SearchResultRow {
                cells: vec![
                    r.get("table_name"),
                    r.get("table_type"),
                    r.get("column_name"),
                    r.get("data_type"),
                ],
            })
            .collect(),
    })
}

/// Find tables/views that do NOT have any column matching a name pattern.
pub async fn search_relations_without_column(
    pool: &PgPool,
    schema: &str,
    column_pattern: &str,
    relation_kind: &str,
) -> Result<SearchResult, sqlx::Error> {
    let kind_filter = match relation_kind {
        "tables" => "AND t.table_type = 'BASE TABLE'",
        "views" => "AND t.table_type IN ('VIEW')",
        _ => "",
    };
    let sql = format!(
        r#"
        SELECT t.table_name, t.table_type
        FROM information_schema.tables t
        WHERE t.table_schema = $1
          {kind_filter}
          AND NOT EXISTS (
            SELECT 1 FROM information_schema.columns c
            WHERE c.table_schema = t.table_schema
              AND c.table_name = t.table_name
              AND c.column_name LIKE $2
          )
        ORDER BY t.table_type, t.table_name
        "#,
    );
    let rows = sqlx::query(&sql)
        .bind(schema)
        .bind(column_pattern)
        .fetch_all(pool)
        .await?;
    Ok(SearchResult {
        columns: vec!["Table/View".into(), "Type".into()],
        rows: rows
            .iter()
            .map(|r| SearchResultRow {
                cells: vec![r.get("table_name"), r.get("table_type")],
            })
            .collect(),
    })
}

/// Find tables/views with columns of a specific data type.
pub async fn search_by_column_type(
    pool: &PgPool,
    schema: &str,
    type_pattern: &str,
    relation_kind: &str,
) -> Result<SearchResult, sqlx::Error> {
    let kind_filter = match relation_kind {
        "tables" => "AND t.table_type = 'BASE TABLE'",
        "views" => "AND t.table_type IN ('VIEW')",
        _ => "",
    };
    let sql = format!(
        r#"
        SELECT DISTINCT t.table_name, t.table_type, c.column_name, c.data_type, c.udt_name
        FROM information_schema.tables t
        JOIN information_schema.columns c
          ON c.table_schema = t.table_schema AND c.table_name = t.table_name
        WHERE t.table_schema = $1
          AND (c.data_type ILIKE $2 OR c.udt_name ILIKE $2)
          {kind_filter}
        ORDER BY t.table_type, t.table_name, c.column_name
        "#,
    );
    let rows = sqlx::query(&sql)
        .bind(schema)
        .bind(type_pattern)
        .fetch_all(pool)
        .await?;
    Ok(SearchResult {
        columns: vec!["Table/View".into(), "Type".into(), "Column".into(), "Data Type".into()],
        rows: rows
            .iter()
            .map(|r| SearchResultRow {
                cells: vec![
                    r.get("table_name"),
                    r.get("table_type"),
                    r.get("column_name"),
                    r.get::<String, _>("data_type"),
                ],
            })
            .collect(),
    })
}

/// Find tables without primary keys.
pub async fn search_tables_without_pk(
    pool: &PgPool,
    schema: &str,
) -> Result<SearchResult, sqlx::Error> {
    let rows = sqlx::query(
        r#"
        SELECT t.table_name,
               (SELECT count(*) FROM information_schema.columns c
                WHERE c.table_schema = t.table_schema AND c.table_name = t.table_name)::text AS col_count
        FROM information_schema.tables t
        WHERE t.table_schema = $1
          AND t.table_type = 'BASE TABLE'
          AND NOT EXISTS (
            SELECT 1 FROM information_schema.table_constraints tc
            WHERE tc.table_schema = t.table_schema
              AND tc.table_name = t.table_name
              AND tc.constraint_type = 'PRIMARY KEY'
          )
        ORDER BY t.table_name
        "#,
    )
    .bind(schema)
    .fetch_all(pool)
    .await?;
    Ok(SearchResult {
        columns: vec!["Table".into(), "Columns".into()],
        rows: rows
            .iter()
            .map(|r| SearchResultRow {
                cells: vec![r.get("table_name"), r.get("col_count")],
            })
            .collect(),
    })
}

/// Find tables without any indexes.
pub async fn search_tables_without_indexes(
    pool: &PgPool,
    schema: &str,
) -> Result<SearchResult, sqlx::Error> {
    let rows = sqlx::query(
        r#"
        SELECT t.table_name,
               pg_size_pretty(pg_total_relation_size(quote_ident($1) || '.' || quote_ident(t.table_name))) AS size
        FROM information_schema.tables t
        WHERE t.table_schema = $1
          AND t.table_type = 'BASE TABLE'
          AND NOT EXISTS (
            SELECT 1 FROM pg_indexes i
            WHERE i.schemaname = t.table_schema AND i.tablename = t.table_name
          )
        ORDER BY t.table_name
        "#,
    )
    .bind(schema)
    .fetch_all(pool)
    .await?;
    Ok(SearchResult {
        columns: vec!["Table".into(), "Size".into()],
        rows: rows
            .iter()
            .map(|r| SearchResultRow {
                cells: vec![r.get("table_name"), r.get("size")],
            })
            .collect(),
    })
}

/// Find tables with approximate row counts (from pg_stat).
pub async fn search_tables_by_row_count(
    pool: &PgPool,
    schema: &str,
    min_rows: i64,
    max_rows: i64,
) -> Result<SearchResult, sqlx::Error> {
    let rows = sqlx::query(
        r#"
        SELECT c.relname AS table_name,
               c.reltuples::bigint::text AS approx_rows,
               pg_size_pretty(pg_total_relation_size(c.oid)) AS total_size
        FROM pg_class c
        JOIN pg_namespace n ON n.oid = c.relnamespace
        WHERE n.nspname = $1
          AND c.relkind = 'r'
          AND c.reltuples::bigint BETWEEN $2 AND $3
        ORDER BY c.reltuples::bigint DESC
        "#,
    )
    .bind(schema)
    .bind(min_rows)
    .bind(max_rows)
    .fetch_all(pool)
    .await?;
    Ok(SearchResult {
        columns: vec!["Table".into(), "Approx Rows".into(), "Total Size".into()],
        rows: rows
            .iter()
            .map(|r| SearchResultRow {
                cells: vec![
                    r.get("table_name"),
                    r.get("approx_rows"),
                    r.get("total_size"),
                ],
            })
            .collect(),
    })
}

/// Find all foreign keys referencing a specific table.
pub async fn search_fk_references(
    pool: &PgPool,
    schema: &str,
    target_table: &str,
) -> Result<SearchResult, sqlx::Error> {
    let rows = sqlx::query(
        r#"
        SELECT
            tc.table_name AS source_table,
            kcu.column_name AS source_column,
            ccu.column_name AS target_column,
            tc.constraint_name,
            rc.update_rule,
            rc.delete_rule
        FROM information_schema.table_constraints tc
        JOIN information_schema.key_column_usage kcu
          ON tc.constraint_name = kcu.constraint_name
         AND tc.table_schema = kcu.table_schema
        JOIN information_schema.constraint_column_usage ccu
          ON ccu.constraint_name = tc.constraint_name
         AND ccu.constraint_schema = tc.table_schema
        JOIN information_schema.referential_constraints rc
          ON rc.constraint_name = tc.constraint_name
         AND rc.constraint_schema = tc.table_schema
        WHERE tc.constraint_type = 'FOREIGN KEY'
          AND tc.table_schema = $1
          AND ccu.table_name = $2
        ORDER BY tc.table_name, kcu.column_name
        "#,
    )
    .bind(schema)
    .bind(target_table)
    .fetch_all(pool)
    .await?;
    Ok(SearchResult {
        columns: vec![
            "Source Table".into(),
            "Source Column".into(),
            "Target Column".into(),
            "Constraint".into(),
            "On Update".into(),
            "On Delete".into(),
        ],
        rows: rows
            .iter()
            .map(|r| SearchResultRow {
                cells: vec![
                    r.get("source_table"),
                    r.get("source_column"),
                    r.get("target_column"),
                    r.get("constraint_name"),
                    r.get("update_rule"),
                    r.get("delete_rule"),
                ],
            })
            .collect(),
    })
}

/// Find duplicate indexes (indexes on the same set of columns).
pub async fn search_duplicate_indexes(
    pool: &PgPool,
    schema: &str,
) -> Result<SearchResult, sqlx::Error> {
    let rows = sqlx::query(
        r#"
        WITH idx_cols AS (
            SELECT
                i.indexrelid,
                n.nspname AS schema_name,
                ct.relname AS table_name,
                ci.relname AS index_name,
                pg_get_indexdef(i.indexrelid) AS index_def,
                array_to_string(ARRAY(
                    SELECT a.attname
                    FROM unnest(i.indkey) WITH ORDINALITY AS k(attnum, ord)
                    JOIN pg_attribute a ON a.attrelid = ct.oid AND a.attnum = k.attnum
                    ORDER BY k.ord
                ), ', ') AS columns
            FROM pg_index i
            JOIN pg_class ct ON ct.oid = i.indrelid
            JOIN pg_class ci ON ci.oid = i.indexrelid
            JOIN pg_namespace n ON n.oid = ct.relnamespace
            WHERE n.nspname = $1
        )
        SELECT a.table_name, a.index_name AS index_1, b.index_name AS index_2, a.columns
        FROM idx_cols a
        JOIN idx_cols b ON a.table_name = b.table_name
                       AND a.columns = b.columns
                       AND a.indexrelid < b.indexrelid
        ORDER BY a.table_name, a.index_name
        "#,
    )
    .bind(schema)
    .fetch_all(pool)
    .await?;
    Ok(SearchResult {
        columns: vec!["Table".into(), "Index 1".into(), "Index 2".into(), "Columns".into()],
        rows: rows
            .iter()
            .map(|r| SearchResultRow {
                cells: vec![
                    r.get("table_name"),
                    r.get("index_1"),
                    r.get("index_2"),
                    r.get("columns"),
                ],
            })
            .collect(),
    })
}

/// Find unused indexes (low scan count from pg_stat_user_indexes).
pub async fn search_unused_indexes(
    pool: &PgPool,
    schema: &str,
) -> Result<SearchResult, sqlx::Error> {
    let rows = sqlx::query(
        r#"
        SELECT
            s.relname AS table_name,
            s.indexrelname AS index_name,
            s.idx_scan::text AS scans,
            pg_size_pretty(pg_relation_size(s.indexrelid)) AS index_size,
            pg_get_indexdef(s.indexrelid) AS index_def
        FROM pg_stat_user_indexes s
        JOIN pg_index i ON s.indexrelid = i.indexrelid
        WHERE s.schemaname = $1
          AND s.idx_scan < 10
          AND NOT i.indisunique
          AND NOT i.indisprimary
        ORDER BY pg_relation_size(s.indexrelid) DESC
        "#,
    )
    .bind(schema)
    .fetch_all(pool)
    .await?;
    Ok(SearchResult {
        columns: vec![
            "Table".into(),
            "Index".into(),
            "Scans".into(),
            "Size".into(),
            "Definition".into(),
        ],
        rows: rows
            .iter()
            .map(|r| SearchResultRow {
                cells: vec![
                    r.get("table_name"),
                    r.get("index_name"),
                    r.get("scans"),
                    r.get("index_size"),
                    r.get("index_def"),
                ],
            })
            .collect(),
    })
}

/// Find tables with nullable columns that have no default value.
pub async fn search_nullable_no_default(
    pool: &PgPool,
    schema: &str,
) -> Result<SearchResult, sqlx::Error> {
    let rows = sqlx::query(
        r#"
        SELECT t.table_name, c.column_name, c.data_type, c.is_nullable
        FROM information_schema.tables t
        JOIN information_schema.columns c
          ON c.table_schema = t.table_schema AND c.table_name = t.table_name
        WHERE t.table_schema = $1
          AND t.table_type = 'BASE TABLE'
          AND c.is_nullable = 'YES'
          AND c.column_default IS NULL
        ORDER BY t.table_name, c.ordinal_position
        "#,
    )
    .bind(schema)
    .fetch_all(pool)
    .await?;
    Ok(SearchResult {
        columns: vec!["Table".into(), "Column".into(), "Data Type".into(), "Nullable".into()],
        rows: rows
            .iter()
            .map(|r| SearchResultRow {
                cells: vec![
                    r.get("table_name"),
                    r.get("column_name"),
                    r.get("data_type"),
                    r.get("is_nullable"),
                ],
            })
            .collect(),
    })
}

/// Find tables by size (largest first).
pub async fn search_largest_tables(
    pool: &PgPool,
    schema: &str,
) -> Result<SearchResult, sqlx::Error> {
    let rows = sqlx::query(
        r#"
        SELECT
            c.relname AS table_name,
            c.reltuples::bigint::text AS approx_rows,
            pg_size_pretty(pg_relation_size(c.oid)) AS table_size,
            pg_size_pretty(pg_total_relation_size(c.oid)) AS total_size,
            pg_size_pretty(pg_total_relation_size(c.oid) - pg_relation_size(c.oid)) AS index_size
        FROM pg_class c
        JOIN pg_namespace n ON n.oid = c.relnamespace
        WHERE n.nspname = $1 AND c.relkind = 'r'
        ORDER BY pg_total_relation_size(c.oid) DESC
        LIMIT 50
        "#,
    )
    .bind(schema)
    .fetch_all(pool)
    .await?;
    Ok(SearchResult {
        columns: vec![
            "Table".into(),
            "Approx Rows".into(),
            "Table Size".into(),
            "Total Size".into(),
            "Index Size".into(),
        ],
        rows: rows
            .iter()
            .map(|r| SearchResultRow {
                cells: vec![
                    r.get("table_name"),
                    r.get("approx_rows"),
                    r.get("table_size"),
                    r.get("total_size"),
                    r.get("index_size"),
                ],
            })
            .collect(),
    })
}

/// Find columns across all tables matching a name pattern (global column search).
pub async fn search_columns_globally(
    pool: &PgPool,
    schema: &str,
    column_pattern: &str,
) -> Result<SearchResult, sqlx::Error> {
    let rows = sqlx::query(
        r#"
        SELECT t.table_name, t.table_type, c.column_name, c.data_type,
               c.is_nullable, COALESCE(c.column_default, '—') AS col_default
        FROM information_schema.tables t
        JOIN information_schema.columns c
          ON c.table_schema = t.table_schema AND c.table_name = t.table_name
        WHERE t.table_schema = $1
          AND c.column_name ILIKE $2
        ORDER BY t.table_name, c.ordinal_position
        "#,
    )
    .bind(schema)
    .bind(column_pattern)
    .fetch_all(pool)
    .await?;
    Ok(SearchResult {
        columns: vec![
            "Table/View".into(),
            "Type".into(),
            "Column".into(),
            "Data Type".into(),
            "Nullable".into(),
            "Default".into(),
        ],
        rows: rows
            .iter()
            .map(|r| SearchResultRow {
                cells: vec![
                    r.get("table_name"),
                    r.get("table_type"),
                    r.get("column_name"),
                    r.get("data_type"),
                    r.get("is_nullable"),
                    r.get("col_default"),
                ],
            })
            .collect(),
    })
}
