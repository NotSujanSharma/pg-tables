//! Pure fake-data generation logic.
//!
//! Given PostgreSQL column metadata ([`crate::db::FakeColumnInfo`]) this module
//! produces plausible random values and assembles them into a CSV document.
//!
//! Strategy
//! --------
//! 1. **Column-name hints** — well-known names (`email`, `first_name`, …) trigger
//!    realistic generators from the `fake` crate.
//! 2. **Data-type dispatch** — everything else is driven by `raw_type`,
//!    `char_max`, `numeric_precision / scale`, etc.
//! 3. **Nullable columns** — 10 % of the time we emit `NULL` unless the column
//!    has a default expression (in which case we still generate a value so the
//!    CSV can stand alone without the database).

mod hints;
mod types;
pub(crate) mod utils;

use crate::db::FakeColumnInfo;
use rand::Rng;

use self::hints::name_hint;
use self::types::type_value;
use self::utils::csv_quote;

// ── Public API ────────────────────────────────────────────────────────────────

/// Generate `row_count` rows of fake data as a CSV string (UTF-8).
///
/// The first line is the header row with column names.
pub fn generate_csv(columns: &[FakeColumnInfo], row_count: usize) -> String {
    let mut rng = rand::thread_rng();

    let mut out = String::with_capacity(row_count * columns.len() * 12);

    // Header
    let header: Vec<String> = columns.iter().map(|c| csv_quote(&c.name)).collect();
    out.push_str(&header.join(","));
    out.push('\n');

    // Rows
    for _ in 0..row_count {
        let row: Vec<String> = columns
            .iter()
            .map(|col| generate_value(col, &mut rng))
            .collect();
        out.push_str(&row.join(","));
        out.push('\n');
    }

    out
}

// ── Value generation ──────────────────────────────────────────────────────────

/// Generate a single CSV-safe value for `col`.
pub fn generate_value(col: &FakeColumnInfo, rng: &mut impl Rng) -> String {
    // Nullable: 10 % chance of NULL (only when truly nullable AND no default)
    if col.is_nullable && col.column_default.is_none() && rng.gen_bool(0.10) {
        return String::new(); // empty CSV field = NULL
    }

    // nextval / sequences → generate a sequential-ish integer
    if let Some(ref def) = col.column_default {
        if def.contains("nextval") {
            return rng.gen_range(1_u64..1_000_000).to_string();
        }
    }

    // Try a name-hint first, then fall back to type-based generation
    let name_lower = col.name.to_lowercase();
    if let Some(v) = name_hint(&name_lower, rng) {
        // Respect VARCHAR max length if hint is over budget
        if let Some(max) = col.char_max {
            if v.len() > max as usize {
                return csv_quote(&v[..max as usize]);
            }
        }
        return csv_quote(&v);
    }

    type_value(col, rng)
}
