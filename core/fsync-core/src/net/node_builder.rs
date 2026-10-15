//! `NodeBuilder`: iroh endpoint with custom relay only + gated protocol router.

use super::{
    discovery::Discovery,
    gate::{Gate, Gated},
    lan_first_path_selector::LanFirstPathSelector,
    node::Node,
    node_addrs::spawn_feed,
    pair_handler::PairHandler,
    path_state::PathTracker,
    relays::{parse_relays, relay_map},
};
use crate::{
    error::{CoreError, Result},
    events::EventSink,
    identity::Identity,
    pair::PairingWindows,
    trust::TrustManager,
};
use fsync_proto::ALPN_PAIR;
use iroh::{
    Endpoint, RelayMode,
    address_lookup::MemoryLookup,
    endpoint::presets,
    protocol::{ProtocolHandler, Router, RouterBuilder},
};
use iroh_mdns_address_lookup::MdnsAddressLookup;
use std::sync::Arc;

type Extra = Box<dyn FnOnce(RouterBuilder, &Arc<Gate>) -> RouterBuilder + Send>;

pub struct NodeBuilder {
    pub(crate) identity: Identity,
    pub(crate) trust: Arc<TrustManager>,
    pub(crate) events: EventSink,
    pub(super) relays: Vec<String>,
    pub(super) mdns: bool,
    pub(super) relay_only: bool,
    pub(super) extra: Vec<Extra>,
}

impl NodeBuilder {
    /// Our relay(s). Empty = no relay (LAN / tests). Never falls back to n0 defaults.
    pub fn relays(mut self, urls: Vec<String>) -> Self {
        self.relays = urls;
        self
    }
    pub fn mdns(mut self, on: bool) -> Self {
        self.mdns = on;
        self
    }
    /// Test flag: drop all IP transports so every path goes through the relay.
    pub fn relay_only(mut self, on: bool) -> Self {
        self.relay_only = on;
        self
    }
    /// Register a protocol (clip/file in phase 07); it is wrapped by the accept gate.
    pub fn protocol<H: ProtocolHandler>(mut self, alpn: &'static [u8], handler: H) -> Self {
        self.extra.push(Box::new(move |rb, gate| {
            rb.accept(alpn, Gated::new(Arc::clone(gate), handler))
        }));
        self
    }

    pub async fn spawn(self) -> Result<Node> {
        // Scheme policy is the service's (it knows dev mode); here it is parse + dedup only.
        let relays = parse_relays(&self.relays, true)?;
        if self.relay_only && relays.is_empty() {
            return Err(CoreError::Invalid("relay_only needs a relay url"));
        }
        let expect_relay = !relays.is_empty();
        let mode = if relays.is_empty() {
            RelayMode::Disabled
        } else {
            RelayMode::Custom(relay_map(&relays))
        };
        let lookup = MemoryLookup::new();
        let mut eb = Endpoint::builder(presets::Minimal)
            .secret_key(self.identity.iroh_secret())
            .relay_mode(mode)
            .address_lookup(lookup.clone())
            .path_selector(Arc::new(LanFirstPathSelector::default()));
        if self.relay_only {
            eb = eb.clear_ip_transports();
        }
        let ep = eb.bind().await.map_err(CoreError::net)?;
        if self.mdns && !self.relay_only {
            let mdns = MdnsAddressLookup::builder()
                .advertise(true)
                .service_name("fsync")
                .build(ep.id())
                .map_err(CoreError::net)?;
            ep.address_lookup().map_err(CoreError::net)?.add(mdns);
        }
        self.trust.set_self(self.identity.node_id())?;
        let windows = Arc::new(PairingWindows::default());
        let paths = PathTracker::new(self.events.clone());
        let gate = Gate::new(self.trust.clone(), windows.clone(), paths.clone());
        let pair = PairHandler {
            windows: windows.clone(),
            me: self.identity.clone(),
            trust: self.trust.clone(),
            events: self.events.clone(),
        };
        let mut rb = Router::builder(ep.clone()).accept(ALPN_PAIR, Gated::new(gate.clone(), pair));
        for add in self.extra {
            rb = add(rb, &gate);
        }
        let router = rb.spawn();
        let mut changes = self.trust.subscribe();
        let g = gate.clone();
        let enforcer = tokio::spawn(async move {
            while changes.changed().await.is_ok() {
                g.enforce();
            }
        });
        let (feed, feeder) = spawn_feed(&ep, expect_relay);
        Ok(Node {
            discovery: Discovery::new(lookup, self.trust.clone()),
            identity: self.identity,
            trust: self.trust,
            events: self.events,
            ep,
            router,
            windows,
            gate,
            paths,
            enforcer,
            feed,
            feeder,
        })
    }
}
