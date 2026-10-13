mod common;

use common::{
    TestApp,
    devices::{Device, add},
    ws::{recv, send},
};
use fsync_proto::presence::{Addrs, ClientMsg, ServerMsg};

#[tokio::test]
async fn listed_nodes_see_peers_updates_and_list_changes() {
    let app = TestApp::start().await;
    let (s, d1, d2) = app.setup_two_devices("a@example.com").await;

    let mut w1 = app.ws_auth(&s, &d1).await;
    let Some(ServerMsg::Peers { peers }) = recv(&mut w1).await else {
        panic!("expected Peers");
    };
    assert_eq!(peers.len(), 1);
    assert_eq!(peers[0].node_id, d2.node);
    assert!(!peers[0].online);

    // d2 connects: d1 sees it come online; d2's snapshot shows d1 online.
    let mut w2 = app.ws_auth(&s, &d2).await;
    let Some(ServerMsg::PeerUpdate(p)) = recv(&mut w1).await else {
        panic!("expected PeerUpdate");
    };
    assert!(p.node_id == d2.node && p.online);
    let Some(ServerMsg::Peers { peers }) = recv(&mut w2).await else {
        panic!("expected Peers");
    };
    assert!(peers.len() == 1 && peers[0].node_id == d1.node && peers[0].online);

    // Hello with addresses fans out.
    let addrs = Addrs {
        relay_url: Some("https://relay.example.com".into()),
        direct: vec!["10.0.0.2:4433".parse().unwrap()],
    };
    send(
        &mut w2,
        &ClientMsg::Hello {
            addrs: addrs.clone(),
        },
    )
    .await;
    let Some(ServerMsg::PeerUpdate(p)) = recv(&mut w1).await else {
        panic!("expected PeerUpdate");
    };
    assert_eq!(p.addrs, addrs);
    send(&mut w2, &ClientMsg::Heartbeat).await;

    // Device list PUT notifies every connected device.
    let list = {
        let cur = app
            .http
            .get(app.url("/v1/device-list"))
            .bearer_auth(&s.access)
            .send()
            .await
            .unwrap()
            .json::<fsync_proto::api_dto::DeviceListResponse>()
            .await
            .unwrap();
        add(&cur.decode().unwrap(), &d1, &Device::new())
    };
    assert_eq!(
        app.put_list(&s, &list, Some("1"), false).await.status(),
        200
    );
    for w in [&mut w1, &mut w2] {
        assert_eq!(recv(w).await, Some(ServerMsg::DeviceListChanged { seq: 2 }));
    }

    // d2 disconnects: d1 sees offline with last_seen set.
    drop(w2);
    let Some(ServerMsg::PeerUpdate(p)) = recv(&mut w1).await else {
        panic!("expected offline PeerUpdate");
    };
    assert!(p.node_id == d2.node && !p.online && p.last_seen_ms > 0);
}

#[tokio::test]
async fn removed_device_is_kicked_and_silent_socket_times_out() {
    let app =
        TestApp::start_with(|c| c.heartbeat_timeout = std::time::Duration::from_millis(600)).await;
    let (s, d1, d2) = app.setup_two_devices("a@example.com").await;
    let mut w2 = app.ws_auth(&s, &d2).await;
    let _ = recv(&mut w2).await;

    let cur = app
        .http
        .get(app.url("/v1/device-list"))
        .bearer_auth(&s.access)
        .send()
        .await
        .unwrap()
        .json::<fsync_proto::api_dto::DeviceListResponse>()
        .await
        .unwrap()
        .decode()
        .unwrap();
    let removed = fsync_proto::DeviceList::next_remove(&cur, d1.node, &d2.node, 3).unwrap();
    let removed = fsync_proto::sign(&d1.key, &removed).unwrap();
    assert_eq!(
        app.put_list(&s, &removed, Some("1"), false).await.status(),
        200
    );
    // Removed node is closed directly, without receiving DeviceListChanged.
    assert!(
        recv(&mut w2).await.is_none(),
        "removed device must be disconnected silently"
    );

    // Heartbeat timeout closes an idle socket.
    let mut w1 = app.ws_auth(&s, &d1).await;
    let _ = recv(&mut w1).await;
    assert!(recv(&mut w1).await.is_none());
}
