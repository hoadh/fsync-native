//! HTTP-only guards: global in-flight cap (503 when saturated) and a per-request deadline.
//! Not applied to the presence WebSocket, which is long-lived by design.

use crate::{error::ApiError, state::AppState};
use axum::{
    extract::{Request, State},
    middleware::Next,
    response::{IntoResponse, Response},
};
use std::time::Duration;

pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);

pub async fn guard(State(s): State<AppState>, req: Request, next: Next) -> Response {
    let Ok(_permit) = s.in_flight.clone().try_acquire_owned() else {
        return ApiError::Unavailable.into_response();
    };
    match tokio::time::timeout(REQUEST_TIMEOUT, next.run(req)).await {
        Ok(resp) => resp,
        Err(_) => (axum::http::StatusCode::REQUEST_TIMEOUT, "request timeout").into_response(),
    }
}
