//! Connection loop: axum's `serve` cannot set hyper's header-read timeout, so we drive hyper-util
//! directly. A client that dribbles request headers (slowloris) is dropped after
//! `header_read_timeout`. WebSocket upgrades are supported.

use axum::{Router, extract::connect_info::IntoMakeServiceWithConnectInfo};
use hyper::{Request, body::Incoming, service::service_fn};
use hyper_util::{
    rt::{TokioExecutor, TokioIo, TokioTimer},
    server::{conn::auto::Builder, graceful::GracefulShutdown},
};
use std::{future::Future, net::SocketAddr, time::Duration};
use tokio::net::TcpListener;
use tower::{Service, ServiceExt};

/// How long open connections (e.g. presence sockets) get to finish after shutdown starts.
const DRAIN: Duration = Duration::from_secs(10);

/// Serve `app` on `listener` until `shutdown` resolves, then drain for up to `DRAIN`.
pub async fn serve(
    listener: TcpListener,
    app: Router,
    header_read_timeout: Duration,
    shutdown: impl Future<Output = ()>,
) -> std::io::Result<()> {
    let mut make: IntoMakeServiceWithConnectInfo<Router, SocketAddr> =
        app.into_make_service_with_connect_info::<SocketAddr>();
    let graceful = GracefulShutdown::new();
    tokio::pin!(shutdown);
    loop {
        let (stream, remote) = tokio::select! {
            accepted = listener.accept() => match accepted {
                Ok(c) => c,
                Err(e) => {
                    // EMFILE etc.: back off instead of spinning.
                    tracing::warn!(error = %e, "accept failed");
                    tokio::time::sleep(Duration::from_millis(100)).await;
                    continue;
                }
            },
            () = &mut shutdown => break,
        };
        let svc = make
            .call(remote)
            .await
            .unwrap_or_else(|never| match never {});
        let svc = service_fn(move |req: Request<Incoming>| svc.clone().oneshot(req));
        let mut builder = Builder::new(TokioExecutor::new());
        builder
            .http1()
            .timer(TokioTimer::new())
            .header_read_timeout(header_read_timeout);
        builder.http2().timer(TokioTimer::new());
        let conn = builder.serve_connection_with_upgrades(TokioIo::new(stream), svc);
        let watched = graceful.watch(conn.into_owned());
        tokio::spawn(async move {
            if let Err(e) = watched.await {
                tracing::debug!(error = %e, "connection ended with error");
            }
        });
    }
    if tokio::time::timeout(DRAIN, graceful.shutdown())
        .await
        .is_err()
    {
        tracing::warn!("shutdown drain timed out; dropping open connections");
    }
    Ok(())
}
