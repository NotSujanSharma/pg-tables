//! Full table metadata — used by the schema/DDL generator.

use sqlx::{PgPool, Row};
use std::collections::BTreeMap;

/// Raw column metadata extracted from `information_schema`.
#[derive(Clone, Debug)]
pub struct MetaColumn {
    pub name: String,
    pub data_type: String, // raw info_schema data_type
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
    Ok(crate::schema::format_table(
        &meta,
        crate::schema::SchemaFormat::Postgres,
    ))
}
