//! Receiver-side resource limits: single-flight fetches, size policy, offer flooding, API hygiene.

mod common;

use common::engine::{EngPeer, group};
use common::files::{hash_file, write_src};
use fsync_core::{
    CoreError, EngineConfig, EngineEvent, Result,
    api::ApiClient,
    blobs::{BlobInfo, BlobStore, FsBlobStore},
    clip::{ClipInput, FileInput, KEEP_BYTES, PublishOpts, ReceivedLog, ReceivedOffer},
    file::{check_declared_size, check_fetch, fetch_deadline, max_verifiable_size},
};
use fsync_proto::{ALPN_CLIP, ClipKind, ClipOffer, RepMeta, frame::encode_frame};
use std::{sync::Arc, time::Duration};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use uuid::Uuid;

fn offer(
    origin: fsync_proto::NodeId,
    reps: Vec<RepMeta>,
    inline: Vec<Option<Vec<u8>>>,
) -> ClipOffer {
    ClipOffer {
        v: 1,
        item_id: Uuid::now_v7(),
        origin,
        ts_ms: 1,
        kind: ClipKind::Text,
        reps,
        inline,
        files: vec![],
        preview: None,
        push: false,
        sensitive: false,
    }
}

fn text(origin: fsync_proto::NodeId, s: &str) -> ClipOffer {
    let rep = RepMeta {
        mime: "text/plain".into(),
        size: s.len() as u64,
        hash: *blake3::hash(s.as_bytes()).as_bytes(),
    };
    offer(origin, vec![rep], vec![Some(s.as_bytes().to_vec())])
}

async fn send_offers(from: &EngPeer, to: &EngPeer, offers: &[ClipOffer]) {
    let conn = from.node().connect(&to.nid(), ALPN_CLIP).await.unwrap();
    for o in offers {
        let (mut tx, _rx) = conn.open_bi().await.unwrap();
        tx.write_all(&encode_frame(o).unwrap()).await.unwrap();
        tx.finish().unwrap();
    }
    tokio::time::sleep(Duration::from_millis(500)).await;
}

fn count_remote(p: &mut EngPeer) -> usize {
    let mut n = 0;
    while let Ok(e) = p.events.try_recv() {
        n += usize::from(matches!(e, EngineEvent::RemoteClip { .. }));
    }
    n
}

#[tokio::test(flavor = "multi_thread")]
async fn concurrent_fetches_of_one_file_transfer_it_once() {
    let mut ps = group(vec![EngineConfig::default(); 2]).await;
    let src = write_src(ps[0].dir.path(), "s.bin", 24 << 20, 21);
    let (id, f) = common::files::offer_file(&mut ps, &src).await;
    let (e1, e2) = (ps[1].engine.clone(), ps[1].engine.clone());
    let (a, b) = tokio::join!(e1.fetch_file(id, f.file_id), e2.fetch_file(id, f.file_id));
    let (a, b) = (a.unwrap(), b.unwrap());
    assert_eq!(a.bytes_received + b.bytes_received, f.size);
    let dest = ps[1].dir.path().join("out");
    std::fs::create_dir(&dest).unwrap();
    let p = ps[1]
        .engine
        .fetch_file_to(id, f.file_id, &dest)
        .await
        .unwrap();
    assert_eq!(hash_file(&p), f.hash);
}

#[tokio::test(flavor = "multi_thread")]
async fn max_file_bytes_refuses_before_any_transfer() {
    let cfg = EngineConfig {
        max_file_bytes: Some(1 << 20),
        ..EngineConfig::default()
    };
    let mut ps = group(vec![EngineConfig::default(), cfg]).await;
    let src = write_src(ps[0].dir.path(), "m.bin", 3 << 20, 4);
    let (id, f) = common::files::offer_file(&mut ps, &src).await;
    let r = ps[1].engine.fetch_file(id, f.file_id).await;
    assert!(matches!(r, Err(CoreError::TooLarge { .. })), "{r:?}");
    let dest = ps[1].dir.path().join("out");
    std::fs::create_dir(&dest).unwrap();
    let r = ps[1].engine.fetch_file_to(id, f.file_id, &dest).await;
    assert!(matches!(r, Err(CoreError::TooLarge { .. })), "{r:?}");
    let r = ps[1].engine.read_file_range(id, f.file_id, 0, 10).await;
    assert!(matches!(r, Err(CoreError::TooLarge { .. })), "{r:?}");
    assert_eq!(common::files::part_len(&ps[1], &f.hash), 0);
}

/// A store that claims to be nearly full.
struct Tiny(FsBlobStore, u64);

impl BlobStore for Tiny {
    fn size_of(&self, h: &[u8; 32]) -> Result<Option<u64>> {
        self.0.size_of(h)
    }
    fn read_at(&self, h: &[u8; 32], o: u64, b: &mut [u8]) -> Result<usize> {
        self.0.read_at(h, o, b)
    }
    fn read_part_at(&self, h: &[u8; 32], o: u64, b: &mut [u8]) -> Result<usize> {
        self.0.read_part_at(h, o, b)
    }
    fn put(&self, h: &[u8; 32], d: &[u8]) -> Result<()> {
        self.0.put(h, d)
    }
    fn part_len(&self, h: &[u8; 32]) -> Result<u64> {
        self.0.part_len(h)
    }
    fn append_part(&self, h: &[u8; 32], o: u64, d: &[u8]) -> Result<()> {
        self.0.append_part(h, o, d)
    }
    fn truncate_part(&self, h: &[u8; 32], l: u64) -> Result<()> {
        self.0.truncate_part(h, l)
    }
    fn finish(&self, h: &[u8; 32], n: u64) -> Result<()> {
        self.0.finish(h, n)
    }
    fn remove(&self, h: &[u8; 32]) -> Result<()> {
        self.0.remove(h)
    }
    fn list(&self) -> Result<Vec<BlobInfo>> {
        self.0.list()
    }
    fn sweep_stale_parts(&self, d: Duration) -> Result<usize> {
        self.0.sweep_stale_parts(d)
    }
    fn free_space(&self) -> Result<Option<u64>> {
        Ok(Some(self.1))
    }
}

#[tokio::test]
async fn declared_size_beyond_free_space_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let store: Arc<dyn BlobStore> = Arc::new(Tiny(
        FsBlobStore::open(dir.path().join("b")).unwrap(),
        10 << 20,
    ));
    let h = [7; 32];
    assert!(check_fetch(&store, h, 5 << 20, None).await.is_ok());
    let r = check_fetch(&store, h, 50 << 20, None).await;
    assert!(
        matches!(r, Err(CoreError::TooLarge { max, .. }) if max == 10 << 20),
        "{r:?}"
    );
    // What a partial download already holds does not count again.
    store.append_part(&h, 0, &vec![0u8; 8 << 20]).unwrap();
    assert!(check_fetch(&store, h, 15 << 20, None).await.is_ok());
}

#[test]
fn size_policy_bounds_cv_memory_and_time() {
    let cap = max_verifiable_size();
    assert!(check_declared_size(cap, cap, None, None).is_ok());
    assert!(matches!(
        check_declared_size(cap + 1, cap + 1, None, None),
        Err(CoreError::TooLarge { .. })
    ));
    assert!(check_declared_size(u64::MAX, u64::MAX, None, Some(u64::MAX)).is_err());
    assert!(matches!(
        check_declared_size(100, 100, Some(99), Some(1 << 40)),
        Err(CoreError::TooLarge { size: 100, max: 99 })
    ));
    assert!(fetch_deadline(0) >= Duration::from_secs(60));
    assert!(fetch_deadline(10 << 30) > fetch_deadline(1 << 30));
}

#[tokio::test(flavor = "multi_thread")]
async fn overflowing_rep_sizes_are_rejected() {
    let mut ps = group(vec![EngineConfig::default(); 2]).await;
    let rep = |n| RepMeta {
        mime: "a/b".into(),
        size: n,
        hash: [1; 32],
    };
    let o = offer(ps[0].nid(), vec![rep(u64::MAX), rep(2)], vec![None, None]);
    send_offers(&ps[0], &ps[1], &[o]).await;
    assert_eq!(count_remote(&mut ps[1]), 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn offer_flood_from_one_peer_is_throttled() {
    let mut ps = group(vec![EngineConfig::default(); 2]).await;
    let offers: Vec<_> = (0..80)
        .map(|i| text(ps[0].nid(), &format!("flood {i}")))
        .collect();
    send_offers(&ps[0], &ps[1], &offers).await;
    let got = count_remote(&mut ps[1]);
    assert!((10..=40).contains(&got), "accepted {got} of 80");
}

#[test]
fn received_log_stays_within_its_byte_budget() {
    let from = fsync_core::Identity::generate().unwrap().node_id();
    let mut log = ReceivedLog::default();
    let mut evicted = 0;
    for i in 0..40 {
        let big = vec![0u8; 4 << 20];
        let mut o = text(from, "x");
        o.reps[0].size = big.len() as u64;
        o.inline = vec![Some(big)];
        let id = o.item_id;
        evicted += log
            .keep(Arc::new(ReceivedOffer {
                from,
                offer: Arc::new(o),
            }))
            .len();
        assert!(log.get(&id).is_some(), "newest entry {i} must stay");
        assert!(log.kept_bytes() <= KEEP_BYTES + (4 << 20) + 1024);
    }
    assert!(evicted >= 30, "evicted {evicted}");
}

#[tokio::test(flavor = "multi_thread")]
async fn range_reader_cache_is_bounded() {
    let mut ps = group(vec![EngineConfig::default(); 2]).await;
    let files: Vec<_> = (0..12)
        .map(|i| write_src(ps[0].dir.path(), &format!("f{i}.bin"), 64 << 10, i))
        .collect();
    ps[0]
        .engine
        .publish_clip(
            ClipInput::files(files.iter().map(FileInput::new).collect()),
            PublishOpts::default(),
        )
        .await
        .unwrap();
    let (id, n) = ps[1]
        .wait(|e| match e {
            EngineEvent::RemoteClip { offer, .. } => Some((offer.item_id, offer.files.len())),
            _ => None,
        })
        .await;
    for fid in 0..n as u32 {
        let b = ps[1]
            .engine
            .read_file_range(id, fid, 0, 1024)
            .await
            .unwrap();
        assert_eq!(b.len(), 1024);
    }
    assert!(ps[1].engine.open_readers() <= fsync_core::file::MAX_READERS);
}

#[test]
fn plain_http_backend_is_refused_unless_allowed() {
    assert!(ApiClient::new("http://api.example.com", None, None).is_err());
    assert!(ApiClient::new("ftp://api.example.com", None, None).is_err());
    assert!(ApiClient::new("api.example.com", None, None).is_err());
    assert!(ApiClient::new("https://api.example.com", None, None).is_ok());
    assert!(ApiClient::with_options("http://127.0.0.1:1", None, None, true).is_ok());
}

#[tokio::test]
async fn oversized_api_response_is_refused() {
    let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = l.local_addr().unwrap();
    tokio::spawn(async move {
        while let Ok((mut s, _)) = l.accept().await {
            tokio::spawn(async move {
                let mut buf = [0u8; 2048];
                let _ = s.read(&mut buf).await;
                let body = vec![b' '; 3 << 20];
                let head = format!(
                    "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n",
                    body.len()
                );
                let _ = s.write_all(head.as_bytes()).await;
                let _ = s.write_all(&body).await;
            });
        }
    });
    let api = ApiClient::with_options(&format!("http://{addr}"), None, None, true).unwrap();
    let r = api.otp_verify("a@b.c", "123456").await;
    assert!(
        matches!(&r, Err(CoreError::Http(m)) if m.contains("too large")),
        "{r:?}"
    );
}
