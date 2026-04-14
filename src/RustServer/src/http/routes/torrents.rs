//! `/torrents/*` endpoints. Both subroutes are protected.
//!
//! * `GET /torrents/all`          — streams every row in `Torrents` as a
//!                                  single JSON array.
//! * `GET /torrents/checkcached`  — looks up a batch of hashes and reports
//!                                  which are present.

use std::collections::HashSet;

use axum::Router;
use axum::body::Body;
use axum::extract::{Query, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::middleware;
use axum::response::{IntoResponse, Json, Response};
use axum::routing::get;
use bytes::Bytes;
use futures::StreamExt;

use crate::db::torrent as repo;
use crate::domain::torrents_api::{CachedItem, CheckCachedRequest, ErrorResponse};
use crate::http::auth::require_api_key;
use crate::http::state::AppState;

pub fn router(state: AppState) -> Router<AppState> {
    let mut router = Router::new();

    if state.config.torrents_scrape_enabled {
        router = router.route("/torrents/all", get(stream_all));
    }
    if state.config.torrents_cache_check_enabled {
        router = router.route("/torrents/checkcached", get(check_cached));
    }

    router.layer(middleware::from_fn_with_state(state, require_api_key))
}

async fn stream_all(State(state): State<AppState>) -> Response {
    // The .NET implementation manually emits `[`, then each item comma-
    // separated, then `]`. We replicate the exact wire format so client
    // JSON parsers see a single well-formed array rather than NDJSON.
    let db = state.db.clone();

    let stream = async_stream::stream! {
        yield Ok::<Bytes, std::io::Error>(Bytes::from_static(b"["));
        let mut rows = Box::pin(repo::stream_all(&db));
        let mut first = true;
        while let Some(result) = rows.next().await {
            match result {
                Ok(entry) => {
                    if !first {
                        yield Ok(Bytes::from_static(b","));
                    }
                    first = false;
                    match serde_json::to_vec(&entry) {
                        Ok(bytes) => yield Ok(Bytes::from(bytes)),
                        Err(err) => {
                            tracing::error!(?err, "serialising StreamedEntry");
                            // Fall through; we still close the array.
                        }
                    }
                }
                Err(err) => {
                    tracing::error!(?err, "streaming Torrents row");
                    // Don't break the array mid-flight; just stop.
                    break;
                }
            }
        }
        yield Ok(Bytes::from_static(b"]"));
    };

    Response::builder()
        .status(StatusCode::OK)
        .header(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        )
        .body(Body::from_stream(stream))
        .expect("static headers always build")
}

async fn check_cached(
    State(state): State<AppState>,
    Query(req): Query<CheckCachedRequest>,
) -> Result<Json<Vec<CachedItem>>, (StatusCode, Json<ErrorResponse>)> {
    let Some(raw) = req.hashes.as_deref().filter(|s| !s.trim().is_empty()) else {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new("No hashes provided")),
        ));
    };

    let hashes: Vec<String> = raw.split(',').map(|s| s.trim().to_string()).collect();
    let limit = state.config.torrents_max_hashes_to_check;
    // .NET uses `>=` for the too-many check (one less than the advertised
    // limit is accepted). We match that comparison exactly.
    if hashes.len() >= limit {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new(format!(
                "Too many hashes provided. The limit is {limit}."
            ))),
        ));
    }

    let rows = match repo::find_by_hashes(&state.db, &hashes).await {
        Ok(rows) => rows,
        Err(err) => {
            tracing::error!(?err, "checkcached lookup failed");
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(err.to_string())),
            ));
        }
    };

    let mut items: Vec<CachedItem> = rows
        .into_iter()
        .map(|row| CachedItem {
            info_hash: row.info_hash.clone(),
            is_cached: true,
            item: Some(row),
        })
        .collect();

    let present: HashSet<String> = items
        .iter()
        .map(|i| i.info_hash.to_ascii_lowercase())
        .collect();
    for hash in hashes {
        if !present.contains(&hash.to_ascii_lowercase()) {
            items.push(CachedItem {
                info_hash: hash,
                is_cached: false,
                item: None,
            });
        }
    }

    Ok(Json(items))
}

impl IntoResponse for ErrorResponse {
    fn into_response(self) -> Response {
        Json(self).into_response()
    }
}
