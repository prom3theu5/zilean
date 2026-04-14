//! `GET /torznab/api` — capabilities + search. The single most important
//! HTTP surface because Sonarr, Radarr and Prowlarr all talk to Zilean
//! via this endpoint.

use axum::Router;
use axum::extract::{Query, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;

use crate::db::torrent as repo;
use crate::torznab::caps::{LIMITS_DEFAULT, LIMITS_MAX};
use crate::torznab::guid::{create_guid_from_infohash, magnet_uri};
use crate::torznab::query::{self as tz_query, TorznabQuery, TorznabRequest};
use crate::torznab::result_page::{self, Release};
use crate::torznab::{caps, error};
use crate::utils::query::{clean_query, parse_bytes, parse_imdb_id};

use crate::domain::torrent::{TorrentInfo, TorrentInfoFilter};
use crate::http::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/torznab/api", get(api))
}

/// Helper that wraps an XML body in a Response with `application/xml`
/// content type. Matches the .NET `XmlResult<T>`.
fn xml_response(body: String, status: StatusCode) -> Response {
    Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, HeaderValue::from_static("application/xml"))
        .body(body.into())
        .expect("response builder")
}

async fn api(
    State(state): State<AppState>,
    Query(req): Query<TorznabRequest>,
    // The Torznab handler needs the full URL to echo it back in
    // `<atom:link rel="self">`. axum doesn't expose the full URL as a
    // first-class extractor; rebuild it from scheme/host/path in the
    // handler body by reading the headers we expect behind a reverse
    // proxy.
    headers: axum::http::HeaderMap,
) -> Response {
    let self_link = build_self_link(&headers);

    let query: TorznabQuery = match tz_query::from_request(req) {
        Ok(q) => q,
        Err(err) => return xml_response(error::render(err.code, &err.description), StatusCode::BAD_REQUEST),
    };

    // Capabilities response.
    if query.is_caps() {
        return xml_response(caps::render(), StatusCode::OK);
    }

    if query.limit > LIMITS_MAX {
        return xml_response(
            error::render(
                900,
                &format!("Requested limit exceeds maximum allowed ({LIMITS_MAX})."),
            ),
            StatusCode::BAD_REQUEST,
        );
    }

    // Effective per-page limit mirrors the .NET switch:
    //   0 -> default,   positive -> cap at itself,   anything else -> default.
    let effective_limit = match query.limit {
        n if n > 0 => n,
        _ => LIMITS_DEFAULT,
    };

    // Build the repository filter.
    let filter = TorrentInfoFilter {
        query: query.search_term.as_deref().map(clean_query),
        season: query.season,
        episode: query.episode,
        year: query.year,
        language: None,
        resolution: None,
        imdb_id: query.imdb_id.clone(),
        category: tz_query::internal_label(&query.query_type, &query.categories)
            .map(str::to_owned),
    };

    let results = match repo::search_filtered(
        &state.db,
        &filter,
        effective_limit,
        state.config.dmm_minimum_score,
    )
    .await
    {
        Ok(rows) => rows,
        Err(err) => {
            tracing::error!(?err, ?query, "torznab search failed");
            return xml_response(
                error::render(900, &err.to_string()),
                StatusCode::BAD_REQUEST,
            );
        }
    };

    let releases: Vec<Release> = results.iter().map(to_release).collect();
    xml_response(result_page::render(&self_link, &releases), StatusCode::OK)
}

fn to_release(row: &TorrentInfo) -> Release {
    let info_hash = row.info_hash.clone();
    let magnet = magnet_uri(&info_hash);
    let guid = create_guid_from_infohash(&info_hash);

    let size = row.size.as_deref().map(parse_bytes);
    let categories = tz_query::emission_ids_for_row(&row.category);
    let imdb = row.imdb_id.as_deref().and_then(parse_imdb_id);
    let languages = row.languages.clone();

    Release {
        title: row.raw_title.clone().unwrap_or_default(),
        guid,
        info_hash,
        magnet,
        publish_date: Some(row.ingested_at),
        size,
        categories,
        imdb,
        languages,
        year: row.year,
    }
}

/// Reconstruct the request URL from the proxy headers the .NET version
/// ends up using (`X-Forwarded-*`). Behind Kubernetes/nginx this is the
/// path most users run; if the request has no such headers we fall back
/// to `http://localhost/torznab/api` which still parses correctly for
/// Sonarr/Radarr's cache.
fn build_self_link(headers: &axum::http::HeaderMap) -> String {
    let scheme = headers
        .get("x-forwarded-proto")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("http");
    let host = headers
        .get("x-forwarded-host")
        .or_else(|| headers.get(header::HOST))
        .and_then(|v| v.to_str().ok())
        .unwrap_or("localhost");
    format!("{scheme}://{host}/torznab/api")
}
