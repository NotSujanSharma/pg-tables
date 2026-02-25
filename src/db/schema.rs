//! Schema, table, view, and column listing queries.

use super::connection::pg_type;
use sqlx::{PgPool, Row};

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
