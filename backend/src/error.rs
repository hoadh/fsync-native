//! API error type mapped to the JSON `ApiErrorBody`.

use axum::{
    Json,
    http::{HeaderValue, StatusCode, header::RETRY_AFTER},
    response::{IntoResponse, Response},
};
use fsync_proto::api_dto::ApiErrorBody;

#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("bad request: {0}")]
    BadRequest(String),
    #[error("unauthorized")]
    Unauthorized,
    #[error("reauth required")]
    ReauthRequired,
    #[error("not found")]
    NotFound,
    #[error("conflict: {0}")]
    Conflict(String),
    #[error("conflict ({code}): {message}")]
    ConflictCode { code: &'static str, message: String },
    #[error("service unavailable")]
    Unavailable,
    #[error("unprocessable ({code}): {message}")]
    Unprocessable { code: &'static str, message: String },
    #[error("rate limited")]
    RateLimited { retry_after_s: u64 },
    #[error("internal: {0}")]
    Internal(String),
}

pub type ApiResult<T> = Result<T, ApiError>;

/// Error category only: sqlx messages can embed row data (constraint values, bound params).
pub fn sqlx_kind(e: &sqlx::Error) -> String {
    match e {
        sqlx::Error::Database(d) => format!("database(sqlstate={})", d.code().unwrap_or_default()),
        sqlx::Error::PoolTimedOut => "pool_timed_out".into(),
        sqlx::Error::PoolClosed => "pool_closed".into(),
        sqlx::Error::Io(_) => "io".into(),
        sqlx::Error::Tls(_) => "tls".into(),
        sqlx::Error::Protocol(_) => "protocol".into(),
        sqlx::Error::RowNotFound => "row_not_found".into(),
        sqlx::Error::ColumnDecode { .. } | sqlx::Error::Decode(_) => "decode".into(),
        _ => "other".into(),
    }
}

impl From<sqlx::Error> for ApiError {
    fn from(e: sqlx::Error) -> Self {
        Self::Internal(format!("database: {}", sqlx_kind(&e)))
    }
}

impl ApiError {
    pub fn internal(e: impl std::fmt::Display) -> Self {
        Self::Internal(e.to_string())
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, code, message) = match &self {
            Self::BadRequest(m) => (StatusCode::BAD_REQUEST, "bad_request", m.clone()),
            Self::Unauthorized => (
                StatusCode::UNAUTHORIZED,
                "unauthorized",
                "unauthorized".into(),
            ),
            Self::ReauthRequired => (
                StatusCode::UNAUTHORIZED,
                "reauth_required",
                "recent login required".into(),
            ),
            Self::NotFound => (StatusCode::NOT_FOUND, "not_found", "not found".into()),
            Self::Conflict(m) => (StatusCode::CONFLICT, "conflict", m.clone()),
            Self::ConflictCode { code, message } => (StatusCode::CONFLICT, *code, message.clone()),
            Self::Unavailable => (
                StatusCode::SERVICE_UNAVAILABLE,
                "unavailable",
                "server busy".into(),
            ),
            Self::Unprocessable { code, message } => {
                (StatusCode::UNPROCESSABLE_ENTITY, *code, message.clone())
            }
            Self::RateLimited { .. } => (
                StatusCode::TOO_MANY_REQUESTS,
                "rate_limited",
                "too many requests".into(),
            ),
            Self::Internal(detail) => {
                tracing::error!(error = %detail, "internal error");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal",
                    "internal error".into(),
                )
            }
        };
        let body = Json(ApiErrorBody {
            code: code.into(),
            message,
        });
        let mut resp = (status, body).into_response();
        if let Self::RateLimited { retry_after_s } = self
            && let Ok(v) = HeaderValue::from_str(&retry_after_s.to_string())
        {
            resp.headers_mut().insert(RETRY_AFTER, v);
        }
        resp
    }
}
