//! BlobStore contract shared by the plain and the encrypted store: finish() length check,
//! no empty blob from a missing partial, serialized appends, safe export.

use fsync_core::blobs::{BlobStore, EncryptedBlobStore, FsBlobStore};
use std::sync::Arc;

type Stores = (tempfile::TempDir, Vec<(&'static str, Arc<dyn BlobStore>)>);

fn stores() -> Stores {
    let tmp = tempfile::tempdir().unwrap();
    let plain: Arc<dyn BlobStore> = Arc::new(FsBlobStore::open(tmp.path().join("p")).unwrap());
    let inner: Arc<dyn BlobStore> = Arc::new(FsBlobStore::open(tmp.path().join("e")).unwrap());
    let enc: Arc<dyn BlobStore> = Arc::new(EncryptedBlobStore::new(inner, &[7; 32]).unwrap());
    (tmp, vec![("plain", plain), ("encrypted", enc)])
}

#[test]
fn finish_requires_exact_length() {
    let (_t, all) = stores();
    for (name, s) in all {
        let h = [1u8; 32];
        s.append_part(&h, 0, b"hello").unwrap();
        assert!(s.finish(&h, 6).is_err(), "{name}: too long expected");
        assert!(s.finish(&h, 4).is_err(), "{name}: too short expected");
        assert_eq!(s.size_of(&h).unwrap(), None, "{name}: nothing promoted");
        s.finish(&h, 5).unwrap();
        assert!(s.size_of(&h).unwrap().is_some(), "{name}");
    }
}

#[test]
fn finish_without_partial_never_creates_a_blob() {
    let (_t, all) = stores();
    for (name, s) in all {
        for size in [0, 10] {
            assert!(s.finish(&[2; 32], size).is_err(), "{name} size {size}");
            assert_eq!(s.size_of(&[2; 32]).unwrap(), None, "{name}");
        }
        // The explicit way to store an empty blob.
        s.put(&[3; 32], b"").unwrap();
        assert_eq!(s.size_of(&[3; 32]).unwrap(), Some(0), "{name}");
    }
}

#[test]
fn racing_appends_at_one_offset_admit_exactly_one() {
    let (_t, all) = stores();
    for (name, s) in all {
        let h = [4u8; 32];
        let threads: Vec<_> = (0..8u8)
            .map(|i| {
                let s = s.clone();
                std::thread::spawn(move || s.append_part(&h, 0, &[i; 1000]).is_ok())
            })
            .collect();
        let ok = threads
            .into_iter()
            .map(|t| t.join().unwrap())
            .filter(|b| *b)
            .count();
        assert_eq!(ok, 1, "{name}");
        assert_eq!(s.part_len(&h).unwrap(), 1000, "{name}");
    }
}

#[test]
fn export_never_clobbers_and_picks_a_free_name() {
    let (t, all) = stores();
    let out = t.path().join("out");
    std::fs::create_dir(&out).unwrap();
    for (name, s) in all {
        let data = b"payload".to_vec();
        let h = *blake3::hash(&data).as_bytes();
        s.put(&h, &data).unwrap();
        let first = s.export(&h, &out.join(format!("{name}.txt"))).unwrap();
        let second = s.export(&h, &out.join(format!("{name}.txt"))).unwrap();
        assert_ne!(first, second, "{name}");
        assert_eq!(
            second.file_name().unwrap().to_string_lossy(),
            format!("{name} (1).txt")
        );
        assert_eq!(std::fs::read(&second).unwrap(), data);
        // a.txt and a.png do not share a temp name any more
        let png = s.export(&h, &out.join(format!("{name}.png"))).unwrap();
        assert!(png.exists());
    }
    let leftovers: Vec<_> = std::fs::read_dir(&out)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().ends_with(".fsync-part"))
        .collect();
    assert!(leftovers.is_empty(), "temp files cleaned up");
}

#[cfg(unix)]
#[test]
fn export_does_not_follow_a_symlink_at_the_destination() {
    let (t, all) = stores();
    let out = t.path().join("out2");
    std::fs::create_dir(&out).unwrap();
    let victim = t.path().join("victim");
    std::fs::write(&victim, b"keep me").unwrap();
    for (name, s) in all {
        let h = *blake3::hash(b"new").as_bytes();
        s.put(&h, b"new").unwrap();
        let dest = out.join(format!("{name}.txt"));
        std::os::unix::fs::symlink(&victim, &dest).unwrap();
        let used = s.export(&h, &dest).unwrap();
        assert_ne!(used, dest, "{name}: symlinked name is treated as taken");
        assert_eq!(std::fs::read(&victim).unwrap(), b"keep me", "{name}");
    }
}

#[test]
fn export_failure_leaves_no_temp_file() {
    let (t, all) = stores();
    let out = t.path().join("out3");
    std::fs::create_dir(&out).unwrap();
    for (name, s) in all {
        // Blob that does not exist: read_at errors mid-export.
        assert!(s.export(&[9; 32], &out.join(name)).is_err());
    }
    assert_eq!(std::fs::read_dir(&out).unwrap().count(), 0);
}

#[test]
fn truncate_part_only_shrinks() {
    let (_t, all) = stores();
    for (name, s) in all {
        let h = [9u8; 32];
        s.append_part(&h, 0, b"hello").unwrap();
        assert!(s.truncate_part(&h, 6).is_err(), "{name}: must not extend");
        assert_eq!(
            s.part_len(&h).unwrap(),
            5,
            "{name}: unchanged after refusal"
        );
        s.truncate_part(&h, 5).unwrap();
        s.truncate_part(&h, 2).unwrap();
        assert_eq!(s.part_len(&h).unwrap(), 2, "{name}");
    }
}
