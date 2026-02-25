//! Advanced search queries — find tables, views, columns by various criteria.

use sqlx::{PgPool, Row};

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
        columns: vec![
            "Table/View".into(),
            "Type".into(),
            "Column".into(),
            "Data Type".into(),
        ],
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
        columns: vec![
            "Table/View".into(),
            "Type".into(),
            "Column".into(),
            "Data Type".into(),
        ],
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
        columns: vec![
            "Table".into(),
            "Index 1".into(),
            "Index 2".into(),
            "Columns".into(),
        ],
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
        columns: vec![
            "Table".into(),
            "Column".into(),
            "Data Type".into(),
            "Nullable".into(),
        ],
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
