mod history_common;
use fsync_core::{
    CoreError,
    blobs::{BlobStore, EncryptedBlobStore, FsBlobStore},
};
use history_common::{pseudo, spy::Spy};
use std::{
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering::Relaxed},
    },
    time::Duration,
};

const KEY: [u8; 32] = [1; 32];

fn setup() -> (tempfile::TempDir, EncryptedBlobStore) {
    let tmp = tempfile::tempdir().unwrap();
    let fs = Arc::new(FsBlobStore::open(tmp.path().join("b")).unwrap());
    (tmp, EncryptedBlobStore::new(fs, &KEY).unwrap())
}

fn path_of(tmp: &tempfile::TempDir, h: &[u8; 32]) -> std::path::PathBuf {
    tmp.path()
        .join("b")
        .join(blake3::Hash::from_bytes(*h).to_hex().as_str())
}

fn read_all(s: &dyn BlobStore, h: &[u8; 32]) -> Vec<u8> {
    let mut out = vec![0u8; s.size_of(h).unwrap().unwrap() as usize];
    let mut got = 0;
    while got < out.len() {
        let n = s.read_at(h, got as u64, &mut out[got..]).unwrap();
        assert!(n > 0);
        got += n;
    }
    out
}

#[test]
fn roundtrip_sizes_and_ciphertext_differs() {
    let (tmp, s) = setup();
    for (i, len) in [0usize, 1, 65535, 65536, 65537, 131072, 200_000]
        .into_iter()
        .enumerate()
    {
        let data = pseudo(len, i as u64 + 1);
        let h = *blake3::hash(&data).as_bytes();
        s.put(&h, &data).unwrap();
        s.put(&h, &data).unwrap(); // idempotent
        assert_eq!(s.size_of(&h).unwrap(), Some(len as u64));
        assert_eq!(read_all(&s, &h), data, "len {len}");
        let disk = std::fs::read(path_of(&tmp, &h)).unwrap();
        assert!(disk.len() > len, "header + tags");
        if len >= 32 {
            assert!(
                !disk.windows(32).any(|w| w == &data[..32]),
                "plaintext not on disk"
            );
        }
    }
}

#[test]
fn ranged_reads_cross_chunk_boundaries() {
    let (_t, s) = setup();
    let data = pseudo(300_000, 9);
    let h = *blake3::hash(&data).as_bytes();
    s.put(&h, &data).unwrap();
    for (off, len) in [
        (0, 10),
        (65530, 20),
        (65536, 65536),
        (131000, 150_000),
        (299_990, 100),
        (300_000, 5),
    ] {
        let mut buf = vec![0u8; len];
        let n = s.read_at(&h, off, &mut buf).unwrap();
        let end = (off as usize + len).min(data.len());
        let want = &data[(off as usize).min(data.len())..end];
        assert_eq!(&buf[..n], want, "off {off} len {len}");
    }
}

#[test]
fn tampering_is_detected() {
    let (tmp, s) = setup();
    let data = pseudo(150_000, 4);
    let h = *blake3::hash(&data).as_bytes();
    let p = path_of(&tmp, &h);

    // Bit flip in the middle chunk.
    s.put(&h, &data).unwrap();
    let good = std::fs::read(&p).unwrap();
    let mut bad = good.clone();
    bad[70_000] ^= 1;
    std::fs::write(&p, &bad).unwrap();
    let mut buf = vec![0u8; 150_000];
    assert!(matches!(
        s.read_at(&h, 0, &mut buf),
        Err(CoreError::Store(_))
    ));

    // Truncation at a chunk boundary (drops the sealed-as-last chunk).
    std::fs::write(&p, &good[..good.len() - (150_000 - 131_072 + 16)]).unwrap();
    assert!(s.read_at(&h, 0, &mut buf).is_err());

    // Ciphertext moved under another hash (AAD = hash).
    let other = pseudo(10, 5);
    let ho = *blake3::hash(&other).as_bytes();
    std::fs::write(path_of(&tmp, &ho), &good).unwrap();
    assert!(s.read_at(&ho, 0, &mut buf).is_err());

    // Wrong key.
    std::fs::write(&p, &good).unwrap();
    let fs2 = Arc::new(FsBlobStore::open(tmp.path().join("b")).unwrap());
    let s2 = EncryptedBlobStore::new(fs2, &[2; 32]).unwrap();
    assert!(s2.read_at(&h, 0, &mut buf).is_err());
    assert_eq!(read_all(&s, &h), data, "restored file reads fine");
}

#[test]
fn partial_download_flow_and_resume() {
    let (tmp, s) = setup();
    let data = pseudo(200_000, 8);
    let h = *blake3::hash(&data).as_bytes();
    s.append_part(&h, 0, &data[..100_000]).unwrap();
    assert_eq!(s.part_len(&h).unwrap(), 100_000);
    assert!(s.append_part(&h, 5, b"x").is_err(), "offset must match");
    let mut b = [0u8; 10];
    s.read_part_at(&h, 99_995, &mut b[..5]).unwrap();
    assert_eq!(&b[..5], &data[99_995..100_000]);
    s.truncate_part(&h, 60_000).unwrap(); // resume after verification failure
    s.append_part(&h, 60_000, &data[60_000..]).unwrap();
    assert_eq!(s.size_of(&h).unwrap(), None);
    s.finish(&h, 200_000).unwrap();
    assert_eq!(read_all(&s, &h), data);
    assert_eq!(s.part_len(&h).unwrap(), 0);
    let files: Vec<_> = std::fs::read_dir(tmp.path().join("b")).unwrap().collect();
    assert_eq!(files.len(), 1, "staging cleaned up");
    assert_eq!(s.list().unwrap().len(), 1);
    s.remove(&h).unwrap();
    assert!(s.list().unwrap().is_empty());

    // Empty blob: only via put; finish on a missing partial must not conjure one.
    let eh = *blake3::hash(b"").as_bytes();
    assert!(s.finish(&eh, 0).is_err());
    assert_eq!(s.size_of(&eh).unwrap(), None);
    s.put(&eh, b"").unwrap();
    assert_eq!(s.size_of(&eh).unwrap(), Some(0));
}

#[test]
fn stale_staging_is_swept() {
    let (tmp, s) = setup();
    let h = [3u8; 32];
    s.append_part(&h, 0, b"plaintext staging").unwrap();
    assert_eq!(s.sweep_stale_parts(Duration::from_secs(3600)).unwrap(), 0);
    assert_eq!(s.sweep_stale_parts(Duration::ZERO).unwrap(), 1);
    assert_eq!(s.part_len(&h).unwrap(), 0);
    drop(tmp);
}

#[test]
fn large_blob_streams_with_bounded_buffers() {
    const MIB64: usize = 64 << 20;
    let tmp = tempfile::tempdir().unwrap();
    let max = Arc::new(AtomicUsize::new(0));
    let spy = Arc::new(Spy {
        inner: FsBlobStore::open(tmp.path().join("b")).unwrap(),
        max: max.clone(),
    });
    let s = EncryptedBlobStore::new(spy, &KEY).unwrap();
    let block = pseudo(1 << 20, 77);
    let mut hasher = blake3::Hasher::new();
    for i in 0..(MIB64 / (64 << 10)) {
        let piece = &block[(i % 16) * (64 << 10)..][..64 << 10];
        hasher.update(piece);
        s.append_part(&[9; 32], (i * (64 << 10)) as u64, piece)
            .unwrap();
    }
    let h = *hasher.finalize().as_bytes();
    let hh = [9u8; 32];
    s.finish(&hh, MIB64 as u64).unwrap();
    assert_eq!(s.size_of(&hh).unwrap(), Some(MIB64 as u64));

    let out = tmp.path().join("out.bin");
    s.export(&hh, &out).unwrap();
    assert_eq!(*blake3::hash(&std::fs::read(&out).unwrap()).as_bytes(), h);
    // Every call across the inner boundary (staging reads, sealed segments, decrypt reads)
    // stays within one 64 KiB segment + tag: memory does not grow with the blob.
    let widest = max.load(Relaxed);
    assert!(widest <= 64 * 1024 + 16, "no whole-blob buffer: {widest}");
}
