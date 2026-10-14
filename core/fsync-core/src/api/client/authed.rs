//! Authenticated requests with refresh-on-401.

use super::{ApiClient, check};
use crate::api::limits::capped_json;
use crate::error::{CoreError, Result};
use reqwest::{Client, RequestBuilder, Response};
use serde::de::DeserializeOwned;

impl ApiClient {
    /// Authenticated request; on 401 (not `reauth_required`) refresh once and retry.
    pub(crate) async fn authed(
        &self,
        build: impl Fn(&Client, &str) -> RequestBuilder,
    ) -> Result<Response> {
        let access = self.access_token()?;
        match check(build(&self.http, &access).send().await).await {
            Err(CoreError::Api(e)) if e.status == 401 && !e.is("reauth_required") => {
                self.refresh_after(&access).await?;
                let access = self.access_token()?;
                check(build(&self.http, &access).send().await).await
            }
            other => other,
        }
    }

    pub(crate) async fn authed_json<T: DeserializeOwned>(
        &self,
        build: impl Fn(&Client, &str) -> RequestBuilder,
    ) -> Result<T> {
        capped_json(self.authed(build).await?).await
    }
}
