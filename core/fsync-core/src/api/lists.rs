//! Endpoint wrappers: login, device list, trust reset, chain, pairing codes.

use super::client::{ApiClient, check};
use crate::error::{CoreError, Result};
use fsync_proto::{
    NodeId, SignedDeviceList,
    api_dto::{
        ChainResponse, DeviceListResponse, OtpStart, OtpVerify, OtpVerifyResponse, PairingCreate,
        PairingCreated, PairingLookup, PutDeviceList, Refresh, decode_signed,
    },
};
use uuid::Uuid;

const CHAIN_PAGE: usize = 100;
const MAX_PAGES: usize = 100;

impl ApiClient {
    pub async fn otp_start(&self, email: &str) -> Result<()> {
        let body = OtpStart {
            email: email.into(),
        };
        check(
            self.plain()
                .post(self.url("auth/otp/start"))
                .json(&body)
                .send()
                .await,
        )
        .await?;
        Ok(())
    }

    /// Verifies the code, stores the tokens and returns the account id.
    pub async fn otp_verify(&self, email: &str, code: &str) -> Result<Uuid> {
        let body = OtpVerify {
            email: email.into(),
            code: code.into(),
        };
        let r = check(
            self.plain()
                .post(self.url("auth/otp/verify"))
                .json(&body)
                .send()
                .await,
        )
        .await?;
        let v: OtpVerifyResponse = super::limits::capped_json(r).await?;
        self.set_tokens(v.tokens);
        Ok(v.account_id)
    }

    /// Revoke `refresh`'s whole session server-side. A 401 means it is already dead: success.
    pub async fn revoke(&self, refresh: &str) -> Result<()> {
        let r = self
            .plain()
            .post(self.url("auth/logout"))
            .json(&Refresh {
                refresh: refresh.into(),
            })
            .send()
            .await;
        match check(r).await {
            Ok(_) => Ok(()),
            Err(CoreError::Api(e)) if e.status == 401 => Ok(()),
            Err(e) => Err(e),
        }
    }

    /// `None` if the account has no list yet (404).
    pub async fn get_device_list(&self) -> Result<Option<SignedDeviceList>> {
        match self
            .authed_json::<DeviceListResponse>(|c, t| c.get(self.url("device-list")).bearer_auth(t))
            .await
        {
            Ok(r) => Ok(Some(r.decode()?)),
            Err(CoreError::Api(e)) if e.status == 404 => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// PUT with `If-Match: <prev_seq>` (`None` = genesis). 409 -> `CoreError::Api` (status 409).
    pub async fn put_device_list(
        &self,
        signed: &SignedDeviceList,
        if_match: Option<u64>,
    ) -> Result<SignedDeviceList> {
        let body = PutDeviceList::new(signed)?;
        let r: DeviceListResponse = self
            .authed_json(|c, t| {
                let rb = c.put(self.url("device-list")).bearer_auth(t).json(&body);
                match if_match {
                    Some(s) => rb.header("If-Match", s.to_string()),
                    None => rb,
                }
            })
            .await?;
        Ok(r.decode()?)
    }

    /// Trust reset with a fresh genesis. Needs fresh auth: `reauth_required` -> run OTP again.
    pub async fn put_trust_reset(&self, genesis: &SignedDeviceList) -> Result<SignedDeviceList> {
        let body = PutDeviceList::new(genesis)?;
        let r: DeviceListResponse = self
            .authed_json(|c, t| {
                c.put(self.url("device-list"))
                    .bearer_auth(t)
                    .header("X-Fsync-Trust-Reset", "1")
                    .json(&body)
            })
            .await?;
        Ok(r.decode()?)
    }

    /// One page (<= 100) of the current-epoch chain, `seq >= from_seq`.
    pub async fn chain(&self, from_seq: u64) -> Result<Vec<SignedDeviceList>> {
        let r: ChainResponse = self
            .authed_json(|c, t| {
                c.get(self.url("device-list/chain"))
                    .query(&[("from_seq", from_seq)])
                    .bearer_auth(t)
            })
            .await?;
        Ok(r.chain
            .iter()
            .map(|s| decode_signed(s))
            .collect::<std::result::Result<_, _>>()?)
    }

    /// Every list with `seq >= from_seq`, following pages.
    pub async fn chain_all(&self, from_seq: u64) -> Result<Vec<SignedDeviceList>> {
        let (mut out, mut from) = (Vec::new(), from_seq);
        for _ in 0..MAX_PAGES {
            let page = self.chain(from).await?;
            let Some(last) = page.last() else { break };
            from = last.decode()?.seq.saturating_add(1);
            let full = page.len() >= CHAIN_PAGE;
            out.extend(page);
            if !full {
                break;
            }
        }
        Ok(out)
    }

    pub async fn create_pairing(
        &self,
        node_id: NodeId,
        relay_url: Option<String>,
    ) -> Result<PairingCreated> {
        let body = PairingCreate { node_id, relay_url };
        self.authed_json(|c, t| c.post(self.url("pairing")).bearer_auth(t).json(&body))
            .await
    }

    pub async fn lookup_pairing(&self, code: &str) -> Result<PairingLookup> {
        if !fsync_proto::api_dto::is_valid_pairing_code(code) {
            return Err(CoreError::Invalid("pairing code must be 6 digits"));
        }
        self.authed_json(|c, t| c.get(self.url(&format!("pairing/{code}"))).bearer_auth(t))
            .await
    }
}
