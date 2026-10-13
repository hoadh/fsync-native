mod common;

use common::{
    TestApp,
    devices::Device,
    ws::{recv, send},
};
use fsync_proto::{
    api_dto::PairingCreated,
    presence::{ClientMsg, ServerMsg, sign_challenge},
};
use serde_json::json;

#[tokio::test]
async fn rejects_unlisted_node_bad_sig_bad_token() {
    let app = TestApp::start().await;
    let (s, d1, _d2) = app.setup_two_devices("a@example.com").await;

    // Unlisted node with a valid signature.
    let stranger = Device::new();
    let mut ws = app.ws_auth(&s, &stranger).await;
    assert!(recv(&mut ws).await.is_none());

    // Listed node but signature made over a different nonce.
    let (mut ws, _nonce) = app.ws_connect().await;
    let wrong = fsync_proto::Nonce::from([9u8; 32]);
    let msg = ClientMsg::Auth {
        access: s.access.clone(),
        node_id: d1.node,
        sig: sign_challenge(&d1.key, &wrong),
    };
    send(&mut ws, &msg).await;
    assert!(recv(&mut ws).await.is_none());

    // Listed node signed by another key.
    let (mut ws, nonce) = app.ws_connect().await;
    let msg = ClientMsg::Auth {
        access: s.access.clone(),
        node_id: d1.node,
        sig: sign_challenge(&stranger.key, &nonce),
    };
    send(&mut ws, &msg).await;
    assert!(recv(&mut ws).await.is_none());

    // Garbage access token.
    let (mut ws, nonce) = app.ws_connect().await;
    let msg = ClientMsg::Auth {
        access: "nope".into(),
        node_id: d1.node,
        sig: sign_challenge(&d1.key, &nonce),
    };
    send(&mut ws, &msg).await;
    assert!(recv(&mut ws).await.is_none());

    // Another account's token cannot claim this account's device.
    let other = app.login("b@example.com").await;
    let mut ws = app.ws_auth(&other, &d1).await;
    assert!(recv(&mut ws).await.is_none());

    // Non-Auth first message.
    let (mut ws, _) = app.ws_connect().await;
    send(&mut ws, &ClientMsg::Heartbeat).await;
    assert!(recv(&mut ws).await.is_none());
}

#[tokio::test]
async fn unlisted_node_connects_only_with_live_pairing_offer() {
    let app = TestApp::start().await;
    let (s, d1, _d2) = app.setup_two_devices("a@example.com").await;
    let newcomer = Device::new();

    let mut w1 = app.ws_auth(&s, &d1).await;
    let _ = recv(&mut w1).await; // Peers

    let created: PairingCreated = app
        .http
        .post(app.url("/v1/pairing"))
        .bearer_auth(&s.access)
        .json(&json!({ "node_id": newcomer.node, "relay_url": null }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    // Even after the code is consumed, the offer stays live until expiry.
    app.http
        .get(app.url(&format!("/v1/pairing/{}", created.code)))
        .bearer_auth(&s.access)
        .send()
        .await
        .unwrap();

    let mut wn = app.ws_auth(&s, &newcomer).await;
    // Pending node gets no peer data, but listed devices can reach it.
    assert_eq!(
        recv(&mut wn).await,
        Some(ServerMsg::Peers { peers: vec![] })
    );
    let Some(ServerMsg::PeerUpdate(p)) = recv(&mut w1).await else {
        panic!("expected PeerUpdate for pending node");
    };
    assert!(p.node_id == newcomer.node && p.online);

    // Offer expiry blocks new connections.
    drop(wn);
    sqlx::query("UPDATE pairing_codes SET expires_at = 1")
        .execute(&app.pool)
        .await
        .unwrap();
    let mut wn = app.ws_auth(&s, &newcomer).await;
    assert!(recv(&mut wn).await.is_none());
}
