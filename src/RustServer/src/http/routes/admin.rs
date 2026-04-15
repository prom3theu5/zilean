//! Admin portal (PWA).
//!
//! This module serves two related things under `/admin`:
//!
//! * **Static assets** — the PWA shell (index.html, app.js, app.css,
//!   manifest, service worker, icon). These are embedded at compile
//!   time via `include_str!` so the binary is self-contained; no files
//!   need to be shipped alongside it. Static routes are NOT behind the
//!   API-key middleware — the HTML page itself needs to load so the
//!   operator can type the key into the login form.
//!
//! * **Admin JSON API** — everything under `/admin/api/*`. Gated by the
//!   API-key middleware, mirrors the .NET dashboard data adapter + the
//!   scheduler trigger buttons.

use axum::Router;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::middleware;
use axum::response::{Json, Redirect, Response};
use axum::routing::{delete, get, post};
use serde::{Deserialize, Serialize};
use sqlx::Row;

use crate::db::{blacklist as blacklist_repo, torrent as torrent_repo};
use crate::domain::blacklist::BlacklistedItem;
use crate::domain::torrent::TorrentInfo;
use crate::http::auth::require_api_key;
use crate::http::state::AppState;

// ---------------------------------------------------------------------------
// Static asset registration
// ---------------------------------------------------------------------------

const INDEX_HTML: &str = include_str!("../../../assets/admin/index.html");
const APP_JS: &str = include_str!("../../../assets/admin/app.js");
const APP_CSS: &str = include_str!("../../../assets/admin/app.css");
const MANIFEST: &str = include_str!("../../../assets/admin/manifest.webmanifest");
const SW_JS: &str = include_str!("../../../assets/admin/sw.js");
const ICON_SVG: &str = include_str!("../../../assets/admin/icon.svg");

fn asset_response(body: &'static str, content_type: &'static str) -> Response {
    // Strict charset on text types so browsers pick UTF-8 unambiguously.
    let ct = if content_type.starts_with("text/") || content_type.contains("json") || content_type.contains("javascript") || content_type.contains("xml") {
        format!("{content_type}; charset=utf-8")
    } else {
        content_type.to_string()
    };
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, HeaderValue::from_str(&ct).unwrap())
        // Short cache so the PWA picks up shell updates without a manual
        // refresh dance. The service worker handles the longer-term
        // offline-cache story itself.
        .header(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"))
        .body(body.into())
        .expect("static response builder")
}

pub fn router(state: AppState) -> Router<AppState> {
    // Static routes. No auth — the login form is the gate.
    let statics: Router<AppState> = Router::new()
        .route("/admin", get(|| async { Redirect::to("/admin/") }))
        .route("/admin/", get(|| async { asset_response(INDEX_HTML, "text/html") }))
        .route("/admin/index.html", get(|| async { asset_response(INDEX_HTML, "text/html") }))
        .route("/admin/app.js", get(|| async { asset_response(APP_JS, "application/javascript") }))
        .route("/admin/app.css", get(|| async { asset_response(APP_CSS, "text/css") }))
        .route(
            "/admin/manifest.webmanifest",
            get(|| async { asset_response(MANIFEST, "application/manifest+json") }),
        )
        .route("/admin/sw.js", get(|| async { asset_response(SW_JS, "application/javascript") }))
        .route("/admin/icon.svg", get(|| async { asset_response(ICON_SVG, "image/svg+xml") }));

    // Authenticated JSON API.
    let api: Router<AppState> = Router::new()
        .route("/admin/api/stats", get(api_stats))
        .route("/admin/api/torrents", get(api_list_torrents))
        .route("/admin/api/torrents/{hash}", delete(api_delete_torrent))
        .route("/admin/api/torrents/{hash}/blacklist", post(api_blacklist_torrent))
        .route("/admin/api/blacklist", get(api_list_blacklist))
        .route("/admin/api/blacklist/{hash}", delete(api_unblacklist))
        .route("/admin/api/sync/dmm", post(api_sync_dmm))
        .route("/admin/api/sync/generic", post(api_sync_generic))
        .route("/admin/api/sync/imdb", post(api_sync_imdb))
        .layer(middleware::from_fn_with_state(state, require_api_key));

    statics.merge(api)
}

// ---------------------------------------------------------------------------
// JSON API handlers
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct StatsResponse {
    torrents: i64,
    imdb_files: i64,
    parsed_pages: i64,
    blacklisted: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    last_dmm_import: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    last_imdb_import: Option<serde_json::Value>,
}

/// Aggregate counts + last-import timestamps for the admin stats card.
async fn api_stats(State(state): State<AppState>) -> Result<Json<StatsResponse>, (StatusCode, String)> {
    let db = &state.db;

    let torrents = torrent_repo::count(db).await.unwrap_or(0);
    let imdb_files: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM "ImdbFiles""#)
        .fetch_one(db)
        .await
        .unwrap_or(0);
    let parsed_pages: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM "ParsedPages""#)
        .fetch_one(db)
        .await
        .unwrap_or(0);
    let blacklisted: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM "BlacklistedItems""#)
        .fetch_one(db)
        .await
        .unwrap_or(0);

    // ImportMetadata carries the last DMM + IMDb sync summaries as JSON.
    // Both entries are optional — fresh installs have nothing yet.
    let dmm_row = sqlx::query(r#"SELECT "Value" FROM "ImportMetadata" WHERE "Key" = 'DmmLastImport'"#)
        .fetch_optional(db)
        .await
        .ok()
        .flatten();
    let imdb_row = sqlx::query(r#"SELECT "Value" FROM "ImportMetadata" WHERE "Key" = 'ImdbLastImport'"#)
        .fetch_optional(db)
        .await
        .ok()
        .flatten();

    let last_dmm_import = dmm_row.and_then(|row| {
        row.try_get::<serde_json::Value, _>(0)
            .ok()
            .and_then(|v| v.get("occured_at").cloned().or_else(|| v.get("OccuredAt").cloned()))
            .and_then(|v| v.as_str().map(str::to_owned))
    });

    let last_imdb_import = imdb_row.and_then(|row| {
        row.try_get::<serde_json::Value, _>(0).ok().map(|v| {
            // Normalise the wrapper so the JS tab can read both the
            // lowercased and the PascalCased shapes the ingestor
            // historically wrote.
            let get = |snake: &str, pascal: &str| {
                v.get(snake).or_else(|| v.get(pascal)).cloned()
            };
            serde_json::json!({
                "occured_at": get("occured_at", "OccuredAt"),
                "entry_count": get("entry_count", "EntryCount"),
                "status": get("status", "Status"),
            })
        })
    });

    Ok(Json(StatsResponse {
        torrents,
        imdb_files,
        parsed_pages,
        blacklisted,
        last_dmm_import,
        last_imdb_import,
    }))
}

#[derive(Deserialize)]
struct ListTorrentsQuery {
    #[serde(default = "default_page")]
    page: i64,
    #[serde(default = "default_per_page")]
    per_page: i64,
    #[serde(default)]
    search: Option<String>,
}
fn default_page() -> i64 { 1 }
fn default_per_page() -> i64 { 50 }

#[derive(Serialize)]
struct ListTorrentsResponse {
    items: Vec<TorrentInfo>,
    total: i64,
}

async fn api_list_torrents(
    State(state): State<AppState>,
    Query(q): Query<ListTorrentsQuery>,
) -> Result<Json<ListTorrentsResponse>, (StatusCode, String)> {
    let per_page = q.per_page.clamp(1, 500);
    let page = q.page.max(1);
    let offset = (page - 1) * per_page;

    torrent_repo::admin_list(&state.db, q.search.as_deref(), per_page, offset)
        .await
        .map(|(items, total)| Json(ListTorrentsResponse { items, total }))
        .map_err(|err| {
            tracing::error!(?err, "admin: list torrents failed");
            (StatusCode::INTERNAL_SERVER_ERROR, err.to_string())
        })
}

async fn api_delete_torrent(
    State(state): State<AppState>,
    Path(hash): Path<String>,
) -> Result<StatusCode, (StatusCode, String)> {
    match torrent_repo::delete_by_hash(&state.db, &hash).await {
        Ok(true) => Ok(StatusCode::NO_CONTENT),
        Ok(false) => Err((StatusCode::NOT_FOUND, "not found".into())),
        Err(err) => Err((StatusCode::INTERNAL_SERVER_ERROR, err.to_string())),
    }
}

#[derive(Deserialize)]
struct BlacklistBody {
    reason: String,
}

async fn api_blacklist_torrent(
    State(state): State<AppState>,
    Path(hash): Path<String>,
    Json(body): Json<BlacklistBody>,
) -> Result<StatusCode, (StatusCode, String)> {
    if body.reason.trim().is_empty() {
        return Err((StatusCode::BAD_REQUEST, "reason is required".into()));
    }
    // If already blacklisted, return 409 to match the /blacklist/add
    // public endpoint's semantics.
    match blacklist_repo::exists(&state.db, &hash).await {
        Ok(true) => return Err((StatusCode::CONFLICT, "already blacklisted".into())),
        Ok(false) => {}
        Err(err) => return Err((StatusCode::INTERNAL_SERVER_ERROR, err.to_string())),
    }
    if let Err(err) = blacklist_repo::insert(&state.db, &hash, &body.reason).await {
        return Err((StatusCode::INTERNAL_SERVER_ERROR, err.to_string()));
    }
    // Cascade: drop the torrent from the main table so it can't be
    // served again until re-ingested.
    let _ = torrent_repo::delete_by_hash(&state.db, &hash).await;
    Ok(StatusCode::NO_CONTENT)
}

async fn api_list_blacklist(
    State(state): State<AppState>,
) -> Result<Json<Vec<BlacklistedItem>>, (StatusCode, String)> {
    blacklist_repo::list(&state.db)
        .await
        .map(Json)
        .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))
}

async fn api_unblacklist(
    State(state): State<AppState>,
    Path(hash): Path<String>,
) -> Result<StatusCode, (StatusCode, String)> {
    match blacklist_repo::remove(&state.db, &hash).await {
        Ok(true) => Ok(StatusCode::NO_CONTENT),
        Ok(false) => Err((StatusCode::NOT_FOUND, "not found".into())),
        Err(err) => Err((StatusCode::INTERNAL_SERVER_ERROR, err.to_string())),
    }
}

// --- sync triggers ---------------------------------------------------------
//
// All three handlers follow the same pattern: acquire the SyncJobs mutex
// `try_lock`-style (409 if busy), run the ingestion, release, return the
// result. The outer `/dmm/on-demand-scrape` handler uses the same mutex so
// admin UI clicks can't overlap a scheduled tick either.

async fn api_sync_dmm(State(state): State<AppState>) -> Result<String, (StatusCode, String)> {
    let guard = state
        .sync_mutex
        .try_lock()
        .map_err(|_| (StatusCode::CONFLICT, "DMM sync already running".to_string()))?;
    let result = crate::ingestion::dmm::run(
        state.config.clone(),
        state.db.clone(),
        state.searcher.clone(),
    )
    .await;
    drop(guard);
    match result {
        Ok(report) => Ok(format!(
            "parsed {} torrents, inserted {} rows",
            report.parsed, report.inserted
        )),
        Err(err) => Err((StatusCode::INTERNAL_SERVER_ERROR, err.to_string())),
    }
}

async fn api_sync_generic(State(state): State<AppState>) -> Result<String, (StatusCode, String)> {
    let guard = state.sync_mutex.try_lock().map_err(|_| {
        (StatusCode::CONFLICT, "generic sync already running".to_string())
    })?;
    let result = crate::run_generic_all(
        state.config.clone(),
        state.db.clone(),
        state.searcher.clone(),
    )
    .await;
    drop(guard);
    match result {
        Ok(()) => Ok("generic sync complete".into()),
        Err(err) => Err((StatusCode::INTERNAL_SERVER_ERROR, err.to_string())),
    }
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct ImdbSyncBody {
    force_download: bool,
    force_create_index: bool,
}

async fn api_sync_imdb(
    State(state): State<AppState>,
    Json(body): Json<ImdbSyncBody>,
) -> Result<String, (StatusCode, String)> {
    let guard = state
        .sync_mutex
        .try_lock()
        .map_err(|_| (StatusCode::CONFLICT, "IMDb re-index already running".to_string()))?;
    let result = crate::ingestion::imdb::run(
        state.config.clone(),
        state.db.clone(),
        state.searcher.clone(),
        crate::ingestion::imdb::ResyncOptions {
            force_download: body.force_download,
            force_create_index: body.force_create_index,
            retag_missing: false,
            retag_all: false,
        },
    )
    .await;
    drop(guard);
    match result {
        Ok(()) => Ok("IMDb re-index complete".into()),
        Err(err) => Err((StatusCode::INTERNAL_SERVER_ERROR, err.to_string())),
    }
}

