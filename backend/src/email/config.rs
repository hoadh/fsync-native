//! Email settings from the environment, validated at startup.
//!
//! `EMAIL_SENDER=log|smtp` (default `log`). With `FSYNC_ENV=production` the log sink is refused
//! unless `ALLOW_LOG_EMAIL=1` says so explicitly (a private staging that reads codes elsewhere).

use anyhow::{Context, bail};
use lettre::message::Mailbox;
use zeroize::Zeroizing;

pub const DEFAULT_SMTP_HOST: &str = "smtp.gmail.com";
pub const DEFAULT_SMTP_PORT: u16 = 587;

#[derive(Clone)]
pub struct SmtpConfig {
    pub host: String,
    /// 587 = STARTTLS (required, never downgraded); 465 = implicit TLS.
    pub port: u16,
    pub username: String,
    pub password: Zeroizing<String>,
    pub from: Mailbox,
}

impl std::fmt::Debug for SmtpConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SmtpConfig")
            .field("host", &self.host)
            .field("port", &self.port)
            .field("from", &self.from.email.to_string())
            .finish_non_exhaustive()
    }
}

#[derive(Clone, Debug)]
pub enum EmailMode {
    Log,
    Smtp(SmtpConfig),
}

fn truthy(v: Option<String>) -> bool {
    matches!(v.as_deref(), Some("1" | "true" | "TRUE" | "yes"))
}

/// `get` reads one variable (the process environment in production, a map in tests). Empty
/// values count as unset so a blank line in `.env` behaves like a missing one.
pub fn resolve(
    get: &dyn Fn(&str) -> Option<String>,
    production: bool,
) -> anyhow::Result<EmailMode> {
    let get = |k: &str| get(k).filter(|v| !v.trim().is_empty());
    match get("EMAIL_SENDER").as_deref().map(str::trim) {
        None | Some("log") => {
            if production {
                if !truthy(get("ALLOW_LOG_EMAIL")) {
                    bail!(
                        "FSYNC_ENV=production needs EMAIL_SENDER=smtp (OTP codes would never be \
                         delivered); set ALLOW_LOG_EMAIL=1 only for a private staging"
                    );
                }
                tracing::warn!("ALLOW_LOG_EMAIL=1: OTP emails are NOT delivered (log sink)");
            }
            Ok(EmailMode::Log)
        }
        Some("smtp") => smtp_from(&get).map(EmailMode::Smtp),
        Some(other) => bail!("EMAIL_SENDER must be 'log' or 'smtp', got {other:?}"),
    }
}

fn smtp_from(get: &dyn Fn(&str) -> Option<String>) -> anyhow::Result<SmtpConfig> {
    let need = |k: &str| get(k).with_context(|| format!("{k} is required when EMAIL_SENDER=smtp"));
    let port = match get("SMTP_PORT") {
        Some(p) => p
            .trim()
            .parse::<u16>()
            .ok()
            .filter(|p| *p != 0)
            .context("SMTP_PORT must be a port number (587 STARTTLS or 465 TLS)")?,
        None => DEFAULT_SMTP_PORT,
    };
    // Google shows App Passwords as four space-separated groups; the spaces are not part of it.
    let password: String = need("SMTP_PASSWORD")?
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    let from = need("SMTP_FROM")?
        .trim()
        .parse::<Mailbox>()
        .map_err(|_| anyhow::anyhow!("SMTP_FROM must look like \"fsync <sender@example.com>\""))?;
    Ok(SmtpConfig {
        host: get("SMTP_HOST").map_or_else(|| DEFAULT_SMTP_HOST.into(), |h| h.trim().into()),
        port,
        username: need("SMTP_USERNAME")?.trim().into(),
        password: Zeroizing::new(password),
        from,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn run(pairs: &[(&str, &str)], production: bool) -> anyhow::Result<EmailMode> {
        let m: HashMap<String, String> = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        resolve(&|k| m.get(k).cloned(), production)
    }

    const SMTP: [(&str, &str); 4] = [
        ("EMAIL_SENDER", "smtp"),
        ("SMTP_USERNAME", "bot@gmail.com"),
        ("SMTP_PASSWORD", "abcd efgh ijkl mnop"),
        ("SMTP_FROM", "fsync <bot@gmail.com>"),
    ];

    #[test]
    fn log_sender_is_the_dev_default_only() {
        assert!(matches!(run(&[], false).unwrap(), EmailMode::Log));
        assert!(matches!(
            run(&[("EMAIL_SENDER", "log")], false).unwrap(),
            EmailMode::Log
        ));
        let err = run(&[], true).unwrap_err().to_string();
        assert!(err.contains("EMAIL_SENDER=smtp"), "{err}");
        assert!(run(&[("EMAIL_SENDER", "log")], true).is_err());
        assert!(run(&[("ALLOW_LOG_EMAIL", "0")], true).is_err());
        assert!(run(&[("ALLOW_LOG_EMAIL", "")], true).is_err());
        assert!(matches!(
            run(&[("ALLOW_LOG_EMAIL", "1")], true).unwrap(),
            EmailMode::Log
        ));
    }

    #[test]
    fn smtp_defaults_and_password_cleanup() {
        for production in [false, true] {
            let EmailMode::Smtp(c) = run(&SMTP, production).unwrap() else {
                panic!("smtp expected")
            };
            assert_eq!((c.host.as_str(), c.port), ("smtp.gmail.com", 587));
            assert_eq!(c.password.as_str(), "abcdefghijklmnop");
            assert_eq!(c.from.email.to_string(), "bot@gmail.com");
        }
        let mut v = SMTP.to_vec();
        v.extend([("SMTP_HOST", "mail.example.com"), ("SMTP_PORT", "465")]);
        let EmailMode::Smtp(c) = run(&v, true).unwrap() else {
            panic!("smtp expected")
        };
        assert_eq!((c.host.as_str(), c.port), ("mail.example.com", 465));
    }

    #[test]
    fn smtp_rejects_incomplete_or_malformed_settings() {
        for missing in ["SMTP_USERNAME", "SMTP_PASSWORD", "SMTP_FROM"] {
            let v: Vec<_> = SMTP
                .iter()
                .copied()
                .filter(|(k, _)| *k != missing)
                .collect();
            let err = run(&v, true).unwrap_err().to_string();
            assert!(err.contains(missing), "{err}");
            // a blank value is the same as a missing one
            let mut blank = v.clone();
            blank.push((missing, "  "));
            assert!(run(&blank, false).is_err());
        }
        for bad in ["0", "70000", "smtp", "-1"] {
            let mut v = SMTP.to_vec();
            v.push(("SMTP_PORT", bad));
            assert!(run(&v, false).is_err(), "port {bad}");
        }
        let mut v = SMTP.to_vec();
        v.push(("SMTP_FROM", "not an address"));
        assert!(run(&v, false).is_err());
        assert!(run(&[("EMAIL_SENDER", "sendgrid")], false).is_err());
    }

    #[test]
    fn errors_and_debug_never_contain_the_password() {
        let EmailMode::Smtp(c) = run(&SMTP, true).unwrap() else {
            panic!("smtp expected")
        };
        assert!(!format!("{c:?}").contains("abcdefgh"));
        let mut v = SMTP.to_vec();
        v.push(("SMTP_PORT", "x"));
        assert!(!run(&v, true).unwrap_err().to_string().contains("abcd"));
    }
}
