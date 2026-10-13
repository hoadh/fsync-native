//! file/1 serves at most 16 requests per connection at once; the rest wait their turn.

mod common;

use common::engine::group;
use fsync_core::{
    EngineConfig,
    net::frames::{recv_frame, send_frame},
};
use fsync_proto::{ALPN_FILE, FetchReq, FetchResp, FetchStatus, FetchTarget};
use std::time::{Duration, Instant};
use uuid::Uuid;

const CAP: usize = 16;

fn req() -> FetchReq {
    FetchReq {
        v: 1,
        item_id: Uuid::new_v4(),
        target: FetchTarget::Rep(0),
        offset: 0,
        len: 0,
        no_cvs: true,
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn extra_streams_wait_for_a_free_handler_slot() {
    let ps = group(vec![EngineConfig::default(); 2]).await;
    let conn = ps[1].node().connect(&ps[0].nid(), ALPN_FILE).await.unwrap();

    // Control: an idle connection answers at once.
    let t = Instant::now();
    let (mut tx, mut rx) = conn.open_bi().await.unwrap();
    send_frame(&mut tx, &req()).await.unwrap();
    let r: FetchResp = recv_frame(&mut rx, Duration::from_secs(5)).await.unwrap();
    assert_eq!(r.status, FetchStatus::NotFound);
    assert!(t.elapsed() < Duration::from_secs(2));

    // CAP streams that start a request and then say nothing hold every slot.
    let mut stalled = Vec::new();
    for _ in 0..CAP {
        let (mut tx, rx) = conn.open_bi().await.unwrap();
        tx.write_all(&[0u8]).await.unwrap();
        stalled.push((tx, rx));
    }
    tokio::time::sleep(Duration::from_millis(300)).await;

    // The next request is not served until a stalled handler times out (REQ_TIMEOUT, 10 s).
    let t = Instant::now();
    let (mut tx, mut rx) = conn.open_bi().await.unwrap();
    send_frame(&mut tx, &req()).await.unwrap();
    let r: FetchResp = recv_frame(&mut rx, Duration::from_secs(20)).await.unwrap();
    assert_eq!(r.status, FetchStatus::NotFound);
    assert!(
        t.elapsed() > Duration::from_secs(5),
        "served after {:?}: cap not enforced",
        t.elapsed()
    );
}
