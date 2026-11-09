# Build image for apps/linux. BASE = ubuntu:22.04 (glibc 2.35, Qt 6.2.4, the supported floor)
# or debian:12 (glibc 2.36, Qt 6.4.2). RUST_TOOLCHAIN comes from rust-toolchain.toml
# (passed by build-in-container.sh) so the pin lives in one place.
ARG BASE=ubuntu:22.04
FROM ${BASE}
ARG RUST_TOOLCHAIN
ENV DEBIAN_FRONTEND=noninteractive
RUN apt-get update && apt-get install -y --no-install-recommends \
      build-essential perl make pkg-config cmake curl ca-certificates git lld \
      qmake6 qt6-base-dev qt6-base-dev-tools qt6-declarative-dev qt6-declarative-dev-tools \
      qml6-module-qtquick qml6-module-qtquick-controls qml6-module-qtquick-layouts \
      qml6-module-qtquick-window qml6-module-qtquick-templates qml6-module-qtqml-workerscript \
      qt6-wayland libgl1-mesa-dev libfontconfig1 libxkbcommon0 \
    && rm -rf /var/lib/apt/lists/*
# Toolchain under /opt, world-readable, so any --user uid can run cargo (and nothing needs
# to be installed at run time: components from rust-toolchain.toml are baked in).
ENV RUSTUP_HOME=/opt/rust/rustup PATH=/opt/rust/cargo/bin:$PATH
RUN test -n "${RUST_TOOLCHAIN}" \
    && curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
       | CARGO_HOME=/opt/rust/cargo sh -s -- -y --profile minimal \
         --default-toolchain "${RUST_TOOLCHAIN}" --component rustfmt,clippy --no-modify-path \
    && chmod -R a+rX /opt/rust
ENV QMAKE=/usr/bin/qmake6
