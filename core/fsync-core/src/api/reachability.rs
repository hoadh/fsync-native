//! Tells "the backend did not answer" apart from "something answered wrongly".
//!
//! Only the first may let a device keep working from its cached device list: a TLS or
//! certificate failure points to a forged or intercepted backend, which must not be treated as
//! mere downtime.

use std::{error::Error, io::ErrorKind};

/// True for timeouts and for connect failures whose cause is the network itself (refused, reset,
/// unreachable, DNS). Anything about TLS, certificates or the response is `false`.
pub(super) fn is_unreachable(e: &reqwest::Error) -> bool {
    if e.is_timeout() {
        return true;
    }
    if !e.is_connect() {
        return false;
    }
    let mut cause = e.source();
    while let Some(c) = cause {
        if let Some(io) = c.downcast_ref::<std::io::Error>()
            && matches!(
                io.kind(),
                ErrorKind::ConnectionRefused
                    | ErrorKind::ConnectionReset
                    | ErrorKind::ConnectionAborted
                    | ErrorKind::NotConnected
                    | ErrorKind::TimedOut
                    | ErrorKind::HostUnreachable
                    | ErrorKind::NetworkUnreachable
                    | ErrorKind::NetworkDown
                    | ErrorKind::AddrNotAvailable
            )
        {
            return true;
        }
        // hyper-util reports a failed lookup as a plain "dns error" without a typed io kind.
        if c.to_string().starts_with("dns error") {
            return true;
        }
        cause = c.source();
    }
    false
}

#[cfg(test)]
mod tests {
    use crate::{api::tls::client_config, error::CoreError};
    use tokio::{io::AsyncWriteExt, net::TcpListener};

    fn client() -> reqwest::Client {
        reqwest::Client::builder()
            .use_preconfigured_tls((*client_config().unwrap()).clone())
            .timeout(std::time::Duration::from_secs(5))
            .build()
            .unwrap()
    }

    async fn classify(url: &str) -> CoreError {
        let r = client().get(url).send().await;
        super::super::check::check(r).await.unwrap_err()
    }

    #[tokio::test]
    async fn refused_connection_is_unreachable() {
        let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/", l.local_addr().unwrap());
        drop(l);
        assert!(matches!(classify(&url).await, CoreError::Unreachable(_)));
    }

    #[tokio::test]
    async fn unresolvable_host_is_unreachable() {
        let e = classify("http://does-not-exist.invalid/").await;
        assert!(matches!(e, CoreError::Unreachable(_)), "{e:?}");
    }

    /// A server that answers TLS with garbage: a handshake failure, not downtime.
    #[tokio::test]
    async fn broken_tls_handshake_is_not_unreachable() {
        let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("https://{}/", l.local_addr().unwrap());
        tokio::spawn(async move {
            while let Ok((mut s, _)) = l.accept().await {
                let _ = s.write_all(b"HTTP/1.1 200 OK\r\n\r\nnot tls at all").await;
            }
        });
        let e = classify(&url).await;
        assert!(matches!(e, CoreError::Http(_)), "{e:?}");
    }

    /// Silence past the client timeout is a timeout, hence unreachable.
    #[tokio::test]
    async fn no_answer_is_a_timeout_and_unreachable() {
        let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/", l.local_addr().unwrap());
        let _hold = tokio::spawn(async move {
            let mut keep = vec![];
            while let Ok(c) = l.accept().await {
                keep.push(c);
            }
        });
        let c = reqwest::Client::builder()
            .timeout(std::time::Duration::from_millis(300))
            .build()
            .unwrap();
        let e = super::super::check::check(c.get(&url).send().await)
            .await
            .unwrap_err();
        assert!(matches!(e, CoreError::Unreachable(_)), "{e:?}");
    }
}
