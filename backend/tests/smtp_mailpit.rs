//! OTP delivery over real SMTP: a Mailpit container speaks STARTTLS + AUTH like Gmail does, and
//! the assertions read what arrived through Mailpit's HTTP API.

mod common;

use common::{TestApp, mailpit};
use fsync_backend::email::{
    config::SmtpConfig,
    sender::{EmailError, EmailSender},
    smtp::SmtpSender,
};
use std::sync::Arc;

fn sender(cfg: &SmtpConfig, mp: &mailpit::Mailpit) -> SmtpSender {
    SmtpSender::with_root_cert(cfg, &mp.cert_der).unwrap()
}

/// Nothing listens on the returned port.
fn dead_config() -> SmtpConfig {
    let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    SmtpConfig {
        host: "127.0.0.1".into(),
        port: l.local_addr().unwrap().port(),
        username: "u".into(),
        password: zeroize::Zeroizing::new("pw".into()),
        from: "fsync <noreply@fsync.test>".parse().unwrap(),
    }
}

fn code_in(text: &str) -> String {
    let digits: Vec<&str> = text
        .split(|c: char| !c.is_ascii_digit())
        .filter(|w| w.len() == 6)
        .collect();
    assert_eq!(digits.len(), 1, "exactly one 6-digit code in {text:?}");
    digits[0].to_string()
}

#[tokio::test]
async fn otp_arrives_with_recipient_subject_and_code() {
    let mp = mailpit::tls().await;
    let s = sender(&mp.smtp_config(mailpit::PASSWORD), mp);
    s.send_otp("alice@example.com", "493027").await.unwrap();

    let list = mp.messages_to("alice@example.com").await;
    assert_eq!(list.len(), 1);
    let m = mp.message(list[0]["ID"].as_str().unwrap()).await;
    assert_eq!(m["To"][0]["Address"], "alice@example.com");
    assert_eq!(m["From"]["Address"], "noreply@fsync.test");
    assert_eq!(m["From"]["Name"], "fsync");
    assert_eq!(m["Subject"], "Your fsync sign-in code");
    let text = m["Text"].as_str().unwrap();
    assert!(
        text.contains("493027") && text.contains("10 minutes"),
        "{text}"
    );
    let html = m["HTML"].as_str().unwrap();
    assert!(
        html.contains("493027") && html.contains("10 minutes"),
        "{html}"
    );
}

#[tokio::test]
async fn wrong_password_is_an_error_without_secrets() {
    let mp = mailpit::tls().await;
    let s = sender(&mp.smtp_config("not-the-password"), mp);
    let EmailError(msg) = s.send_otp("bob@example.com", "111111").await.unwrap_err();
    assert!(msg.starts_with("smtp "), "{msg}");
    for secret in [
        "not-the-password",
        mailpit::USER,
        "bob@example.com",
        "111111",
    ] {
        assert!(!msg.contains(secret), "{msg}");
    }
    assert!(mp.messages_to("bob@example.com").await.is_empty());
}

#[tokio::test]
async fn refuses_a_server_without_tls() {
    let mp = mailpit::plain().await;
    let s = SmtpSender::new(&mp.smtp_config("x")).unwrap();
    let r = s.send_otp("carol@example.com", "222222").await;
    assert!(r.is_err(), "must not fall back to plaintext");
    assert!(mp.messages_to("carol@example.com").await.is_empty());
}

#[tokio::test]
async fn untrusted_certificate_is_rejected() {
    let mp = mailpit::tls().await;
    // Same server, but the client only trusts the public web roots.
    let s = SmtpSender::new(&mp.smtp_config(mailpit::PASSWORD)).unwrap();
    assert!(s.send_otp("dave@example.com", "333333").await.is_err());
    assert!(mp.messages_to("dave@example.com").await.is_empty());
}

#[tokio::test]
async fn unreachable_server_and_bad_recipient_are_errors() {
    let s = SmtpSender::new(&dead_config()).unwrap();
    let EmailError(msg) = s.send_otp("erin@example.com", "444444").await.unwrap_err();
    assert!(!msg.contains("erin@example.com"), "{msg}");
    assert!(s.send_otp("not an address", "444444").await.is_err());
}

#[tokio::test]
async fn otp_flow_over_smtp_end_to_end() {
    let mp = mailpit::tls().await;
    let s = Arc::new(sender(&mp.smtp_config(mailpit::PASSWORD), mp));
    let app = TestApp::start_with_sender(s).await;
    let to = "frank@example.com";
    assert_eq!(app.start_otp(to).await.status(), 204);

    let list = mp.messages_to(to).await;
    assert_eq!(list.len(), 1);
    let m = mp.message(list[0]["ID"].as_str().unwrap()).await;
    let code = code_in(m["Text"].as_str().unwrap());
    assert_eq!(app.verify_otp(to, &code).await.status(), 200);
}

#[tokio::test]
async fn smtp_failure_gives_a_generic_500_and_frees_the_code() {
    let s = Arc::new(SmtpSender::new(&dead_config()).unwrap());
    let app = TestApp::start_with_sender(s).await;
    let to = "gina@example.com";
    let resp = app.start_otp(to).await;
    assert_eq!(resp.status(), 500);
    let body = resp.text().await.unwrap();
    assert!(body.contains("internal error"), "{body}");
    for leak in ["smtp", "127.0.0.1", "gina@example.com", "connection"] {
        assert!(!body.contains(leak), "{body}");
    }
    // The undelivered code was removed again, so nothing live is left to guess.
    let left: (i64,) = sqlx::query_as("SELECT count(*) FROM otp_codes WHERE email = $1")
        .bind(to)
        .fetch_one(&app.pool)
        .await
        .unwrap();
    assert_eq!(left.0, 0);
}

/// A server that accepts the TCP connection and never sends the SMTP greeting must end in the
/// overall send timeout (30 s; lettre's 10 s socket timeout does not cover the greeting read),
/// not hang the request.
#[tokio::test]
async fn silent_server_ends_in_a_timeout_error() {
    let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let cfg = SmtpConfig {
        port: l.local_addr().unwrap().port(),
        ..dead_config()
    };
    let started = std::time::Instant::now();
    let err = SmtpSender::new(&cfg)
        .unwrap()
        .send_otp("alice@example.com", "493027")
        .await
        .unwrap_err();
    assert!(err.0.contains("timeout"), "{err:?}");
    assert!(started.elapsed() < std::time::Duration::from_secs(40));
    drop(l);
}
