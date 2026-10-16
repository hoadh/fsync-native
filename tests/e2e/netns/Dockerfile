# Runner image for the netns matrix: Rust toolchain (matches rust-toolchain.toml) + netns tooling.
# No postgres here: the matrix starts a separate postgres container on the same docker network.
FROM rust:1.99-bookworm
RUN apt-get update \
 && apt-get install -y --no-install-recommends iproute2 iptables python3 curl ca-certificates procps \
 && rm -rf /var/lib/apt/lists/*
ENV CARGO_HOME=/cargo CARGO_BUILD_JOBS=6 RUSTUP_TOOLCHAIN=1.99.0
LABEL fsync.e2e=netns
