//! Database dialect type-mapping functions.
//!
//! Each function converts a PostgreSQL type string into the equivalent type
//! for a specific target database. Add new dialects by creating a new function
//! here and wiring it up in the parent [`super::map_type`] dispatcher.

use crate::db::MetaColumn;

// ── Oracle ───────────────────────────────────────────────────────────────────

pub fn pg_to_oracle(pg: &str, col: &MetaColumn) -> String {
    let upper = pg.to_uppercase();
    match col.data_type.as_str() {
        "integer" | "smallint" => "NUMBER(10)".into(),
        "bigint" => "NUMBER(19)".into(),
        "boolean" => "NUMBER(1)".into(),
        "real" => "BINARY_FLOAT".into(),
        "double precision" => "BINARY_DOUBLE".into(),
        "text" => "CLOB".into(),
        "bytea" => "BLOB".into(),
        "uuid" => "RAW(16)".into(),
        "json" | "jsonb" => "CLOB".into(),
        "timestamp with time zone" => "TIMESTAMP WITH TIME ZONE".into(),
        "timestamp without time zone" => "TIMESTAMP".into(),
        "time without time zone" => "DATE".into(),
        "time with time zone" => "DATE".into(),
        "interval" => "INTERVAL DAY TO SECOND".into(),
        "character varying" => col
            .char_max
            .map(|n| format!("VARCHAR2({n})"))
            .unwrap_or_else(|| "VARCHAR2(4000)".into()),
        "character" => col
            .char_max
            .map(|n| format!("CHAR({n})"))
            .unwrap_or_else(|| "CHAR(1)".into()),
        "numeric" | "decimal" => {
            if upper.starts_with("NUMERIC") {
                upper.replace("NUMERIC", "NUMBER")
            } else {
                upper
            }
        }
        "ARRAY" => "CLOB".into(),
        "USER-DEFINED" => "CLOB".into(),
        _ => upper,
    }
}

// ── MySQL ────────────────────────────────────────────────────────────────────

pub fn pg_to_mysql(_pg: &str, col: &MetaColumn) -> String {
    match col.data_type.as_str() {
        "integer" => "INT".into(),
        "smallint" => "SMALLINT".into(),
        "bigint" => "BIGINT".into(),
        "real" => "FLOAT".into(),
        "double precision" => "DOUBLE".into(),
        "boolean" => "TINYINT(1)".into(),
        "text" => "LONGTEXT".into(),
        "bytea" => "LONGBLOB".into(),
        "uuid" => "CHAR(36)".into(),
        "json" | "jsonb" => "JSON".into(),
        "timestamp with time zone" | "timestamp without time zone" => "DATETIME".into(),
        "time without time zone" | "time with time zone" => "TIME".into(),
        "interval" => "VARCHAR(255)".into(),
        "date" => "DATE".into(),
        "character varying" => col
            .char_max
            .map(|n| format!("VARCHAR({n})"))
            .unwrap_or_else(|| "TEXT".into()),
        "character" => col
            .char_max
            .map(|n| format!("CHAR({n})"))
            .unwrap_or_else(|| "CHAR(1)".into()),
        "ARRAY" => "JSON".into(),
        "USER-DEFINED" => "TEXT".into(),
        _ => col.data_type.to_uppercase(),
    }
}

// ── SQL Server ───────────────────────────────────────────────────────────────

pub fn pg_to_sqlserver(pg: &str, col: &MetaColumn) -> String {
    let upper = pg.to_uppercase();
    match col.data_type.as_str() {
        "integer" => "INT".into(),
        "smallint" => "SMALLINT".into(),
        "bigint" => "BIGINT".into(),
        "real" => "FLOAT(24)".into(),
        "double precision" => "FLOAT(53)".into(),
        "boolean" => "BIT".into(),
        "text" => "NVARCHAR(MAX)".into(),
        "bytea" => "VARBINARY(MAX)".into(),
        "uuid" => "UNIQUEIDENTIFIER".into(),
        "json" | "jsonb" => "NVARCHAR(MAX)".into(),
        "timestamp with time zone" => "DATETIMEOFFSET".into(),
        "timestamp without time zone" => "DATETIME2".into(),
        "time without time zone" | "time with time zone" => "TIME".into(),
        "interval" => "NVARCHAR(100)".into(),
        "date" => "DATE".into(),
        "character varying" => col
            .char_max
            .map(|n| format!("NVARCHAR({n})"))
            .unwrap_or_else(|| "NVARCHAR(MAX)".into()),
        "character" => col
            .char_max
            .map(|n| format!("NCHAR({n})"))
            .unwrap_or_else(|| "NCHAR(1)".into()),
        "numeric" | "decimal" => {
            if upper.starts_with("NUMERIC") {
                upper.replace("NUMERIC", "DECIMAL")
            } else {
                upper
            }
        }
        "ARRAY" => "NVARCHAR(MAX)".into(),
        "USER-DEFINED" => "NVARCHAR(MAX)".into(),
        _ => upper,
    }
}

// ── Databricks ───────────────────────────────────────────────────────────────

pub fn pg_to_databricks(_pg: &str, col: &MetaColumn) -> String {
    match col.data_type.as_str() {
        "integer" => "INT".into(),
        "smallint" => "SMALLINT".into(),
        "bigint" => "BIGINT".into(),
        "real" => "FLOAT".into(),
        "double precision" => "DOUBLE".into(),
        "boolean" => "BOOLEAN".into(),
        "text" | "character varying" | "character" => "STRING".into(),
        "bytea" => "BINARY".into(),
        "uuid" => "STRING".into(),
        "json" | "jsonb" => "STRING".into(),
        "timestamp with time zone" | "timestamp without time zone" => "TIMESTAMP".into(),
        "time without time zone" | "time with time zone" => "STRING".into(),
        "interval" => "STRING".into(),
        "date" => "DATE".into(),
        "numeric" | "decimal" => match (col.num_prec, col.num_scale) {
            (Some(p), Some(s)) if s > 0 => format!("DECIMAL({p}, {s})"),
            (Some(p), _) => format!("DECIMAL({p}, 0)"),
            _ => "DECIMAL(38, 10)".into(),
        },
        "ARRAY" => "ARRAY<STRING>".into(),
        "USER-DEFINED" => "STRING".into(),
        _ => col.data_type.to_uppercase(),
    }
}

// ── SQLite ───────────────────────────────────────────────────────────────────

pub fn pg_to_sqlite(_pg: &str, col: &MetaColumn) -> String {
    match col.data_type.as_str() {
        "integer" | "smallint" | "bigint" => "INTEGER".into(),
        "real" | "double precision" | "numeric" | "decimal" => "REAL".into(),
        "boolean" => "INTEGER".into(),
        "text" | "character varying" | "character" | "json" | "jsonb" | "uuid" | "interval" => {
            "TEXT".into()
        }
        "bytea" => "BLOB".into(),
        "date" | "time without time zone" | "time with time zone"
        | "timestamp with time zone" | "timestamp without time zone" => "TEXT".into(),
        "ARRAY" | "USER-DEFINED" => "TEXT".into(),
        _ => "TEXT".into(),
    }
}

// ── Snowflake ────────────────────────────────────────────────────────────────

pub fn pg_to_snowflake(pg: &str, col: &MetaColumn) -> String {
    let upper = pg.to_uppercase();
    match col.data_type.as_str() {
        "integer" | "smallint" => "NUMBER(38, 0)".into(),
        "bigint" => "NUMBER(38, 0)".into(),
        "real" => "FLOAT".into(),
        "double precision" => "FLOAT".into(),
        "boolean" => "BOOLEAN".into(),
        "text" => "VARCHAR(16777216)".into(),
        "bytea" => "BINARY".into(),
        "uuid" => "VARCHAR(36)".into(),
        "json" | "jsonb" => "VARIANT".into(),
        "timestamp with time zone" => "TIMESTAMP_TZ".into(),
        "timestamp without time zone" => "TIMESTAMP_NTZ".into(),
        "time without time zone" | "time with time zone" => "TIME".into(),
        "interval" => "VARCHAR(100)".into(),
        "date" => "DATE".into(),
        "character varying" => col
            .char_max
            .map(|n| format!("VARCHAR({n})"))
            .unwrap_or_else(|| "VARCHAR(16777216)".into()),
        "character" => col
            .char_max
            .map(|n| format!("CHAR({n})"))
            .unwrap_or_else(|| "CHAR(1)".into()),
        "numeric" | "decimal" => match (col.num_prec, col.num_scale) {
            (Some(p), Some(s)) if s > 0 => format!("NUMBER({p}, {s})"),
            (Some(p), _) => format!("NUMBER({p}, 0)"),
            _ => "NUMBER(38, 10)".into(),
        },
        "ARRAY" => "VARIANT".into(),
        "USER-DEFINED" => "VARIANT".into(),
        _ => upper,
    }
}
