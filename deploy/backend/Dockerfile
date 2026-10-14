# Build context: repository root.  docker build -f deploy/backend/Dockerfile -t fsync-backend .
# Cross-compiles on the build host's native architecture (no QEMU: rustc segfaults under it), so an
# Apple Silicon Mac can produce linux/amd64 images:  docker buildx build --platform linux/amd64 ...
FROM --platform=$BUILDPLATFORM rust:1.99-bookworm AS build
ARG TARGETARCH
RUN apt-get update \
    && apt-get install -y --no-install-recommends gcc-x86-64-linux-gnu libc6-dev-amd64-cross \
       gcc-aarch64-linux-gnu libc6-dev-arm64-cross perl make \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /src
COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY core ./core
COPY backend ./backend
RUN set -eu; \
    case "$TARGETARCH" in \
      amd64) T=x86_64-unknown-linux-gnu; G=x86_64-linux-gnu;; \
      arm64) T=aarch64-unknown-linux-gnu; G=aarch64-linux-gnu;; \
      *) echo "unsupported TARGETARCH $TARGETARCH" >&2; exit 1;; \
    esac; \
    U="$(echo "$T" | tr 'a-z-' 'A-Z_')"; L="$(echo "$T" | tr - _)"; \
    rustup target add "$T"; \
    env "CARGO_TARGET_${U}_LINKER=$G-gcc" "CC_${L}=$G-gcc" \
      cargo build --release --locked -p fsync-backend --target "$T"; \
    "$G-strip" "target/$T/release/fsync-backend"; \
    cp "target/$T/release/fsync-backend" /fsync-backend

# No RUN steps here: they would execute under emulation when cross-building.
FROM debian:bookworm-slim
COPY --from=build /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/ca-certificates.crt
COPY --from=build /fsync-backend /usr/local/bin/fsync-backend
USER 10001:10001
ENV BIND_ADDR=0.0.0.0:8080
EXPOSE 8080
# /health also runs SELECT 1. bash /dev/tcp: the slim image has no curl.
HEALTHCHECK --interval=30s --timeout=3s --start-period=20s CMD ["bash", "-c", "exec 3<>/dev/tcp/127.0.0.1/8080 && printf 'GET /health HTTP/1.0\\r\\n\\r\\n' >&3 && grep -q ' 200 ' <&3"]
ENTRYPOINT ["/usr/local/bin/fsync-backend"]
