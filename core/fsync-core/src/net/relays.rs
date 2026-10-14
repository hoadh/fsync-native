//! The relay list: parsing, validation and the `RelayMap` iroh gets.
//!
//! With several relays in the map iroh probes all of them and picks the home relay by latency
//! (and moves it when the network changes), so a second region needs no extra logic here.

use crate::error::{CoreError, Result};
use iroh::{RelayMap, RelayUrl};

/// Parses `urls`: blanks are skipped, duplicates dropped (first wins, order kept; `a` and `a/`
/// are the same relay). Only `https` unless `allow_http` (dev backends), never other schemes.
pub fn parse_relays<S: AsRef<str>>(urls: &[S], allow_http: bool) -> Result<Vec<RelayUrl>> {
    let mut out: Vec<RelayUrl> = Vec::with_capacity(urls.len());
    for raw in urls {
        let raw = raw.as_ref().trim();
        if raw.is_empty() {
            continue;
        }
        let url: RelayUrl = raw
            .parse()
            .map_err(|_| CoreError::Invalid("invalid relay url"))?;
        match url.scheme() {
            "https" => {}
            "http" if allow_http => {}
            _ => return Err(CoreError::Invalid("relay url must be https")),
        }
        if url.host_str().is_none() {
            return Err(CoreError::Invalid("invalid relay url"));
        }
        if !out.contains(&url) {
            out.push(url);
        }
    }
    Ok(out)
}

/// All relays in one map (default QUIC address-discovery ports).
pub fn relay_map(urls: &[RelayUrl]) -> RelayMap {
    urls.iter().cloned().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merges_dedups_and_keeps_order() {
        let urls = [
            "https://relay-us.example",
            "https://relay-sg.example/",
            "https://relay-us.example/",
            "  ",
        ];
        let parsed = parse_relays(&urls, false).unwrap();
        assert_eq!(
            parsed.iter().map(|u| u.to_string()).collect::<Vec<_>>(),
            ["https://relay-us.example/", "https://relay-sg.example/"]
        );
        let map = relay_map(&parsed);
        assert_eq!(map.len(), 2);
        assert!(parsed.iter().all(|u| map.contains(u)));
    }

    #[test]
    fn empty_list_is_empty_map() {
        let parsed = parse_relays::<&str>(&[], false).unwrap();
        assert!(parsed.is_empty() && relay_map(&parsed).is_empty());
    }

    #[test]
    fn plain_http_needs_dev_mode() {
        assert!(parse_relays(&["http://127.0.0.1:3340"], false).is_err());
        assert_eq!(
            parse_relays(&["http://127.0.0.1:3340"], true)
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn rejects_other_schemes_and_garbage() {
        for bad in [
            "ftp://r.example",
            "wss://r.example",
            "r.example",
            "https://",
            "file:///x",
        ] {
            assert!(parse_relays(&[bad], true).is_err(), "{bad}");
        }
    }

    #[test]
    fn one_bad_url_fails_the_whole_list() {
        assert!(parse_relays(&["https://ok.example", "nope"], false).is_err());
    }
}
