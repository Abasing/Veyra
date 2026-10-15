//! Input decoding: bytes → UTF-8 text.
//!
//! Phase 2 subset of the Encoding standard ([ST6]): BOM sniffing
//! (UTF-8/UTF-16LE/UTF-16BE), then UTF-8 with replacement-character
//! recovery. The full encoding stack — meta-charset discovery, the legacy
//! windows-1252 fallback, label lookup — is deferred and recorded as such
//! in the compatibility matrix; no other encoding is silently mis-decoded
//! (it decodes lossily as UTF-8, which the matrix documents).

/// The decoded input plus what the BOM said about it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Decoded {
    /// The text, BOM stripped.
    pub text: String,
    /// Whether a BOM was present and consumed.
    pub bom: Option<Bom>,
}

/// The byte-order marks this decoder recognizes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bom {
    /// UTF-8 BOM (`EF BB BF`).
    Utf8,
    /// UTF-16 little-endian BOM (`FF FE`).
    Utf16Le,
    /// UTF-16 big-endian BOM (`FE FF`).
    Utf16Be,
}

impl Bom {
    /// Sniffs a BOM at the start of `bytes`.
    pub fn sniff(bytes: &[u8]) -> Option<Self> {
        if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
            Some(Self::Utf8)
        } else if bytes.starts_with(&[0xFF, 0xFE]) {
            Some(Self::Utf16Le)
        } else if bytes.starts_with(&[0xFE, 0xFF]) {
            Some(Self::Utf16Be)
        } else {
            None
        }
    }
}

/// Decodes `bytes` to UTF-8 text (see the module docs for the subset).
///
/// UTF-16 decoding pairs surrogate escapes; unpaired surrogates become
/// U+FFFD, matching the standard's error recovery intent. UTF-8 input that
/// is not valid UTF-8 goes through lossy conversion (each bad byte run →
/// one U+FFFD).
#[must_use]
pub fn decode(bytes: &[u8]) -> Decoded {
    match Bom::sniff(bytes) {
        Some(Bom::Utf8) => Decoded {
            text: String::from_utf8_lossy(&bytes[3..]).into_owned(),
            bom: Some(Bom::Utf8),
        },
        Some(Bom::Utf16Le) => Decoded {
            text: decode_utf16(&bytes[2..], true),
            bom: Some(Bom::Utf16Le),
        },
        Some(Bom::Utf16Be) => Decoded {
            text: decode_utf16(&bytes[2..], false),
            bom: Some(Bom::Utf16Be),
        },
        None => Decoded {
            text: String::from_utf8_lossy(bytes).into_owned(),
            bom: None,
        },
    }
}

/// UTF-16 → UTF-8 with unpaired-surrogate recovery.
fn decode_utf16(bytes: &[u8], little_endian: bool) -> String {
    let units: Vec<u16> = bytes
        .chunks(2)
        .filter(|pair| pair.len() == 2)
        .map(|pair| {
            if little_endian {
                u16::from_le_bytes([pair[0], pair[1]])
            } else {
                u16::from_be_bytes([pair[0], pair[1]])
            }
        })
        .collect();
    String::from_utf16_lossy(&units)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;

    #[test]
    fn utf8_bom_is_stripped() {
        let decoded = decode(b"\xEF\xBB\xBF<html>");
        assert_eq!(decoded.bom, Some(Bom::Utf8));
        assert_eq!(decoded.text, "<html>");
    }

    #[test]
    fn utf16_le_and_be_decode() {
        let decoded = decode(&[0xFF, 0xFE, b'a', 0x00, b'b', 0x00]);
        assert_eq!(decoded.bom, Some(Bom::Utf16Le));
        assert_eq!(decoded.text, "ab");

        let decoded = decode(&[0xFE, 0xFF, 0x00, b'a', 0x00, b'b']);
        assert_eq!(decoded.bom, Some(Bom::Utf16Be));
        assert_eq!(decoded.text, "ab");
    }

    #[test]
    fn no_bom_means_utf8_lossy() {
        let decoded = decode(b"<p>ok</p>");
        assert_eq!(decoded.bom, None);
        assert_eq!(decoded.text, "<p>ok</p>");

        // Invalid UTF-8 becomes U+FFFD, never a panic.
        let decoded = decode(&[0x61, 0xFF, 0x62]);
        assert_eq!(decoded.text, "a\u{FFFD}b");
    }

    #[test]
    fn unpaired_surrogates_become_replacement() {
        let decoded = decode(&[0xFF, 0xFE, 0x00, 0xD8, b'a', 0x00]);
        assert_eq!(decoded.text, "\u{FFFD}a");
    }
}
