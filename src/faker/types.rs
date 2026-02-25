//! Type-based value generation.
//!
//! Generates fake values based on PostgreSQL data types when column-name
//! heuristics don't match.

use crate::db::FakeColumnInfo;
use fake::faker::lorem::en::{Sentence, Word};
use fake::Fake;
use rand::Rng;

use super::utils::{csv_quote, random_uuid};

/// Generate a type-based fake value for `col`.
pub fn type_value(col: &FakeColumnInfo, rng: &mut impl Rng) -> String {
    match col.raw_type.as_str() {
        // ── Integers ─────────────────────────────────────────────────────
        "smallint" => rng.gen_range(1_i16..1000).to_string(),
        "integer" => rng.gen_range(1_i32..100_000).to_string(),
        "bigint" => rng.gen_range(1_i64..10_000_000).to_string(),

        // ── Floating-point ───────────────────────────────────────────────
        "real" => format!("{:.4}", rng.gen_range(0.0_f32..1000.0)),
        "double precision" => format!("{:.6}", rng.gen_range(0.0_f64..1_000_000.0)),
        "numeric" | "decimal" => {
            let scale = col.numeric_scale.unwrap_or(2).max(0) as usize;
            let max_int = 10_f64
                .powi(col.numeric_precision.unwrap_or(8) - col.numeric_scale.unwrap_or(2));
            let v = rng.gen_range(0.0..max_int.min(999_999.0));
            format!("{v:.prec$}", prec = scale)
        }

        // ── Boolean ──────────────────────────────────────────────────────
        "boolean" => {
            if rng.gen_bool(0.5) {
                "true".into()
            } else {
                "false".into()
            }
        }

        // ── Text / character types ───────────────────────────────────────
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

        // ── Date / time ──────────────────────────────────────────────────
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

        // ── UUID ─────────────────────────────────────────────────────────
        "uuid" => random_uuid(rng),

        // ── JSON ─────────────────────────────────────────────────────────
        "json" | "jsonb" => {
            let k: String = Word().fake();
            let v: String = Word().fake();
            csv_quote(&format!(r#"{{"{k}": "{v}"}}"#))
        }

        // ── Byte array ───────────────────────────────────────────────────
        "bytea" => {
            let bytes: Vec<u8> = (0..8).map(|_| rng.r#gen()).collect();
            let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
            format!("\\x{hex}")
        }

        // ── Fallback ─────────────────────────────────────────────────────
        _ => {
            // USER-DEFINED / ARRAY / unknown → short random word
            let w: String = Word().fake();
            csv_quote(&w)
        }
    }
}
