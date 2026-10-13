//! Device-list helpers for tests (built with fsync_proto, like a real client).

use super::{Session, TestApp};
use ed25519_dalek::SigningKey;
use fsync_proto::{DeviceList, NodeId, Platform, SignedDeviceList, api_dto::PutDeviceList, sign};
use uuid::Uuid;

pub struct Device {
    pub key: SigningKey,
    pub node: NodeId,
}

impl Device {
    pub fn new() -> Self {
        let mut seed = [0u8; 32];
        getrandom::fill(&mut seed).unwrap();
        let key = SigningKey::from_bytes(&seed);
        Self {
            node: NodeId::from_signing_key(&key),
            key,
        }
    }
}

pub fn genesis(dev: &Device, account: Uuid) -> SignedDeviceList {
    let l = DeviceList::genesis(account, dev.node, "mac".into(), Platform::Mac, 1);
    sign(&dev.key, &l).unwrap()
}

pub fn add(prev: &SignedDeviceList, signer: &Device, new: &Device) -> SignedDeviceList {
    let l =
        DeviceList::next_add(prev, signer.node, new.node, "win".into(), Platform::Win, 2).unwrap();
    sign(&signer.key, &l).unwrap()
}

impl TestApp {
    pub async fn put_list(
        &self,
        s: &Session,
        list: &SignedDeviceList,
        if_match: Option<&str>,
        reset: bool,
    ) -> reqwest::Response {
        let mut rb = self
            .http
            .put(self.url("/v1/device-list"))
            .bearer_auth(&s.access)
            .json(&PutDeviceList::new(list).unwrap());
        if let Some(m) = if_match {
            rb = rb.header("If-Match", m);
        }
        if reset {
            rb = rb.header("X-Fsync-Trust-Reset", "1");
        }
        rb.send().await.unwrap()
    }
}

impl TestApp {
    /// Current stored list of the account.
    pub async fn current_list(&self, s: &Session) -> SignedDeviceList {
        self.http
            .get(self.url("/v1/device-list"))
            .bearer_auth(&s.access)
            .send()
            .await
            .unwrap()
            .json::<fsync_proto::api_dto::DeviceListResponse>()
            .await
            .unwrap()
            .decode()
            .unwrap()
    }

    /// Account with a two-device list (genesis + one add); returns (session, d1, d2).
    pub async fn setup_two_devices(&self, email: &str) -> (Session, Device, Device) {
        let s = self.login(email).await;
        let (d1, d2) = (Device::new(), Device::new());
        let g = genesis(&d1, s.account_id);
        assert_eq!(self.put_list(&s, &g, None, false).await.status(), 200);
        let l1 = add(&g, &d1, &d2);
        assert_eq!(self.put_list(&s, &l1, Some("0"), false).await.status(), 200);
        (s, d1, d2)
    }

    /// A session whose access token claims an OTP login `age_s` seconds ago.
    pub fn stale_session(&self, s: &Session, age_s: i64) -> Session {
        let now = fsync_backend::util::now_ms() / 1000;
        // Same (live) session family as `s`, only the claimed login time differs.
        let sid = self.state.tokens.verify_access(&s.access).unwrap().sid;
        let access = self
            .state
            .tokens
            .issue_with(s.account_id, sid, now - age_s, now, now + 900)
            .unwrap();
        Session {
            access,
            refresh: s.refresh.clone(),
            account_id: s.account_id,
        }
    }
}
