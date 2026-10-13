mod common;

use common::{
    TestApp,
    ws::{recv, send},
};
use fsync_backend::util::now_ms;
use fsync_proto::presence::{Addrs, ClientMsg, ServerMsg};
use futures_util::SinkExt;
use tokio_tungstenite::tungstenite::Message;

#[tokio::test]
async fn oversize_ws_frame_is_rejected() {
    let app = TestApp::start().await;
    let (s, d1, _d2) = app.setup_two_devices("a@example.com").await;
    let mut w = app.ws_auth(&s, &d1).await;
    let _ = recv(&mut w).await;
    let _ = w.send(Message::text("x".repeat(64 * 1024))).await;
    assert!(recv(&mut w).await.is_none());
}

#[tokio::test]
async fn socket_closes_when_access_token_expires() {
    let app = TestApp::start().await;
    let (s, d1, _d2) = app.setup_two_devices("a@example.com").await;
    let now = now_ms() / 1000;
    let sid = app.state.tokens.verify_access(&s.access).unwrap().sid;
    let short = app
        .state
        .tokens
        .issue_with(s.account_id, sid, now, now - 890, now + 2)
        .unwrap();
    let s2 = common::Session {
        access: short,
        refresh: s.refresh.clone(),
        account_id: s.account_id,
    };
    let mut w = app.ws_auth(&s2, &d1).await;
    assert!(matches!(recv(&mut w).await, Some(ServerMsg::Peers { .. })));
    assert!(recv(&mut w).await.is_none());
}

#[tokio::test]
async fn logout_closes_the_session_sockets() {
    let app = TestApp::start().await;
    let (s, d1, _d2) = app.setup_two_devices("a@example.com").await;
    let mut w = app.ws_auth(&s, &d1).await;
    let _ = recv(&mut w).await;
    let r = app
        .http
        .post(app.url("/v1/auth/logout"))
        .json(&serde_json::json!({ "refresh": s.refresh }))
        .send()
        .await
        .unwrap();
    assert!(r.status().is_success());
    assert!(recv(&mut w).await.is_none());
}

#[tokio::test]
async fn unauthenticated_socket_cap_is_enforced() {
    let app = TestApp::start_with(|c| c.max_unauth_ws_per_ip = 2).await;
    let (_a, _) = app.ws_connect().await;
    let (_b, _) = app.ws_connect().await;
    let third = tokio_tungstenite::connect_async(format!("ws://{}/v1/presence", app.addr)).await;
    match third {
        Err(_) => {}
        Ok((mut ws, _)) => assert!(recv(&mut ws).await.is_none()),
    }
}

#[tokio::test]
async fn hello_is_rate_limited_and_addrs_sanitized() {
    let app = TestApp::start().await;
    let (s, d1, d2) = app.setup_two_devices("a@example.com").await;
    let mut w1 = app.ws_auth(&s, &d1).await;
    let _ = recv(&mut w1).await;
    let mut w2 = app.ws_auth(&s, &d2).await;
    let _ = recv(&mut w2).await;
    let _ = recv(&mut w1).await; // d2 online

    let addrs = Addrs {
        relay_url: None,
        direct: vec![
            "127.0.0.1:4433".parse().unwrap(),
            "0.0.0.0:4433".parse().unwrap(),
            "10.0.0.2:0".parse().unwrap(),
            "10.0.0.2:4433".parse().unwrap(),
        ],
    };
    send(&mut w2, &ClientMsg::Hello { addrs }).await;
    let Some(ServerMsg::PeerUpdate(p)) = recv(&mut w1).await else {
        panic!("expected PeerUpdate");
    };
    assert_eq!(p.addrs.direct, vec!["10.0.0.2:4433".parse().unwrap()]);

    // Insecure relay url closes the socket.
    let bad = Addrs {
        relay_url: Some("javascript:alert(1)".into()),
        direct: vec![],
    };
    send(&mut w2, &ClientMsg::Hello { addrs: bad }).await;
    assert!(recv(&mut w2).await.is_none());

    // Flooding Hello closes the socket.
    let mut w2 = app.ws_auth(&s, &d2).await;
    let _ = recv(&mut w2).await;
    for _ in 0..20 {
        send(
            &mut w2,
            &ClientMsg::Hello {
                addrs: Addrs::default(),
            },
        )
        .await;
    }
    let mut closed = false;
    for _ in 0..30 {
        if recv(&mut w2).await.is_none() {
            closed = true;
            break;
        }
    }
    assert!(closed);
}

#[tokio::test]
async fn heartbeat_flood_is_closed() {
    let app = TestApp::start().await;
    let (s, d1, _d2) = app.setup_two_devices("a@example.com").await;
    let mut w = app.ws_auth(&s, &d1).await;
    let _ = recv(&mut w).await;
    for _ in 0..7 {
        send(&mut w, &ClientMsg::Heartbeat).await;
    }
    assert!(recv(&mut w).await.is_none(), "7th heartbeat/min closes");
}

#[tokio::test]
async fn pending_socket_ends_when_its_pairing_offer_expires() {
    let app =
        TestApp::start_with(|c| c.pending_recheck = std::time::Duration::from_millis(200)).await;
    let (s, d1, _d2) = app.setup_two_devices("a@example.com").await;
    let newcomer = common::devices::Device::new();
    app.http
        .post(app.url("/v1/pairing"))
        .bearer_auth(&s.access)
        .json(&serde_json::json!({ "node_id": newcomer.node, "relay_url": null }))
        .send()
        .await
        .unwrap();
    let mut wn = app.ws_auth(&s, &newcomer).await;
    assert!(matches!(recv(&mut wn).await, Some(ServerMsg::Peers { .. })));
    let mut listed = app.ws_auth(&s, &d1).await;
    let _ = recv(&mut listed).await;
    sqlx::query("UPDATE pairing_codes SET expires_at = 1")
        .execute(&app.pool)
        .await
        .unwrap();
    // The recheck notices the expired offer and drops the unlisted socket.
    loop {
        match recv(&mut wn).await {
            None => break,
            Some(_) => continue,
        }
    }
}
