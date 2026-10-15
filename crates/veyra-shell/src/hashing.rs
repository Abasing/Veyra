//! Canonical hashing for the screenshot/golden infrastructure.
//!
//! **What is hashed** (this is the documented contract referenced by the
//! golden tests and CI): SHA-256 over [`veyra_gfx::PixelBuffer::
//! canonical_bytes`] — the `VEYRA-CANON1` header (magic + width + height,
//! little-endian) followed by the raw RGBA8 pixel rows *without* stride
//! padding. PNG bytes are deliberately **not** hashed: they carry encoder
//! metadata incidental to the image. Two buffers displaying the same
//! image therefore always produce the same hash, regardless of stride or
//! encoder version.
//!
//! The hash is an integrity/identity check for deterministic rendering,
//! not a security primitive; SHA-256 keeps it collision-safe enough to
//! double as a regression-tracking identifier in the compatibility
//! matrix's evidence trail.

use sha2::{Digest, Sha256};

/// Computes the SHA-256 digest over `bytes` and returns lowercase hex.
pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let digest = hasher.finalize();
    let mut out = String::with_capacity(digest.len() * 2);
    for byte in digest {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

/// Parses a 64-character hexadecimal SHA-256 string (case-insensitive).
///
/// Returns `None` when the input is not exactly 64 hex characters.
pub fn parse_sha256_hex(value: &str) -> Option<[u8; 32]> {
    let lowered = value.trim().to_ascii_lowercase();
    if lowered.len() != 64 {
        return None;
    }
    let mut out = [0u8; 32];
    for (i, chunk) in lowered.as_bytes().chunks(2).enumerate() {
        let hi = hex_value(chunk[0])?;
        let lo = hex_value(chunk[1])?;
        out[i] = (hi << 4) | lo;
    }
    Some(out)
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;

    #[test]
    fn known_sha256_vector() {
        // FIPS 180 test vector for the empty message.
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        // And for the two-character sample "abc".
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn hex_output_is_lowercase_64() {
        let hex = sha256_hex(b"veyra");
        assert_eq!(hex.len(), 64);
        assert!(
            hex.chars()
                .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
        );
    }

    #[test]
    fn parse_accepts_case_insensitive_hex() {
        let lower = parse_sha256_hex(&"a".repeat(64)).expect("valid");
        let upper = parse_sha256_hex(&"A".repeat(64)).expect("valid");
        assert_eq!(lower, upper);
    }

    #[test]
    fn parse_rejects_bad_input() {
        assert!(parse_sha256_hex("").is_none());
        assert!(parse_sha256_hex(&"g".repeat(64)).is_none());
        assert!(parse_sha256_hex(&"a".repeat(63)).is_none());
        assert!(parse_sha256_hex(&"a".repeat(65)).is_none());
    }
}
