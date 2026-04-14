//! Query-text helpers shared by the HTTP handlers and the Torznab query
//! translator. Matches the behaviour of the .NET `Parsing` static class
//! (`src/Zilean.Shared/Features/Utilities/Parsing.cs`).

use lazy_static::lazy_static;
use regex::Regex;

lazy_static! {
    // .NET source: StopWordRegex = @"(?i)\b(?:a|the|and|of|in|on|with|to|for|by|is|it)\b"
    static ref STOP_WORDS: Regex = Regex::new(
        r"(?i)\b(?:a|the|and|of|in|on|with|to|for|by|is|it)\b"
    ).expect("stop-word regex must compile");

    // .NET source: SpaceRemovalRegex = @"\s{2,}"
    static ref SQUEEZE_SPACES: Regex = Regex::new(r"\s{2,}")
        .expect("whitespace regex must compile");

    // Matches .NET Parsing.ImdbIdRegex = @"^(?:tt)?(\d{1,8})$"
    static ref IMDB_ID: Regex = Regex::new(r"^(?:tt)?(\d{1,8})$")
        .expect("imdb id regex must compile");
}

/// Strip common English stop words and collapse multi-space runs.
///
/// Mirrors `Parsing.CleanQuery`. Used before a query hits the
/// `search_torrents_meta` / `search_imdb_meta` stored procedures, both of
/// which rely on trigram similarity and do better when the query is
/// stripped of filler words.
pub fn clean_query(input: &str) -> String {
    if input.trim().is_empty() {
        return input.to_owned();
    }
    let stripped = STOP_WORDS.replace_all(input, "");
    SQUEEZE_SPACES.replace_all(&stripped, " ").trim().to_owned()
}

/// Extract the numeric portion of an IMDb id (`tt1234567` or `1234567`).
/// Returns `None` if the input doesn't look like a valid id.
pub fn parse_imdb_id(input: &str) -> Option<u64> {
    IMDB_ID
        .captures(input)
        .and_then(|c| c.get(1))
        .and_then(|m| m.as_str().parse::<u64>().ok())
}

/// Byte size parser for the `Size` column. The value stored there is
/// typically the raw integer byte count as a text string, but DMM and
/// friends sometimes hand us human-readable sizes (`"1.2 GB"`). Matches
/// .NET's `Parsing.GetBytes`.
pub fn parse_bytes(input: &str) -> i64 {
    // First try a plain numeric parse, which is the fast path for the
    // canonical storage format.
    if let Ok(n) = input.trim().parse::<i64>() {
        return n;
    }

    // Fall back to the full human-readable parser.
    let mut digits: String = input
        .chars()
        .filter(|c| c.is_ascii_digit() || *c == '.' || *c == ',')
        .collect();
    digits = if digits.is_empty() { "0".to_string() } else { digits.replace(',', ".") };
    if digits.matches('.').count() > 1 {
        let last = digits.rfind('.').unwrap();
        let (head, tail) = digits.split_at(last);
        digits = format!("{}{}", head.replace('.', ""), tail);
    }

    let unit: String = input
        .chars()
        .filter(|c| c.is_ascii_alphabetic())
        .collect::<String>()
        .replace('i', "")
        .to_ascii_lowercase();
    let value: f64 = digits.parse().unwrap_or(0.0);
    let multiplier: f64 = if unit.contains("tb") {
        1024f64.powi(4)
    } else if unit.contains("gb") {
        1024f64.powi(3)
    } else if unit.contains("mb") {
        1024f64.powi(2)
    } else if unit.contains("kb") {
        1024f64
    } else {
        1.0
    };
    (value * multiplier) as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_query_strips_stopwords_and_collapses() {
        assert_eq!(clean_query("The lord of the rings"), "lord rings");
    }

    #[test]
    fn clean_query_collapses_double_spaces() {
        assert_eq!(clean_query("Lord  of   the Rings"), "Lord Rings");
    }

    #[test]
    fn clean_query_preserves_casing_of_nonstop_words() {
        // Stop words are case-insensitive but non-stop tokens preserve case
        // exactly the way .NET's regex replacement does.
        assert_eq!(clean_query("Lord OF THE Rings"), "Lord Rings");
    }

    #[test]
    fn clean_query_empty_input() {
        assert_eq!(clean_query(""), "");
    }

    #[test]
    fn parse_imdb_id_bare_and_prefixed() {
        assert_eq!(parse_imdb_id("1234567"), Some(1234567));
        assert_eq!(parse_imdb_id("tt1234567"), Some(1234567));
        assert_eq!(parse_imdb_id("nope"), None);
    }

    #[test]
    fn parse_bytes_plain_number() {
        assert_eq!(parse_bytes("1073741824"), 1073741824);
    }

    #[test]
    fn parse_bytes_human_readable() {
        assert_eq!(parse_bytes("1 GB"), 1073741824);
        assert_eq!(parse_bytes("1.5 MB"), 1572864);
    }
}
