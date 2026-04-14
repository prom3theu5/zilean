//! API-key authentication middleware.
//!
//! Matches the .NET `ApiKeyAuthenticationHandler`:
//!
//! * Header name: `X-API-KEY` (case-insensitive; axum normalises headers
//!   at the HTTP layer).
//! * Policy: the request must carry a value that exactly equals the
//!   `ApiKey` config property.
//! * A blank or unset server-side `ApiKey` makes every protected endpoint
//!   reject with 401 unconditionally. This mirrors the "no key configured
//!   == no access" behaviour of the original handler, and it's the reason
//!   the .NET app auto-generates an ApiKey at first run.

use axum::extract::{Request, State};
use axum::http::{HeaderName, StatusCode};
use axum::middleware::Next;
use axum::response::Response;

use super::state::AppState;

pub const API_KEY_HEADER: HeaderName = HeaderName::from_static("x-api-key");

/// axum middleware enforcing the `X-API-KEY` header check. Mount via
/// `Router::layer(middleware::from_fn_with_state(state, require_api_key))`.
pub async fn require_api_key(
    State(state): State<AppState>,
    req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let Some(configured) = state.config.api_key.as_deref().filter(|k| !k.is_empty())
    else {
        // No server-side key configured; refuse every authenticated request.
        tracing::warn!(
            "Request to a protected endpoint rejected because ZILEAN_API_KEY \
             is not configured"
        );
        return Err(StatusCode::UNAUTHORIZED);
    };

    let Some(header_value) = req
        .headers()
        .get(&API_KEY_HEADER)
        .and_then(|v| v.to_str().ok())
    else {
        return Err(StatusCode::UNAUTHORIZED);
    };

    if header_value != configured {
        return Err(StatusCode::UNAUTHORIZED);
    }

    Ok(next.run(req).await)
}
