//! Live view of our own addresses (relay URL + direct IPs), fed by iroh's address watcher.

use fsync_proto::presence::Addrs;
use iroh::{Endpoint, EndpointAddr};
use n0_watcher::Watcher;
use tokio::{sync::watch, task::JoinHandle};

/// Receiver of our current addresses; presence re-sends `Hello` whenever it changes.
#[derive(Clone, Debug)]
pub struct AddrFeed {
    pub rx: watch::Receiver<Addrs>,
    /// A relay is configured, so `Hello` should wait (bounded) until its URL is known.
    pub expect_relay: bool,
}

pub(super) fn addrs_of(a: &EndpointAddr) -> Addrs {
    let mut direct: Vec<_> = a.ip_addrs().copied().collect();
    direct.sort_unstable();
    Addrs {
        relay_url: a.relay_urls().next().map(|u| u.to_string()),
        direct,
    }
}

/// Forwards `Endpoint::watch_addr` into a `watch` channel; ends when the endpoint is dropped.
pub(super) fn spawn_feed(ep: &Endpoint, expect_relay: bool) -> (AddrFeed, JoinHandle<()>) {
    let mut w = ep.watch_addr();
    let (tx, rx) = watch::channel(addrs_of(&w.get()));
    let task = tokio::spawn(async move {
        while let Ok(a) = w.updated().await {
            if tx.send(addrs_of(&a)).is_err() {
                break;
            }
        }
    });
    (AddrFeed { rx, expect_relay }, task)
}
