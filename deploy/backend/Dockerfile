# Build context: repository root.  docker build -f deploy/backend/Dockerfile -t fsync-backend .
FROM rust:1.99-bookworm AS build
WORKDIR /src
COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY core ./core
COPY backend ./backend
RUN cargo build --release --locked -p fsync-backend \
    && strip target/release/fsync-backend

FROM debian:bookworm-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --uid 10001 --no-create-home fsync
COPY --from=build /src/target/release/fsync-backend /usr/local/bin/fsync-backend
USER fsync
ENV BIND_ADDR=0.0.0.0:8080
EXPOSE 8080
HEALTHCHECK --interval=30s --timeout=3s CMD curl -fsS http://127.0.0.1:8080/health || exit 1
ENTRYPOINT ["/usr/local/bin/fsync-backend"]
