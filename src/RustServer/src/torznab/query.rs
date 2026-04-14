//! Torznab query translation.
//!
//! Converts the raw query string bound into [`TorznabRequest`] into a
//! validated [`TorznabQuery`] and then onward into a
//! [`TorrentInfoFilter`] that the torrent repository can consume. Mirrors
//! the `TorznabRequestExtensions.ToTorznabQuery` and
//! `TorznabEndpoints.GetFromTorznabCategories` logic on the .NET side.

use serde::Deserialize;

use crate::domain::torrent::TorrentInfoFilter;

use super::categories::{
    MOVIES, TV, XXX, ids_for_internal_label, internal_label_for_id,
};

/// Raw query-string shape bound by axum. Field names are all lowercase
/// because that's how the .NET `TorznabRequest` class exposes them
/// (deliberately non-PascalCase so the binding is Torznab-spec-friendly).
#[derive(Debug, Clone, Default, Deserialize)]
pub struct TorznabRequest {
    #[serde(default)]
    pub q: Option<String>,
    #[serde(default)]
    pub imdbid: Option<String>,
    #[serde(default)]
    pub ep: Option<String>,
    #[serde(default)]
    pub t: Option<String>,
    #[serde(default)]
    pub extended: Option<String>,
    #[serde(default)]
    pub limit: Option<String>,
    #[serde(default)]
    pub offset: Option<String>,
    #[serde(default)]
    pub cat: Option<String>,
    #[serde(default)]
    pub season: Option<String>,
    #[serde(default)]
    pub year: Option<String>,
}

/// Normalised Torznab query ready for dispatching. Mirrors the .NET
/// `TorznabQuery` type. The `.NET` `TorznabQuery` carries some state we
/// don't need here (interactive search, cached query-string parts) so the
/// Rust port keeps only the fields actually consumed by the handler.
#[derive(Debug, Clone, Default)]
pub struct TorznabQuery {
    pub query_type: String,
    pub search_term: Option<String>,
    pub imdb_id: Option<String>,
    pub season: Option<i32>,
    pub episode: Option<i32>,
    pub year: Option<i32>,
    pub limit: i32,
    pub offset: i32,
    pub categories: Vec<i32>,
}

impl TorznabQuery {
    pub fn is_movie_search(&self) -> bool {
        self.query_type == "movie"
    }
    pub fn is_tv_search(&self) -> bool {
        // .NET accepts both the official "tvsearch" and the historical
        // "tvSearch" value. We accept both on read and compare in lower
        // case.
        self.query_type.eq_ignore_ascii_case("tvsearch")
    }
    pub fn is_xxx_search(&self) -> bool {
        self.query_type == "xxx"
    }
    pub fn is_caps(&self) -> bool {
        self.query_type.eq_ignore_ascii_case("caps")
    }
}

/// Error that may arise while converting a [`TorznabRequest`] into a
/// [`TorznabQuery`]. The numeric code maps directly onto Torznab's error
/// codes (900 = generic invalid, 201 = unsupported).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TorznabQueryError {
    pub code: i32,
    pub description: String,
}

impl std::fmt::Display for TorznabQueryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}] {}", self.code, self.description)
    }
}

impl std::error::Error for TorznabQueryError {}

fn parse_int(s: &str) -> Option<i32> {
    // .NET uses `Parsing.CoerceInt` which strips non-digits before parsing.
    // Our handlers hand us already-cleanish strings, so a plain parse is
    // enough.
    s.trim().parse::<i32>().ok()
}

/// Public entry point used by the HTTP handler.
pub fn from_request(req: TorznabRequest) -> Result<TorznabQuery, TorznabQueryError> {
    translate(req)
}

fn translate(req: TorznabRequest) -> Result<TorznabQuery, TorznabQueryError> {
    let mut q = TorznabQuery {
        query_type: req.t.clone().unwrap_or_else(|| "search".to_string()),
        search_term: req.q,
        imdb_id: req.imdbid,
        ..Default::default()
    };

    if let Some(ref s) = req.season {
        q.season = parse_int(s);
    }
    if let Some(ref ep) = req.ep {
        q.episode = parse_int(ep);
    }
    if let Some(ref y) = req.year {
        q.year = parse_int(y);
    }
    if let Some(ref l) = req.limit {
        q.limit = parse_int(l).unwrap_or(0);
    }
    if let Some(ref o) = req.offset {
        q.offset = parse_int(o).unwrap_or(0);
    }

    q.categories = match req.cat {
        Some(ref cats) if !cats.is_empty() => cats
            .split(',')
            .filter_map(|s| parse_int(s.trim()))
            .collect(),
        _ => default_categories_for(&q.query_type, q.imdb_id.as_deref()),
    };

    Ok(q)
}

fn default_categories_for(query_type: &str, imdb_id: Option<&str>) -> Vec<i32> {
    if imdb_id.is_none_or(str::is_empty) {
        return Vec::new();
    }
    match query_type {
        "movie" => vec![MOVIES.id],
        "tvSearch" | "tvsearch" => vec![TV.id],
        "xxx" => vec![XXX.id],
        _ => Vec::new(),
    }
}

/// Resolve the internal category label (`"movie"` / `"tvSeries"` / `"xxx"`)
/// a query should be filtered by. Matches the precedence in
/// `TorznabEndpoints.GetFromTorznabCategories`:
///
///   1. The explicit query type, if it's one of the three recognised
///      labels (`movie` / `tvSearch` / `xxx`).
///   2. Otherwise, the first recognised parent/subcategory id in the
///      `cat` list.
///   3. Otherwise, `None`.
pub fn internal_label(query_type: &str, categories: &[i32]) -> Option<&'static str> {
    match query_type {
        "movie" => return Some("movie"),
        // Accept both camelCase and lowercase as .NET does.
        "tvSearch" | "tvsearch" => return Some("tvSeries"),
        "xxx" => return Some("xxx"),
        _ => {}
    }
    for id in categories {
        if let Some(label) = internal_label_for_id(*id) {
            return Some(label);
        }
    }
    None
}

/// Map the internal category label back to the Torznab category ids we
/// should emit in an `<item>`.
pub fn emission_ids_for_row(row_category: &str) -> Vec<i32> {
    ids_for_internal_label(row_category)
}

/// Convert a [`TorznabQuery`] to the repository-level filter.
pub fn to_filter(q: &TorznabQuery) -> TorrentInfoFilter {
    TorrentInfoFilter {
        query: q.search_term.clone(),
        season: q.season,
        episode: q.episode,
        year: q.year,
        language: None,
        resolution: None,
        imdb_id: q.imdb_id.clone(),
        category: internal_label(&q.query_type, &q.categories).map(str::to_owned),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_query_type_is_search() {
        let req = TorznabRequest::default();
        let q: TorznabQuery = translate(req).unwrap();
        assert_eq!(q.query_type, "search");
    }

    #[test]
    fn imdb_defaults_to_movies_parent_when_movie() {
        let q = translate(TorznabRequest {
            t: Some("movie".into()),
            imdbid: Some("tt1234567".into()),
            ..Default::default()
        })
        .unwrap();
        assert_eq!(q.categories, vec![MOVIES.id]);
    }

    #[test]
    fn imdb_defaults_to_tv_parent_when_tvsearch() {
        let q = translate(TorznabRequest {
            t: Some("tvSearch".into()),
            imdbid: Some("tt1234567".into()),
            ..Default::default()
        })
        .unwrap();
        assert_eq!(q.categories, vec![TV.id]);
    }

    #[test]
    fn explicit_categories_are_parsed() {
        let q = translate(TorznabRequest {
            t: Some("search".into()),
            cat: Some("2040,2045,5040".into()),
            ..Default::default()
        })
        .unwrap();
        assert_eq!(q.categories, vec![2040, 2045, 5040]);
    }

    #[test]
    fn internal_label_prefers_query_type() {
        assert_eq!(internal_label("movie", &[5000]), Some("movie"));
        assert_eq!(internal_label("tvSearch", &[2000]), Some("tvSeries"));
        assert_eq!(internal_label("xxx", &[]), Some("xxx"));
    }

    #[test]
    fn internal_label_falls_back_to_category_list() {
        assert_eq!(internal_label("search", &[5040]), Some("tvSeries"));
        assert_eq!(internal_label("search", &[2045]), Some("movie"));
        assert_eq!(internal_label("search", &[6060]), Some("xxx"));
    }

    #[test]
    fn internal_label_none_when_unresolvable() {
        assert_eq!(internal_label("search", &[9999]), None);
        assert_eq!(internal_label("search", &[]), None);
    }

    #[test]
    fn limit_parsed_or_zero() {
        let q = translate(TorznabRequest {
            limit: Some("50".into()),
            ..Default::default()
        })
        .unwrap();
        assert_eq!(q.limit, 50);

        let q = translate(TorznabRequest {
            limit: Some("not a number".into()),
            ..Default::default()
        })
        .unwrap();
        assert_eq!(q.limit, 0);
    }
}
