//! Arbitrary SQL execution.

use sqlx::{Column, PgPool, Row, TypeInfo};

/// Result of an arbitrary SQL query.
#[derive(Clone, Default)]
pub struct QueryResult {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<String>>,
    pub affected: u64,
    pub is_select: bool,
}

/// Execute an arbitrary SQL statement and return results.
pub async fn execute_query(pool: &PgPool, sql: &str) -> Result<QueryResult, String> {
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
