//! SMTP delivery of OTP codes (lettre, rustls). TLS is mandatory: STARTTLS on 587 (Gmail's
//! default), implicit TLS on 465. There is no plaintext fallback.

use super::{
    config::SmtpConfig,
    sender::{EmailError, EmailSender},
};
use crate::auth::otp::OTP_TTL_MS;
use async_trait::async_trait;
use lettre::{
    AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor,
    message::{Mailbox, MultiPart},
    transport::smtp::{
        authentication::Credentials,
        client::{Certificate, Tls, TlsParameters},
    },
};
use std::time::Duration;

/// Per socket operation (connect, each SMTP command).
const IO_TIMEOUT: Duration = Duration::from_secs(10);
/// Hard cap on one whole delivery, so a stuck server cannot hold an API request open. Below the 15 s HTTP request timeout
/// (limits.rs), otherwise that would cancel the handler first and the outcome would go unclassified.
const SEND_TIMEOUT: Duration = Duration::from_secs(12);

pub struct SmtpSender {
    transport: AsyncSmtpTransport<Tokio1Executor>,
    from: Mailbox,
}

impl SmtpSender {
    /// Trusts the public web roots (Gmail and other real providers).
    pub fn new(cfg: &SmtpConfig) -> Result<Self, EmailError> {
        Self::build(cfg, None)
    }

    /// Additionally trusts one DER root certificate: a private relay with its own CA, or the
    /// self-signed certificate of the local test server.
    pub fn with_root_cert(cfg: &SmtpConfig, der: &[u8]) -> Result<Self, EmailError> {
        Self::build(cfg, Some(der))
    }

    fn build(cfg: &SmtpConfig, root: Option<&[u8]>) -> Result<Self, EmailError> {
        let mut tls = TlsParameters::builder(cfg.host.clone());
        if let Some(der) = root {
            let cert = Certificate::from_der(der.to_vec()).map_err(|_| bad("root certificate"))?;
            tls = tls.add_root_certificate(cert);
        }
        let tls = tls.build_rustls().map_err(|_| bad("tls parameters"))?;
        let mode = if cfg.port == 465 {
            Tls::Wrapper(tls)
        } else {
            Tls::Required(tls)
        };
        let transport = AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(&cfg.host)
            .port(cfg.port)
            .tls(mode)
            .credentials(Credentials::new(
                cfg.username.clone(),
                cfg.password.to_string(),
            ))
            .timeout(Some(IO_TIMEOUT))
            .build();
        Ok(Self {
            transport,
            from: cfg.from.clone(),
        })
    }
}

fn bad(what: &str) -> EmailError {
    EmailError::failed(format!("invalid smtp {what}"))
}

/// Category and numeric status only: server texts can quote the account name or address.
fn classify(e: &lettre::transport::smtp::Error) -> EmailError {
    let status = e.status().map(|c| c.to_string()).unwrap_or_default();
    let kind = if e.is_timeout() {
        "timeout"
    } else if e.is_tls() {
        "tls"
    } else if e.is_permanent() {
        "rejected (permanent)"
    } else if e.is_transient() {
        "rejected (transient)"
    } else if e.is_response() {
        "protocol"
    } else {
        "connection"
    };
    let msg = format!("smtp {kind} {status}").trim_end().to_string();
    // Only a timeout leaves the outcome unknown (the server may have accepted the message).
    if e.is_timeout() {
        EmailError::ambiguous(msg)
    } else {
        EmailError::failed(msg)
    }
}

fn build_message(from: &Mailbox, to: &str, code: &str) -> Result<Message, EmailError> {
    let to: Mailbox = to.parse().map_err(|_| bad("recipient"))?;
    let minutes = OTP_TTL_MS / 60_000;
    let text = format!(
        "Your fsync sign-in code is {code}\n\nIt expires in {minutes} minutes. If you did not \
         request it, you can ignore this email.\n"
    );
    let html = format!(
        "<p>Your fsync sign-in code is:</p>\
         <p style=\"font-size:28px;font-weight:bold;letter-spacing:4px;font-family:monospace\">\
         {code}</p>\
         <p>It expires in {minutes} minutes. If you did not request it, you can ignore this \
         email.</p>"
    );
    Message::builder()
        .from(from.clone())
        .to(to)
        .subject("Your fsync sign-in code")
        .multipart(MultiPart::alternative_plain_html(text, html))
        .map_err(|_| bad("message"))
}

#[async_trait]
impl EmailSender for SmtpSender {
    async fn send_otp(&self, to: &str, code: &str) -> Result<(), EmailError> {
        let msg = build_message(&self.from, to, code)?;
        match tokio::time::timeout(SEND_TIMEOUT, self.transport.send(msg)).await {
            Ok(Ok(_)) => Ok(()),
            Ok(Err(e)) => Err(classify(&e)),
            Err(_) => Err(EmailError::ambiguous("smtp timeout")),
        }
    }
}
