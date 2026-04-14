//! `/dmm/*` endpoints. Mirrors the .NET `SearchEndpoints` class.
//!
//! `POST /dmm/search`              — unfiltered pg_trgm search on ParsedTitle.
//! `GET  /dmm/filtered`            — `search_torrents_meta`-backed filtered search.
//! `GET  /dmm/on-demand-scrape`    — triggers DMM sync. Stubbed out in
//!                                   Phase 2 because the in-process
//!                                   scheduler lands in Phase 3.

use axum::Router;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::middleware;
use axum::response::{IntoResponse, Json};
use axum::routing::{get, post};

use crate::db::torrent as repo;
use crate::domain::torrent::{
    DmmQueryRequest, SearchFilteredRequest, TorrentInfo, TorrentInfoFilter,
};
use crate::http::auth::require_api_key;
use crate::http::state::AppState;
use crate::utils::query::clean_query;

pub fn router(state: AppState) -> Router<AppState> {
    // Only /dmm/on-demand-scrape requires the API key; it shares the router
    // with the anonymous /dmm/search and /dmm/filtered endpoints. Putting
    // the three routes on the same Router lets axum infer `Router<AppState>`
    // from the two handlers that do extract `State<AppState>`, and the
    // middleware layer is attached only to the single protected route.
    let auth = middleware::from_fn_with_state(state, require_api_key);
    Router::new()
        .route("/dmm/search", post(dmm_search))
        .route("/dmm/filtered", get(dmm_filtered))
        .route("/dmm/on-demand-scrape", get(on_demand_scrape).layer(auth))
}

async fn dmm_search(
    State(state): State<AppState>,
    Json(body): Json<DmmQueryRequest>,
) -> Json<Vec<TorrentInfo>> {
    if body.query_text.trim().is_empty() {
        return Json(Vec::new());
    }

    let cleaned = clean_query(&body.query_text);
    match repo::search_by_title(&state.db, &cleaned).await {
        Ok(rows) => Json(rows),
        Err(err) => {
            // Match the .NET swallow-and-return-empty behaviour so mis-formed
            // queries surface as "no results" rather than 500s.
            tracing::error!(?err, query = %body.query_text, "unfiltered search failed");
            Json(Vec::new())
        }
    }
}

async fn dmm_filtered(
    State(state): State<AppState>,
    Query(req): Query<SearchFilteredRequest>,
) -> Json<Vec<TorrentInfo>> {
    let filter = TorrentInfoFilter {
        query: req.query.as_ref().map(|q| clean_query(q)),
        season: req.season,
        episode: req.episode,
        year: req.year,
        language: req.language,
        resolution: req.resolution,
        imdb_id: req.imdb_id,
        category: req.category,
    };
    let limit = state.config.dmm_max_filtered_results;
    let threshold = state.config.dmm_minimum_score;

    match repo::search_filtered(&state.db, &filter, limit, threshold).await {
        Ok(rows) => Json(rows),
        Err(err) => {
            tracing::error!(?err, "filtered search failed");
            Json(Vec::new())
        }
    }
}

async fn on_demand_scrape(State(state): State<AppState>) -> axum::response::Response {
    // Acquire the shared "SyncJobs" mutex so we don't overlap a scheduled
    // tick. `try_lock` returns immediately; if the mutex is held we reply
    // 409 Conflict, matching the .NET `SyncOnDemandState.IsRunning` guard
    // which returned early when another scrape was already in flight.
    let guard = match state.sync_mutex.try_lock() {
        Ok(g) => g,
        Err(_) => {
            return (
                StatusCode::CONFLICT,
                "on-demand scrape already running",
            )
                .into_response();
        }
    };

    // The endpoint returns once the sync completes, mirroring the .NET
    // handler. For long-running scrapes callers should use the regular
    // scheduled cadence.
    let result = crate::ingestion::dmm::run(
        state.config.clone(),
        state.db.clone(),
        state.searcher.clone(),
    )
    .await;
    drop(guard);

    match result {
        Ok(report) => (
            StatusCode::NO_CONTENT,
            format!(
                "DMM sync completed: parsed {}, inserted {}",
                report.parsed, report.inserted
            ),
        )
            .into_response(),
        Err(err) => {
            tracing::error!(?err, "on-demand DMM sync failed");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("DMM sync failed: {err}"),
            )
                .into_response()
        }
    }
}
