# iroh-relay from crates.io, pinned to the version in Cargo.lock. Build context: deploy/relay.
# Cross-compiled natively on the build host (rustc segfaults under QEMU), see backend/Dockerfile.
FROM --platform=$BUILDPLATFORM rust:1.99-bookworm AS build
ARG TARGETARCH
ARG IROH_RELAY_VERSION=1.3.0
RUN apt-get update \
    && apt-get install -y --no-install-recommends gcc-x86-64-linux-gnu libc6-dev-amd64-cross \
       gcc-aarch64-linux-gnu libc6-dev-arm64-cross perl make \
    && rm -rf /var/lib/apt/lists/*
RUN set -eu; \
    case "$TARGETARCH" in \
      amd64) T=x86_64-unknown-linux-gnu; G=x86_64-linux-gnu;; \
      arm64) T=aarch64-unknown-linux-gnu; G=aarch64-linux-gnu;; \
      *) echo "unsupported TARGETARCH $TARGETARCH" >&2; exit 1;; \
    esac; \
    U="$(echo "$T" | tr 'a-z-' 'A-Z_')"; L="$(echo "$T" | tr - _)"; \
    rustup target add "$T"; \
    env "CARGO_TARGET_${U}_LINKER=$G-gcc" "CC_${L}=$G-gcc" \
      cargo install iroh-relay --version "=${IROH_RELAY_VERSION}" --locked --features server \
        --target "$T" --root /out; \
    "$G-strip" /out/bin/iroh-relay

# No RUN steps here: they would execute under emulation when cross-building.
FROM debian:bookworm-slim
COPY --from=build /out/bin/iroh-relay /usr/local/bin/iroh-relay
COPY entrypoint.sh /usr/local/bin/entrypoint.sh
# Non-root unless compose sets RELAY_USER=0 (QAD reading Caddy's root-only key). Ports are >1024.
USER 10001:10001
EXPOSE 3340/tcp 7842/udp
HEALTHCHECK --interval=30s --timeout=3s --start-period=120s CMD ["bash", "-c", "exec 3<>/dev/tcp/127.0.0.1/3340 && printf 'GET /ping HTTP/1.0\\r\\n\\r\\n' >&3 && grep -q ' 200 ' <&3"]
ENTRYPOINT ["/usr/local/bin/entrypoint.sh"]
