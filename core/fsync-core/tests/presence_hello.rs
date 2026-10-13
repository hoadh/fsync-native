//! Presence client against a scripted WebSocket server: `Hello` waits (bounded) for the relay
//! URL, and goes out again at once when the addresses change.

mod common;

use fsync_core::{
    Identity,
    api::{ApiClient, PresenceEvent, spawn_presence},
    net::AddrFeed,
};
use fsync_proto::{
    api_dto::TokenPair,
    fixed_bytes::FixedBytes,
    presence::{Addrs, ClientMsg, ServerMsg},
};
use futures_util::{SinkExt, StreamExt};
use std::{sync::Arc, time::Duration};
use tokio::{
    net::TcpListener,
    sync::{mpsc, watch},
    time::Instant,
};
use tokio_tungstenite::{accept_async, tungstenite::Message};

/// Accepts one connection, runs the handshake, then forwards every client message.
async fn scripted_server() -> (String, mpsc::UnboundedReceiver<ClientMsg>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let (tx, rx) = mpsc::unbounded_channel();
    tokio::spawn(async move {
        let (sock, _) = listener.accept().await.unwrap();
        let mut ws = accept_async(sock).await.unwrap();
        let nonce = FixedBytes([7u8; 32]);
        let ch = serde_json::to_string(&ServerMsg::Challenge { nonce }).unwrap();
        ws.send(Message::text(ch)).await.unwrap();
        let _auth = ws.next().await;
        let peers = serde_json::to_string(&ServerMsg::Peers { peers: vec![] }).unwrap();
        ws.send(Message::text(peers)).await.unwrap();
        while let Some(Ok(m)) = ws.next().await {
            if let Message::Text(t) = m
                && let Ok(c) = serde_json::from_str::<ClientMsg>(t.as_str())
            {
                let _ = tx.send(c);
            }
        }
    });
    (base, rx)
}

fn api(base: &str) -> Arc<ApiClient> {
    let tokens = TokenPair {
        access: "a".into(),
        refresh: "r".into(),
    };
    Arc::new(ApiClient::with_options(base, Some(tokens), None, true).unwrap())
}

async fn next_hello(rx: &mut mpsc::UnboundedReceiver<ClientMsg>) -> Addrs {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let ClientMsg::Hello { addrs } = rx.recv().await.expect("server gone") {
                return addrs;
            }
        }
    })
    .await
    .expect("no hello")
}

fn addrs(relay: Option<&str>) -> Addrs {
    Addrs {
        relay_url: relay.map(str::to_owned),
        direct: vec![],
    }
}

#[tokio::test]
async fn hello_waits_for_the_relay_then_follows_address_changes() {
    let (base, mut seen) = scripted_server().await;
    let (addr_tx, addr_rx) = watch::channel(addrs(None));
    let feed = AddrFeed {
        rx: addr_rx,
        expect_relay: true,
    };
    let (tx, mut events) = mpsc::channel(16);
    let t0 = Instant::now();
    let _h = spawn_presence(api(&base), Identity::generate().unwrap(), feed, tx);

    // The relay comes up within the grace period: the first Hello already carries it.
    tokio::time::sleep(Duration::from_millis(100)).await;
    addr_tx.send_replace(addrs(Some("https://relay.example/")));
    let first = next_hello(&mut seen).await;
    assert_eq!(first.relay_url.as_deref(), Some("https://relay.example/"));
    assert!(
        t0.elapsed() < Duration::from_secs(3),
        "did not wait the full bound"
    );
    assert!(matches!(
        events.recv().await,
        Some(PresenceEvent::Connected)
    ));

    // A later change is sent promptly, not at the next 20 s heartbeat.
    let t1 = Instant::now();
    addr_tx.send_replace(addrs(Some("https://relay2.example/")));
    let second = next_hello(&mut seen).await;
    assert_eq!(second.relay_url.as_deref(), Some("https://relay2.example/"));
    assert!(t1.elapsed() < Duration::from_secs(2), "{:?}", t1.elapsed());
}

#[tokio::test]
async fn hello_goes_out_without_a_relay_after_the_bound() {
    let (base, mut seen) = scripted_server().await;
    let (_addr_tx, addr_rx) = watch::channel(addrs(None));
    let feed = AddrFeed {
        rx: addr_rx,
        expect_relay: true,
    };
    let (tx, _events) = mpsc::channel(16);
    let t0 = Instant::now();
    let _h = spawn_presence(api(&base), Identity::generate().unwrap(), feed, tx);
    let first = next_hello(&mut seen).await;
    assert_eq!(first.relay_url, None);
    let waited = t0.elapsed();
    assert!(
        waited >= Duration::from_millis(250) && waited < Duration::from_secs(2),
        "{waited:?}"
    );
}
