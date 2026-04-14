//! `/imdb/search` — Tantivy/trigram IMDb title search.

use axum::Router;
use axum::extract::{Query, State};
use axum::response::Json;
use axum::routing::post;

use crate::db::imdb_file as repo;
use crate::domain::imdb::{ImdbFilteredRequest, ImdbSearchResult};
use crate::http::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/imdb/search", post(search))
}

/// `POST /imdb/search` — the .NET version is POST with parameters bound
/// from the query string. We preserve that shape so existing callers
/// continue to work without changes.
async fn search(
    State(state): State<AppState>,
    Query(req): Query<ImdbFilteredRequest>,
) -> Json<Vec<ImdbSearchResult>> {
    let Some(query) = req.query.as_deref() else {
        return Json(Vec::new());
    };
    if query.is_empty() {
        return Json(Vec::new());
    }

    let limit = 10; // matches the .NET IImdbFileService.SearchForImdbIdAsync default.
    match repo::search(
        &state.db,
        query,
        req.year,
        req.category.as_deref(),
        limit,
        state.config.dmm_minimum_score,
    )
    .await
    {
        Ok(rows) => Json(rows),
        Err(err) => {
            tracing::error!(?err, ?req, "imdb search failed");
            Json(Vec::new())
        }
    }
}
