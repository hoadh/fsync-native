//! iroh-blobs evaluation: serve a random blob from an FsStore; fetch with resume + verify.
use crate::node::{self, Opts, Res};
use iroh::protocol::RouterBuilder;
use iroh_blobs::{store::fs::FsStore, BlobsProtocol, Hash, HashAndFormat};
use n0_future::StreamExt;
use std::{path::PathBuf, time::Instant};

fn flag(pos: &[String], name: &str) -> Option<String> {
    pos.iter().position(|p| p == name).and_then(|i| pos.get(i + 1)).cloned()
}

/// `listen ... blobs-dir D blob-mib N` -> create N MiB pseudo-random file, add, print BLOB hash.
pub async fn maybe_serve(rb: RouterBuilder, pos: &[String]) -> Res<RouterBuilder> {
    let Some(dir) = flag(pos, "blobs-dir") else { return Ok(rb) };
    let mib: usize = flag(pos, "blob-mib").unwrap_or("200".into()).parse()?;
    let dir = PathBuf::from(dir);
    std::fs::create_dir_all(&dir)?;
    let file = dir.join("source.bin");
    if !file.exists() {
        let mut data = Vec::with_capacity(mib << 20);
        let mut s: u64 = 0x9E3779B97F4A7C15;
        while data.len() < mib << 20 {
            s ^= s << 13; s ^= s >> 7; s ^= s << 17; // xorshift: incompressible-ish
            data.extend_from_slice(&s.to_le_bytes());
        }
        std::fs::write(&file, &data)?;
    }
    let store = FsStore::load(dir.join("store")).await?;
    let t = Instant::now();
    let tag = store.blobs().add_path(std::path::absolute(&file)?).await?;
    println!("BLOB {} add_ms={}", tag.hash, t.elapsed().as_millis());
    Ok(rb.accept(iroh_blobs::ALPN, BlobsProtocol::new(&store, None)))
}

pub async fn fetch(mut o: Opts, pos: Vec<String>) -> Res<()> {
    let id = pos[0].parse()?;
    let hash: Hash = pos[1].parse()?;
    let dir = PathBuf::from(&pos[2]);
    o.alpns = vec![];
    let (ep, lookup) = node::bind(&o, iroh::SecretKey::generate()).await?;
    let store = FsStore::load(dir.join("store")).await?;
    let local = store.remote().local(HashAndFormat::raw(hash)).await?;
    println!("LOCAL_BYTES_BEFORE {} complete={}", local.local_bytes(), local.is_complete());
    let t0 = Instant::now();
    let conn = ep.connect(node::target(&o, id, &lookup), iroh_blobs::ALPN).await?;
    println!("CONNECTED_MS {}", t0.elapsed().as_millis());
    // fetch() requests only the ranges missing locally -> resume.
    let mut s = store.remote().fetch(conn.clone(), HashAndFormat::raw(hash)).stream();
    let mut last = 0u64;
    while let Some(item) = s.next().await {
        use iroh_blobs::api::remote::GetProgressItem as G;
        match item {
            G::Progress(n) => {
                if n / (20 << 20) != last / (20 << 20) { println!("PROGRESS {} MiB at {}ms", n >> 20, t0.elapsed().as_millis()); }
                last = n;
            }
            G::Done(st) => println!("DONE {st:?} in {}ms", t0.elapsed().as_millis()),
            G::Error(e) => return Err(format!("fetch error: {e:?}").into()),
        }
    }
    let out = dir.join("out.bin");
    store.blobs().export(hash, &out).await?;
    let h = blake3::hash(&std::fs::read(&out)?);
    println!("VERIFY blake3(out)={h} expected={hash} match={}", h.as_bytes() == hash.as_bytes());
    conn.close(0u32.into(), b"bye");
    ep.close().await;
    store.shutdown().await?;
    Ok(())
}
