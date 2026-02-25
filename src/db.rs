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

/// Generate the CREATE TABLE script as a string.
pub async fn generate_create_script(
    pool: &PgPool,
    schema: &str,
    table: &str,
) -> Result<String, sqlx::Error> {
    // ── columns ──────────────────────────────────────────────────────────────
    let col_rows = sqlx::query(
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
    .await?;

    if col_rows.is_empty() {
        return Ok(format!(
            "-- Table '{schema}.{table}' not found or has no columns."
        ));
    }

    // ── primary key ───────────────────────────────────────────────────────────
    let pk_cols: Vec<String> = sqlx::query(
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

    // ── unique constraints ────────────────────────────────────────────────────
    let mut unique_map: BTreeMap<String, Vec<String>> = BTreeMap::new();
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
        unique_map
            .entry(row.get("constraint_name"))
            .or_default()
            .push(row.get("column_name"));
    }

    // ── foreign keys ──────────────────────────────────────────────────────────
    struct FkInfo {
        columns: Vec<String>,
        foreign_schema: String,
        foreign_table: String,
        foreign_columns: Vec<String>,
        update_rule: String,
        delete_rule: String,
    }
    let mut fk_map: BTreeMap<String, FkInfo> = BTreeMap::new();
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
        let entry = fk_map.entry(cname).or_insert_with(|| FkInfo {
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

    // ── check constraints ─────────────────────────────────────────────────────
    let check_rows = sqlx::query(
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
    .await?;

    // ── assemble script ───────────────────────────────────────────────────────
    let mut parts: Vec<String> = vec![];

    for row in &col_rows {
        let col_name: String = row.get("column_name");
        let data_type: String = row.get("data_type");
        let udt_name: String = row.get("udt_name");
        let char_max: Option<i32> = row.get("character_maximum_length");
        let num_prec: Option<i32> = row.get("numeric_precision");
        let num_scale: Option<i32> = row.get("numeric_scale");
        let is_nullable: String = row.get("is_nullable");
        let default: Option<String> = row.get("column_default");

        let type_str = pg_type(&data_type, &udt_name, char_max, num_prec, num_scale);
        let mut def = format!("    {col_name} {type_str}");
        if let Some(d) = default {
            def.push_str(&format!(" DEFAULT {d}"));
        }
        if is_nullable == "NO" && !pk_cols.contains(&col_name) {
            def.push_str(" NOT NULL");
        }
        parts.push(def);
    }

    if !pk_cols.is_empty() {
        parts.push(format!("    PRIMARY KEY ({})", pk_cols.join(", ")));
    }
    for (cname, cols) in &unique_map {
        parts.push(format!(
            "    CONSTRAINT {cname} UNIQUE ({})",
            cols.join(", ")
        ));
    }
    for (cname, fk) in &fk_map {
        let ref_table = if fk.foreign_schema == schema {
            fk.foreign_table.clone()
        } else {
            format!("{}.{}", fk.foreign_schema, fk.foreign_table)
        };
        let mut s = format!(
            "    CONSTRAINT {cname} FOREIGN KEY ({}) REFERENCES {ref_table} ({})",
            fk.columns.join(", "),
            fk.foreign_columns.join(", ")
        );
        if fk.update_rule != "NO ACTION" {
            s.push_str(&format!(" ON UPDATE {}", fk.update_rule));
        }
        if fk.delete_rule != "NO ACTION" {
            s.push_str(&format!(" ON DELETE {}", fk.delete_rule));
        }
        parts.push(s);
    }
    for row in &check_rows {
        let cname: String = row.get("constraint_name");
        let clause: String = row.get("check_clause");
        parts.push(format!("    CONSTRAINT {cname} CHECK {clause}"));
    }

    let script = format!(
        "CREATE TABLE {schema}.{table} (\n{}\n);",
        parts.join(",\n")
    );
    Ok(script)
}

fn pg_type(
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
