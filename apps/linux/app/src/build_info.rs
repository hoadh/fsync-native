//! `--build-info`: JSON a packaging step can check (a release must not list `e2e-control`).

pub fn json() -> String {
    let mut features: Vec<&str> = Vec::new();
    // Features of the platform crate and of core count too (platform e2e-control is the same
    // bypass; core test-hooks / test-relay-ca are what the release guard also looks for).
    let built = fsync_linux_platform::BUILT_FEATURES.iter().chain(fsync_core::BUILT_FEATURES);
    for f in built {
        if !features.contains(f) {
            features.push(f);
        }
    }
    if cfg!(feature = "e2e-control") && !features.contains(&"e2e-control") {
        features.push("e2e-control");
    }
    if cfg!(feature = "emit-platform-linux") {
        features.push("emit-platform-linux");
    }
    serde_json::json!({
        "version": env!("CARGO_PKG_VERSION"),
        "git_sha": env!("FSYNC_GIT_SHA"),
        "features": features,
    })
    .to_string()
}

#[cfg(test)]
mod tests {
    #[test]
    fn json_has_version_sha_and_features() {
        let v: serde_json::Value = serde_json::from_str(&super::json()).unwrap();
        assert_eq!(v["version"], env!("CARGO_PKG_VERSION"));
        assert!(v["git_sha"].is_string() && v["features"].is_array());
    }
}
