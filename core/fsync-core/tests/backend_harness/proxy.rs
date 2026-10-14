//! A TCP forwarder in front of a backend that can be cut, black-holed or gated, to test client
//! behavior on a bad network against the real server.
#![allow(dead_code)]

use std::net::SocketAddr;
use std::sync::Arc;
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
    net::TcpListener,
    sync::watch,
    task::JoinHandle,
};

#[derive(Clone, Copy, PartialEq)]
pub enum Mode {
    /// Forward connections to the backend (waiting for the gate to be open first).
    Forward,
    /// Accept connections and never answer.
    Blackhole,
}

/// Copy `r` to `w`, holding each chunk back while the gate is closed (also on live connections).
async fn pipe(
    r: &mut (impl AsyncRead + Unpin),
    w: &mut (impl AsyncWrite + Unpin),
    mut open: watch::Receiver<bool>,
) -> std::io::Result<()> {
    let mut buf = vec![0u8; 16 * 1024];
    loop {
        let n = r.read(&mut buf).await?;
        if n == 0 {
            return w.shutdown().await;
        }
        let _ = open.wait_for(|o| *o).await;
        w.write_all(&buf[..n]).await?;
    }
}

pub struct Proxy {
    pub addr: SocketAddr,
    backend: SocketAddr,
    task: Option<JoinHandle<()>>,
    gate: Arc<watch::Sender<bool>>,
}

impl Proxy {
    pub async fn up(backend: SocketAddr) -> Self {
        let mut p = Self {
            addr: "127.0.0.1:0".parse().unwrap(),
            backend,
            task: None,
            gate: Arc::new(watch::channel(true).0),
        };
        p.start(Mode::Forward).await;
        p
    }

    /// (Re)open the listener on the same port.
    pub async fn start(&mut self, mode: Mode) {
        let l = TcpListener::bind(self.addr).await.unwrap();
        self.addr = l.local_addr().unwrap();
        let (backend, gate) = (self.backend, self.gate.clone());
        self.task = Some(tokio::spawn(async move {
            // Live connections belong to this task: stopping the proxy cuts them too.
            let mut conns = tokio::task::JoinSet::new();
            while let Ok((mut c, _)) = l.accept().await {
                let (mut open, g) = (gate.subscribe(), gate.clone());
                conns.spawn(async move {
                    if mode == Mode::Blackhole {
                        std::future::pending::<()>().await;
                    }
                    let _ = open.wait_for(|o| *o).await;
                    if let Ok(mut s) = tokio::net::TcpStream::connect(backend).await {
                        let (mut cr, mut cw) = c.split();
                        let (mut sr, mut sw) = s.split();
                        let _ = tokio::join!(
                            pipe(&mut cr, &mut sw, g.subscribe()),
                            pipe(&mut sr, &mut cw, g.subscribe())
                        );
                    }
                });
            }
        }));
    }

    /// Free the port (connection refused) and cut live connections.
    pub async fn stop(&mut self) {
        if let Some(t) = self.task.take() {
            t.abort();
            let _ = t.await;
        }
    }

    /// While closed, data on every connection (new or live) is held until it opens again.
    pub fn set_gate(&self, open: bool) {
        self.gate.send_replace(open);
    }
}
