//! `/blacklist/*` endpoints. All protected; callers must supply the
//! `X-API-KEY` header.

use axum::Json;
use axum::Router;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::middleware;
use axum::routing::{delete, put};

use crate::db::{blacklist as repo, torrent as torrent_repo};
use crate::domain::blacklist::BlacklistItemRequest;
use crate::domain::torrents_api::BlacklistRemoveRequest;
use crate::http::auth::require_api_key;
use crate::http::state::AppState;

pub fn router(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/blacklist/add", put(add))
        .route("/blacklist/remove", delete(remove))
        .layer(middleware::from_fn_with_state(state, require_api_key))
}

/// The .NET handler accepts `[AsParameters]` binding, which reads from
/// query/form/body heuristically. The axum counterpart accepts either a
/// JSON body or form-encoded body; callers that previously sent it as a
/// query string keep working via the axum `Query` fallback if they use
/// method PUT with a query string.
///
/// If none of the three decoding strategies land a value we fall back to
/// the "field missing" branch, which mirrors the original 400 response.
async fn add(
    State(state): State<AppState>,
    Query(query): Query<BlacklistItemRequest>,
    // The inner resolver picks the first body decoding that works (JSON,
    // then form). A `null`-ish body leaves `body_request` None.
    body_request: Option<Json<BlacklistItemRequest>>,
) -> Result<StatusCode, (StatusCode, String)> {
    let req = body_request.map(|Json(j)| j).unwrap_or(query);

    if req.info_hash.trim().is_empty() {
        return Err((StatusCode::BAD_REQUEST, "info_hash is required".to_string()));
    }
    if req.reason.trim().is_empty() {
        return Err((StatusCode::BAD_REQUEST, "reason is required".to_string()));
    }

    match repo::exists(&state.db, &req.info_hash).await {
        Ok(true) => {
            return Err((StatusCode::CONFLICT, "Item already blacklisted".to_string()));
        }
        Ok(false) => {}
        Err(err) => {
            tracing::error!(?err, "checking blacklist membership");
            return Err((
                StatusCode::BAD_REQUEST,
                "An error occurred while adding a blacklisted item".to_string(),
            ));
        }
    }

    if let Err(err) = repo::insert(&state.db, &req.info_hash, &req.reason).await {
        tracing::error!(?err, "inserting into blacklist");
        return Err((
            StatusCode::BAD_REQUEST,
            "An error occurred while adding a blacklisted item".to_string(),
        ));
    }

    // Cascade: remove the torrent from the main table so it cannot be
    // served again. Mirrors the .NET handler.
    match torrent_repo::delete_by_hash(&state.db, &req.info_hash).await {
        Ok(true) => tracing::info!(%req.info_hash, "removed blacklisted torrent from Torrents"),
        Ok(false) => {}
        Err(err) => tracing::warn!(?err, "failed to delete torrent after blacklisting"),
    }

    Ok(StatusCode::NO_CONTENT)
}

async fn remove(
    State(state): State<AppState>,
    Query(query): Query<BlacklistRemoveRequest>,
) -> Result<StatusCode, (StatusCode, String)> {
    let Some(info_hash) = query.info_hash.filter(|s| !s.trim().is_empty()) else {
        return Err((StatusCode::BAD_REQUEST, "InfoHash is required".to_string()));
    };

    match repo::remove(&state.db, &info_hash).await {
        Ok(true) => Ok(StatusCode::NO_CONTENT),
        Ok(false) => Err((StatusCode::NOT_FOUND, String::new())),
        Err(err) => {
            tracing::error!(?err, %info_hash, "blacklist remove failed");
            Err((
                StatusCode::BAD_REQUEST,
                "An error occurred while removing a blacklisted item".to_string(),
            ))
        }
    }
}

