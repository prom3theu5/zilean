//! Helpers shared by every Torznab XML writer.

use chrono::{DateTime, Utc};

/// RFC 822 date formatter matching the .NET `XmlDateFormat` helper:
///
/// ```text
/// Sat, 14 Mar 2015 17:10:42 -0400
/// ```
///
/// chrono's `%a` and `%b` are locale-independent English abbreviations and
/// `%z` produces `+0000`-style offsets with no colon, so the formatted
/// string is byte-identical to the .NET output (modulo the `en-US` culture
/// the C# code forces explicitly).
pub fn xml_date_format(dt: DateTime<Utc>) -> String {
    dt.format("%a, %d %b %Y %H:%M:%S %z").to_string()
}

/// Strip code points that must never appear in XML 1.0 text content:
///
/// * control characters 0x00-0x08, 0x0B, 0x0C, 0x0E-0x1F, 0x7F-0x9F
/// * byte-order mark and non-characters (U+FEFF, U+FFFE, U+FFFF)
///
/// Rust strings are already guaranteed well-formed UTF-8 so we do not need
/// the surrogate-pair clauses from the .NET regex (orphan surrogates
/// cannot exist in an `&str`).
pub fn strip_invalid_xml_chars(text: &str) -> String {
    text.chars()
        .filter(|c| {
            let cp = *c as u32;
            !matches!(
                cp,
                0x00..=0x08
                    | 0x0B
                    | 0x0C
                    | 0x0E..=0x1F
                    | 0x7F..=0x9F
                    | 0xFEFF
                    | 0xFFFE
                    | 0xFFFF
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn rfc822_format_is_en_us_with_colonless_offset() {
        let dt = Utc.with_ymd_and_hms(2015, 3, 14, 17, 10, 42).unwrap();
        assert_eq!(xml_date_format(dt), "Sat, 14 Mar 2015 17:10:42 +0000");
    }

    #[test]
    fn strips_control_characters() {
        // \x07 (BEL) and \x1B (ESC) are illegal; \x09 (TAB) is not.
        let input = "Title\x07With\tControls\x1B!";
        assert_eq!(strip_invalid_xml_chars(input), "TitleWith\tControls!");
    }

    #[test]
    fn passes_through_ordinary_text() {
        let input = "The Lord of the Rings (2001) 1080p BluRay";
        assert_eq!(strip_invalid_xml_chars(input), input);
    }

    #[test]
    fn strips_bom_and_noncharacters() {
        let input = "x\u{FEFF}y\u{FFFE}z\u{FFFF}";
        assert_eq!(strip_invalid_xml_chars(input), "xyz");
    }
}
