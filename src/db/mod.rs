//! Database access layer for PostgreSQL.
//!
//! Each sub-module owns a distinct responsibility:
//!
//! - [`connection`] — pool creation and PostgreSQL type helpers
//! - [`schema`]     — schema/table/view/column listing
//! - [`metadata`]   — full table metadata for DDL generation
//! - [`query`]      — arbitrary SQL execution
//! - [`search`]     — advanced search queries
//! - [`fake_columns`] — column info for fake data generation

pub mod connection;
pub mod fake_columns;
pub mod metadata;
pub mod query;
pub mod schema;
pub mod search;

// Re-export all public items so callers can continue using `crate::db::*`.
#[allow(unused_imports)]
pub use connection::{connect, pg_type};
pub use fake_columns::{FakeColumnInfo, fetch_fake_columns};
#[allow(unused_imports)]
pub use metadata::{
    MetaCheck, MetaColumn, MetaFk, TableMeta, fetch_table_meta, generate_create_script,
};
pub use query::{QueryResult, execute_query};
#[allow(unused_imports)]
pub use schema::{ColumnInfo, Dep, list_columns, list_schemas, list_tables, list_views, view_dependencies};
#[allow(unused_imports)]
pub use search::{
    SearchResult, SearchResultRow, search_by_column_type, search_columns_globally,
    search_duplicate_indexes, search_fk_references, search_largest_tables,
    search_nullable_no_default, search_relations_with_column, search_relations_without_column,
    search_tables_by_row_count, search_tables_without_indexes, search_tables_without_pk,
    search_unused_indexes,
};
