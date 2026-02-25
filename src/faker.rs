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

use crate::db::FakeColumnInfo;
use fake::faker::address::en::{CityName, CountryName, StateAbbr, StreetName, ZipCode};
use fake::faker::company::en::CompanyName;
use fake::faker::internet::en::{FreeEmail, SafeEmail, Username};
use fake::faker::lorem::en::{Sentence, Word, Words};
use fake::faker::name::en::{FirstName, LastName, Name};
use fake::faker::phone_number::en::PhoneNumber;
use fake::Fake;
use rand::Rng;

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

// ── Name-hint dispatch ────────────────────────────────────────────────────────

fn name_hint(col_name: &str, _rng: &mut impl Rng) -> Option<String> {
    // Match on common column-name patterns
    if col_name == "email" || col_name.ends_with("_email") || col_name.starts_with("email_") {
        return Some(FreeEmail().fake::<String>());
    }
    if col_name == "safe_email" {
        return Some(SafeEmail().fake::<String>());
    }
    if col_name == "username" || col_name == "user_name" || col_name == "login" {
        return Some(Username().fake::<String>());
    }
    if col_name == "first_name" || col_name == "firstname" || col_name == "given_name" {
        return Some(FirstName().fake::<String>());
    }
    if col_name == "last_name" || col_name == "lastname" || col_name == "surname"
        || col_name == "family_name"
    {
        return Some(LastName().fake::<String>());
    }
    if col_name == "name"
        || col_name == "full_name"
        || col_name == "fullname"
        || col_name == "display_name"
    {
        return Some(Name().fake::<String>());
    }
    if col_name == "phone" || col_name == "phone_number" || col_name == "mobile"
        || col_name == "tel"
    {
        return Some(PhoneNumber().fake::<String>());
    }
    if col_name == "city" || col_name == "city_name" {
        return Some(CityName().fake::<String>());
    }
    if col_name == "country" || col_name == "country_name" {
        return Some(CountryName().fake::<String>());
    }
    if col_name == "state" || col_name == "province" || col_name == "region" {
        return Some(StateAbbr().fake::<String>());
    }
    if col_name == "zip" || col_name == "zip_code" || col_name == "postal_code"
        || col_name == "postcode"
    {
        return Some(ZipCode().fake::<String>());
    }
    if col_name.contains("street") || col_name == "address_line" || col_name == "address1" {
        return Some(StreetName().fake::<String>());
    }
    if col_name == "company" || col_name == "company_name" || col_name == "organisation"
        || col_name == "organization"
    {
        return Some(CompanyName().fake::<String>());
    }
    if col_name == "word" || col_name == "tag" || col_name == "key" {
        return Some(Word().fake::<String>());
    }
    if col_name.contains("description")
        || col_name.contains("comment")
        || col_name.contains("note")
        || col_name == "bio"
        || col_name == "summary"
        || col_name == "content"
        || col_name == "body"
        || col_name == "message"
    {
        let words: Vec<String> = Words(5..10).fake();
        return Some(words.join(" "));
    }
    if col_name == "title" || col_name == "subject" {
        let words: Vec<String> = Words(3..6).fake();
        return Some(words.join(" "));
    }
    if col_name.contains("sentence") || col_name == "text" || col_name == "excerpt" {
        return Some(Sentence(6..12).fake::<String>());
    }

    None
}

// ── Type-based value generation ───────────────────────────────────────────────

fn type_value(col: &FakeColumnInfo, rng: &mut impl Rng) -> String {
    match col.raw_type.as_str() {
        // ── Integers ─────────────────────────────────────────────────────────
        "smallint" => rng.gen_range(1_i16..1000).to_string(),
        "integer" => rng.gen_range(1_i32..100_000).to_string(),
        "bigint" => rng.gen_range(1_i64..10_000_000).to_string(),

        // ── Floating-point ────────────────────────────────────────────────────
        "real" => format!("{:.4}", rng.gen_range(0.0_f32..1000.0)),
        "double precision" => format!("{:.6}", rng.gen_range(0.0_f64..1_000_000.0)),
        "numeric" | "decimal" => {
            let scale = col.numeric_scale.unwrap_or(2).max(0) as usize;
            let max_int = 10_f64.powi(col.numeric_precision.unwrap_or(8) - col.numeric_scale.unwrap_or(2));
            let v = rng.gen_range(0.0..max_int.min(999_999.0));
            format!("{v:.prec$}", prec = scale)
        }

        // ── Boolean ───────────────────────────────────────────────────────────
        "boolean" => if rng.gen_bool(0.5) { "true".into() } else { "false".into() },

        // ── Text / character types ────────────────────────────────────────────
        "character varying" | "character" => {
            let max = col.char_max.unwrap_or(32) as usize;
            let len = rng.gen_range(3..=max.min(24).max(3));
            // Use a lorem word for short fields, sentence for long
            if max <= 64 {
                let w: String = Word().fake();
                csv_quote(&w[..w.len().min(len)])
            } else {
                let s: String = Sentence(3..7).fake();
                csv_quote(&s[..s.len().min(max)])
            }
        }
        "text" => {
            let s: String = Sentence(5..12).fake();
            csv_quote(&s)
        }

        // ── Date / time ───────────────────────────────────────────────────────
        "date" => {
            let y = rng.gen_range(2000..2025_u32);
            let m = rng.gen_range(1..=12_u32);
            let d = rng.gen_range(1..=28_u32);
            format!("{y}-{m:02}-{d:02}")
        }
        "time without time zone" => {
            format!(
                "{:02}:{:02}:{:02}",
                rng.gen_range(0..24_u32),
                rng.gen_range(0..60_u32),
                rng.gen_range(0..60_u32)
            )
        }
        "time with time zone" => {
            format!(
                "{:02}:{:02}:{:02}+00",
                rng.gen_range(0..24_u32),
                rng.gen_range(0..60_u32),
                rng.gen_range(0..60_u32)
            )
        }
        "timestamp without time zone" | "timestamp with time zone" => {
            let y = rng.gen_range(2000..2025_u32);
            let mo = rng.gen_range(1..=12_u32);
            let d = rng.gen_range(1..=28_u32);
            let h = rng.gen_range(0..24_u32);
            let mi = rng.gen_range(0..60_u32);
            let s = rng.gen_range(0..60_u32);
            format!("{y}-{mo:02}-{d:02} {h:02}:{mi:02}:{s:02}")
        }
        "interval" => format!(
            "{:02}:{:02}:{:02}",
            rng.gen_range(0..48_u32),
            rng.gen_range(0..60_u32),
            rng.gen_range(0..60_u32)
        ),

        // ── UUID ──────────────────────────────────────────────────────────────
        "uuid" => random_uuid(rng),

        // ── JSON ──────────────────────────────────────────────────────────────
        "json" | "jsonb" => {
            let k: String = Word().fake();
            let v: String = Word().fake();
            csv_quote(&format!(r#"{{"{k}": "{v}"}}"#))
        }

        // ── Byte array ────────────────────────────────────────────────────────
        "bytea" => {
            let bytes: Vec<u8> = (0..8).map(|_| rng.r#gen()).collect();
            let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
            format!("\\x{hex}")
        }

        // ── Fallback ──────────────────────────────────────────────────────────
        _ => {
            // USER-DEFINED / ARRAY / unknown → short random word
            let w: String = Word().fake();
            csv_quote(&w)
        }
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// RFC 4122 v4 UUID produced from random bytes.
fn random_uuid(rng: &mut impl Rng) -> String {
    let a: u32 = rng.r#gen();
    let b: u16 = rng.r#gen();
    let c: u16 = (rng.r#gen::<u16>() & 0x0fff) | 0x4000; // version 4
    let d: u16 = (rng.r#gen::<u16>() & 0x3fff) | 0x8000; // variant 1
    let e: u64 = rng.r#gen::<u64>() & 0x0000_ffff_ffff_ffff;
    format!("{a:08x}-{b:04x}-{c:04x}-{d:04x}-{e:012x}")
}

/// Quote a CSV field if it contains commas, quotes, or newlines.
fn csv_quote(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') || s.contains('\r') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}
