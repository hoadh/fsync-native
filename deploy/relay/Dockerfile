# iroh-relay from crates.io, pinned to the version in Cargo.lock. Build context: deploy/relay.
FROM rust:1.99-bookworm AS build
ARG IROH_RELAY_VERSION=1.3.0
RUN cargo install iroh-relay --version "=${IROH_RELAY_VERSION}" --locked --features server --root /out

FROM debian:bookworm-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/*
COPY --from=build /out/bin/iroh-relay /usr/local/bin/iroh-relay
COPY entrypoint.sh /usr/local/bin/entrypoint.sh
EXPOSE 3340/tcp 7842/udp
HEALTHCHECK --interval=30s --timeout=3s --start-period=120s \
    CMD curl -fsS -o /dev/null http://127.0.0.1:3340/ping || exit 1
ENTRYPOINT ["/usr/local/bin/entrypoint.sh"]
