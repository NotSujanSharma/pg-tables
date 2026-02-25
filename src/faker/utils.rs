//! Shared helper utilities for the faker module.

use rand::Rng;

/// RFC 4122 v4 UUID produced from random bytes.
pub fn random_uuid(rng: &mut impl Rng) -> String {
    let a: u32 = rng.r#gen();
    let b: u16 = rng.r#gen();
    let c: u16 = (rng.r#gen::<u16>() & 0x0fff) | 0x4000; // version 4
    let d: u16 = (rng.r#gen::<u16>() & 0x3fff) | 0x8000; // variant 1
    let e: u64 = rng.r#gen::<u64>() & 0x0000_ffff_ffff_ffff;
    format!("{a:08x}-{b:04x}-{c:04x}-{d:04x}-{e:012x}")
}

/// Quote a CSV field if it contains commas, quotes, or newlines.
pub fn csv_quote(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') || s.contains('\r') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}
