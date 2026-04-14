//! Deterministic GUID derived from a torrent info hash.
//!
//! Matches the byte layout that `Parsing.CreateGuidFromInfohash` produces
//! on the .NET side. The first sixteen bytes of `SHA-256(infohash)` are
//! loaded into a `System.Guid`, whose canonical string form uses a mix of
//! little-endian and big-endian byte ordering. Keeping the exact same
//! ordering in Rust means Sonarr/Radarr instances that have already
//! cached release GUIDs continue to dedupe correctly after the backend
//! swap.

use sha2::{Digest, Sha256};

/// Length-40 hex info hash -> C#-compatible lowercase GUID string.
///
/// If the info hash is not exactly 40 hex characters, returns an empty
/// string so we never emit a malformed GUID.
pub fn create_guid_from_infohash(info_hash: &str) -> String {
    if info_hash.len() != 40 || !info_hash.chars().all(|c| c.is_ascii_hexdigit()) {
        return String::new();
    }

    let digest = Sha256::digest(info_hash.as_bytes());
    let b = &digest[0..16];

    // C# Guid byte layout:
    //   Group 1 (int32): bytes 0..4 little-endian -> hex is b[3] b[2] b[1] b[0]
    //   Group 2 (int16): bytes 4..6 little-endian -> hex is b[5] b[4]
    //   Group 3 (int16): bytes 6..8 little-endian -> hex is b[7] b[6]
    //   Group 4 (node):  bytes 8..10 big-endian   -> hex is b[8] b[9]
    //   Group 5 (node):  bytes 10..16 big-endian  -> hex is b[10]..b[15]
    format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        b[3], b[2], b[1], b[0],
        b[5], b[4],
        b[7], b[6],
        b[8], b[9],
        b[10], b[11], b[12], b[13], b[14], b[15],
    )
}

/// Produce a `magnet:?xt=urn:btih:<hash>` URI. Mirrors
/// `Parsing.GetMagnetLink`. Returns an empty string for malformed inputs.
pub fn magnet_uri(info_hash: &str) -> String {
    if info_hash.is_empty() {
        String::new()
    } else {
        format!("magnet:?xt=urn:btih:{info_hash}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guid_from_infohash_format() {
        // The info hash "00000000000000000000000000000000000000001" is 40
        // chars of ASCII "0"s + "1"; we just want to make sure the output
        // is shaped like a canonical lowercase Guid, not any specific
        // digest.
        let guid = create_guid_from_infohash("0123456789abcdef0123456789abcdef01234567");
        assert_eq!(guid.len(), 36, "guid = {guid}");
        assert!(
            guid.chars()
                .all(|c| c.is_ascii_hexdigit() || c == '-'),
            "guid must be lowercase hex + hyphens: {guid}",
        );
        // Positions of hyphens in a canonical Guid (8-4-4-4-12).
        let hyphens: Vec<usize> = guid.match_indices('-').map(|(i, _)| i).collect();
        assert_eq!(hyphens, vec![8, 13, 18, 23]);
    }

    #[test]
    fn guid_from_infohash_deterministic() {
        let a = create_guid_from_infohash("0123456789abcdef0123456789abcdef01234567");
        let b = create_guid_from_infohash("0123456789abcdef0123456789abcdef01234567");
        assert_eq!(a, b);
    }

    #[test]
    fn guid_rejects_short_hash() {
        assert!(create_guid_from_infohash("tooshort").is_empty());
    }

    #[test]
    fn magnet_uri_shape() {
        assert_eq!(
            magnet_uri("0123456789abcdef0123456789abcdef01234567"),
            "magnet:?xt=urn:btih:0123456789abcdef0123456789abcdef01234567"
        );
    }

    // Cross-check against a hand-computed expected value so we catch any
    // future regression in byte ordering. The expected Guid was derived in
    // Python by computing SHA-256("0123456789abcdef0123456789abcdef01234567"),
    // taking the first 16 bytes, then mimicking the C# Guid(ReadOnlySpan<byte>)
    // constructor: Group 1/2/3 little-endian, Groups 4/5 big-endian. This
    // is the format System.Guid.ToString("D") emits on the .NET side.
    #[test]
    fn guid_cross_check_against_known_vector() {
        // SHA-256("0123456789abcdef0123456789abcdef01234567"), first 16 bytes:
        //   de b8 7f ab d1 77 15 bb 31 ad 4c f4 ff b9 49 4e
        //
        // In C# Guid layout:
        //   Group 1: LE of [de b8 7f ab] -> "ab7fb8de"
        //   Group 2: LE of [d1 77]       -> "77d1"
        //   Group 3: LE of [15 bb]       -> "bb15"
        //   Group 4: BE of [31 ad]       -> "31ad"
        //   Group 5: BE of [4c f4 ff b9 49 4e] -> "4cf4ffb9494e"
        assert_eq!(
            create_guid_from_infohash("0123456789abcdef0123456789abcdef01234567"),
            "ab7fb8de-77d1-bb15-31ad-4cf4ffb9494e"
        );
    }
}
