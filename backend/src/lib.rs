//! fsync backend: email-OTP auth, signed device-list registry, pairing codes, presence.

pub mod auth;
pub mod client_ip;
pub mod config;
pub mod db;
pub mod devices;
pub mod email;
pub mod error;
pub mod limits;
pub mod pairing;
pub mod presence;
pub mod rate_limit;
pub mod serve;
pub mod state;
pub mod util;

use axum::{Router, extract::DefaultBodyLimit, extract::State, middleware, routing::get};
use error::ApiResult;
use state::AppState;

async fn health(State(s): State<AppState>) -> ApiResult<&'static str> {
    sqlx::query("SELECT 1").execute(&s.db).await?;
    Ok("ok")
}

/// Build the full router. Serve with `into_make_service_with_connect_info::<SocketAddr>()`.
pub fn build_router(state: AppState) -> Router {
    // HTTP routes get the in-flight cap and request deadline; the presence socket and /health
    // (so orchestrator probes still work while saturated) do not.
    let http = Router::new()
        .nest(
            "/v1",
            Router::new()
                .merge(auth::routes::router())
                .merge(devices::routes::router())
                .merge(pairing::routes::router()),
        )
        .layer(middleware::from_fn_with_state(state.clone(), limits::guard));
    Router::new()
        .route("/health", get(health))
        .merge(http)
        .nest("/v1", presence::ws::router())
        .layer(DefaultBodyLimit::max(64 * 1024))
        .with_state(state)
}
