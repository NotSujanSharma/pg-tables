//! Column metadata for fake data generation.

use super::connection::pg_type;
use sqlx::{PgPool, Row};

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
