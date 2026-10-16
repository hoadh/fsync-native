//! Map HTTP outcomes to `CoreError`.

use super::{limits::capped_json, reachability::is_unreachable};
use crate::error::{ApiError, CoreError, Result};
use fsync_proto::api_dto::ApiErrorBody;
use reqwest::Response;

/// Map transport errors and non-2xx bodies to `CoreError`.
pub(crate) async fn check(r: reqwest::Result<Response>) -> Result<Response> {
    let resp = r.map_err(|e| {
        let unreachable = is_unreachable(&e);
        let msg = e.without_url().to_string();
        if unreachable {
            CoreError::Unreachable(msg)
        } else {
            CoreError::Http(msg)
        }
    })?;
    if resp.status().is_success() {
        return Ok(resp);
    }
    let status = resp.status().as_u16();
    let body = capped_json::<ApiErrorBody>(resp).await.ok();
    Err(CoreError::Api(ApiError {
        status,
        code: body
            .as_ref()
            .map_or_else(|| "unknown".into(), |b| b.code.clone()),
        message: body.map(|b| b.message).unwrap_or_default(),
    }))
}
