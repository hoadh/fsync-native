//! iroh 1.x spike. Usage:
//!   relay [port]                       in-process iroh-relay (http, dev mode)
//!   listen   [flags] [--blobs-dir D --blob-mib N]
//!   dial  <id> [flags]                 connect, report timings + path changes
//!   bench <id> <MiB> [flags]           upload throughput after path settles
//!   blob-fetch <id> <hash> <dir> [flags]
//! flags: --relay URL --mdns --mdns-ip-only --no-ip --hint-relay --hint-ip A --static-lookup
//! env: KEY_FILE=path (persist secret key)
mod blob_cmds;
mod echo_proto;
mod node;

use echo_proto::Spike;
use iroh::{endpoint::Connection, protocol::Router, EndpointId, SecretKey};
use iroh_relay::server::{RelayConfig, Server, ServerConfig};
use n0_future::StreamExt;
use node::{Opts, Res, ALPN};
use std::time::{Duration, Instant};

fn key() -> Res<SecretKey> {
    if let Ok(p) = std::env::var("KEY_FILE") {
        if let Ok(b) = std::fs::read(&p) {
            return Ok(SecretKey::from_bytes(&b.try_into().map_err(|_| "bad key file")?));
        }
        let k = SecretKey::generate();
        std::fs::write(&p, k.to_bytes())?;
        return Ok(k);
    }
    Ok(SecretKey::generate())
}

async fn relay(port: u16) -> Res<()> {
    let mut cfg = ServerConfig::default();
    cfg.relay = Some(RelayConfig::new(([127, 0, 0, 1], port)));
    let server = Server::spawn(cfg).await?;
    println!("RELAY http://{}", server.http_addr().ok_or("no http addr")?);
    tokio::signal::ctrl_c().await?;
    server.shutdown().await?;
    Ok(())
}

async fn listen(mut o: Opts, pos: Vec<String>) -> Res<()> {
    o.alpns = vec![ALPN.to_vec(), iroh_blobs::ALPN.to_vec()];
    let (ep, _) = node::bind(&o, key()?).await?;
    println!("ID {}", ep.id());
    let mut rb = Router::builder(ep.clone()).accept(ALPN, Spike);
    rb = blob_cmds::maybe_serve(rb, &pos).await?;
    let t0 = Instant::now();
    ep.online().await;
    println!("ONLINE_MS {}", t0.elapsed().as_millis());
    println!("ADDR {:?}", ep.addr());
    let _router = rb.spawn();
    tokio::signal::ctrl_c().await?;
    Ok(())
}

async fn watch_paths(conn: Connection, t0: Instant) {
    let mut ev = conn.path_events();
    while let Some(e) = ev.next().await {
        use iroh::endpoint::PathEvent as P;
        let ms = t0.elapsed().as_millis();
        match e {
            P::Opened { id, remote_addr, .. } => println!("PATH +{ms}ms opened {id:?} {remote_addr:?}"),
            P::Closed { id, remote_addr, .. } => println!("PATH +{ms}ms closed {id:?} {remote_addr:?}"),
            P::Selected { id, remote_addr, .. } => println!("PATH +{ms}ms SELECTED {id:?} {remote_addr:?}"),
            other => println!("PATH +{ms}ms {other:?}"),
        }
    }
}

/// Connect and wait (<=15 s) until the selected path is a direct IP path.
async fn connect(o: &Opts, id: EndpointId) -> Res<(iroh::Endpoint, Connection, Instant)> {
    let mut o = o.clone();
    o.alpns = vec![];
    let tb = Instant::now();
    let (ep, lookup) = node::bind(&o, key()?).await?;
    println!("BIND_MS {}", tb.elapsed().as_millis());
    if std::env::var("WAIT_ONLINE").is_ok() {
        let t = Instant::now();
        let r = tokio::time::timeout(Duration::from_secs(10), ep.online()).await;
        println!("ONLINE_MS {} timed_out={}", t.elapsed().as_millis(), r.is_err());
    }
    let addr = node::target(&o, id, &lookup);
    let t0 = Instant::now();
    let conn = ep.connect(addr, ALPN).await?;
    println!("CONNECTED_MS {} (dial by id -> handshake done)", t0.elapsed().as_millis());
    tokio::spawn(watch_paths(conn.clone(), t0));
    Ok((ep, conn, t0))
}

fn selected_is_ip(conn: &Connection) -> Option<bool> {
    conn.paths().iter().find(|p| p.is_selected()).map(|p| p.is_ip())
}

async fn wait_direct(conn: &Connection, t0: Instant) -> bool {
    while t0.elapsed() < Duration::from_secs(15) {
        if selected_is_ip(conn) == Some(true) {
            println!("DIRECT_MS {}", t0.elapsed().as_millis());
            return true;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    println!("DIRECT_NEVER (15s)");
    false
}

async fn op(conn: &Connection, code: u8, len: u64, payload: u64) -> Res<Duration> {
    let (mut tx, mut rx) = conn.open_bi().await?;
    let t = Instant::now();
    let mut hdr = vec![code];
    hdr.extend_from_slice(&len.to_be_bytes());
    tx.write_all(&hdr).await?;
    let buf = vec![0xA5u8; 1 << 20];
    let mut left = payload;
    while left > 0 {
        let n = left.min(buf.len() as u64) as usize;
        tx.write_all(&buf[..n]).await?;
        left -= n as u64;
    }
    tx.finish()?;
    let mut ack = [0u8; 1];
    rx.read_exact(&mut ack).await?;
    Ok(t.elapsed())
}

async fn dial(o: Opts, id: EndpointId) -> Res<()> {
    let (ep, conn, t0) = connect(&o, id).await?;
    let mut rtts = vec![];
    for _ in 0..5 { rtts.push(op(&conn, b'E', 0, 0).await?.as_micros()); }
    println!("RTT_US {rtts:?} (first on initial path)");
    wait_direct(&conn, t0).await;
    let mut rtts = vec![];
    for _ in 0..5 { rtts.push(op(&conn, b'E', 0, 0).await?.as_micros()); }
    println!("RTT_US_AFTER {rtts:?} selected_is_ip={:?}", selected_is_ip(&conn));
    conn.close(0u32.into(), b"bye");
    ep.close().await;
    Ok(())
}

async fn bench(o: Opts, id: EndpointId, mib: u64) -> Res<()> {
    let (ep, conn, t0) = connect(&o, id).await?;
    op(&conn, b'E', 0, 0).await?;
    if !o.no_ip { wait_direct(&conn, t0).await; }
    let sel = selected_is_ip(&conn);
    println!("SELECTED_PATH {:?}", conn.paths().iter().find(|p| p.is_selected()).map(|p| p.remote_addr().clone()));
    let bytes = mib << 20;
    let d = op(&conn, b'U', bytes, bytes).await?;
    println!("BENCH {mib} MiB in {:.2}s = {:.1} MiB/s ({:.0} Mbit/s) selected_is_ip={sel:?}",
        d.as_secs_f64(), mib as f64 / d.as_secs_f64(), mib as f64 * 8.388608 / d.as_secs_f64());
    conn.close(0u32.into(), b"bye");
    ep.close().await;
    Ok(())
}

#[tokio::main]
async fn main() -> Res<()> {
    if std::env::var("RUST_LOG").is_ok() {
        tracing_subscriber::fmt().with_env_filter(tracing_subscriber::EnvFilter::from_default_env()).init();
    }
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some((cmd, rest)) = args.split_first() else { return Err("no command".into()) };
    let (o, pos) = node::parse(rest)?;
    match cmd.as_str() {
        "relay" => relay(pos.first().map(|p| p.parse()).transpose()?.unwrap_or(3340)).await,
        "listen" => listen(o, pos).await,
        "dial" => dial(o, pos[0].parse()?).await,
        "bench" => bench(o, pos[0].parse()?, pos[1].parse()?).await,
        "blob-fetch" => blob_cmds::fetch(o, pos).await,
        _ => Err(format!("unknown command {cmd}").into()),
    }
}
