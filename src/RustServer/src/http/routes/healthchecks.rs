//! `GET /healthchecks/ping` — liveness probe. Matches the .NET
//! [HealthCheckEndpoints] response format verbatim, including the leading
//! bracketed timestamp.
//!
//! [HealthCheckEndpoints]: https://github.com/iPromKnight/zilean/blob/main/src/Zilean.ApiService/Features/HealthChecks/HealthCheckEndpoints.cs

use axum::Router;
use axum::routing::get;

use crate::http::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/healthchecks/ping", get(ping))
}

async fn ping() -> String {
    // The .NET original formats the timestamp with `CultureInfo.InvariantCulture`:
    //   `DateTime.UtcNow.ToString(CultureInfo.InvariantCulture)`
    // which produces e.g. `04/14/2026 10:20:30`. chrono's `%m/%d/%Y %H:%M:%S`
    // reproduces the same layout.
    format!("[{}]: Pong!", chrono::Utc::now().format("%m/%d/%Y %H:%M:%S"))
}
