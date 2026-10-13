//! A client that never finishes its request headers is disconnected; /health skips the cap.
mod common;

use common::TestApp;
use std::time::Duration;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};

#[tokio::test]
async fn slow_header_client_is_dropped_and_websocket_upgrade_still_works() {
    let app = TestApp::start().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let router = fsync_backend::build_router(app.state.clone());
    tokio::spawn(fsync_backend::serve::serve(
        listener,
        router,
        Duration::from_millis(400),
        std::future::pending(),
    ));

    let mut slow = TcpStream::connect(addr).await.unwrap();
    slow.write_all(b"GET /health HTTP/1.1\r\nHost: x\r\n")
        .await
        .unwrap();
    let mut buf = Vec::new();
    let read = tokio::time::timeout(Duration::from_secs(5), slow.read_to_end(&mut buf)).await;
    assert!(read.is_ok(), "server must close the stalled connection");

    let ok = reqwest::get(format!("http://{addr}/health")).await.unwrap();
    assert_eq!(ok.status(), 200);

    let (mut ws, _) = tokio_tungstenite::connect_async(format!("ws://{addr}/v1/presence"))
        .await
        .unwrap();
    assert!(
        common::ws::recv(&mut ws).await.is_some(),
        "challenge arrives"
    );
}

#[tokio::test]
async fn health_is_exempt_from_the_in_flight_cap() {
    let app = TestApp::start_with(|c| c.max_in_flight = 1).await;
    let _hold = app.state.in_flight.clone().try_acquire_owned().unwrap();
    let r = app.http.get(app.url("/health")).send().await.unwrap();
    assert_eq!(r.status(), 200);
    let r = app
        .http
        .get(app.url("/v1/device-list"))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 503);
}
