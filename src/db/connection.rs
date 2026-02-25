//! Connection pool creation and PostgreSQL type-name helpers.

use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;

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

/// Map raw PostgreSQL `data_type` / `udt_name` to a human-friendly type string.
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
