# Release packaging image: the ubuntu-2204 build image (build.Dockerfile, Qt 6.2.4, glibc 2.35)
# plus pinned packaging tools. One binary built here feeds .deb, .rpm and AppImage.
# BUILD_IMAGE is the tag build-release.sh built from build.Dockerfile.
ARG BUILD_IMAGE
FROM ${BUILD_IMAGE}
USER root
ENV DEBIAN_FRONTEND=noninteractive
RUN apt-get update && apt-get install -y --no-install-recommends \
      desktop-file-utils file xz-utils zstd binutils appstream \
    && rm -rf /var/lib/apt/lists/*

# Packaging tools, exact versions (a floating tool could change what ships).
ARG CARGO_DEB_VERSION=3.8.0
ARG CARGO_GENERATE_RPM_VERSION=0.21.1
RUN CARGO_HOME=/opt/rust/cargo cargo install --locked --version ${CARGO_DEB_VERSION} cargo-deb \
    && CARGO_HOME=/opt/rust/cargo cargo install --locked --version ${CARGO_GENERATE_RPM_VERSION} \
       cargo-generate-rpm \
    && rm -rf /opt/rust/cargo/registry /opt/rust/cargo/git && chmod -R a+rX /opt/rust

# License report of the bundled crates (THIRD-PARTY-LICENSES.txt), same pin rule.
ARG CARGO_ABOUT_VERSION=0.9.3
RUN CARGO_HOME=/opt/rust/cargo cargo install --locked --version ${CARGO_ABOUT_VERSION} --features cli cargo-about \
    && for t in cargo-deb cargo-generate-rpm cargo-about; do test -x /opt/rust/cargo/bin/$t; done \
    && rm -rf /opt/rust/cargo/registry /opt/rust/cargo/git && chmod -R a+rX /opt/rust

# linuxdeploy + Qt plugin: fixed release tags, sha256 verified. Downloaded into a fresh empty dir,
# then unpacked (never run from the download dir) into /opt/linuxdeploy (no FUSE in containers).
ARG LINUXDEPLOY_TAG=1-alpha-20250213-2
ARG LINUXDEPLOY_SHA256=4648f278ab3ef31f819e67c30d50f462640e5365a77637d7e6f2ad9fd0b4522a
ARG LINUXDEPLOY_QT_TAG=1-alpha-20250213-1
ARG LINUXDEPLOY_QT_SHA256=15106be885c1c48a021198e7e1e9a48ce9d02a86dd0a1848f00bdbf3c1c92724
RUN set -eu; d=$(mktemp -d); \
    curl --proto '=https' --tlsv1.2 -fsSL -o "$d/linuxdeploy.AppImage" \
      "https://github.com/linuxdeploy/linuxdeploy/releases/download/${LINUXDEPLOY_TAG}/linuxdeploy-x86_64.AppImage"; \
    curl --proto '=https' --tlsv1.2 -fsSL -o "$d/plugin-qt.AppImage" \
      "https://github.com/linuxdeploy/linuxdeploy-plugin-qt/releases/download/${LINUXDEPLOY_QT_TAG}/linuxdeploy-plugin-qt-x86_64.AppImage"; \
    echo "${LINUXDEPLOY_SHA256}  $d/linuxdeploy.AppImage" | sha256sum -c -; \
    echo "${LINUXDEPLOY_QT_SHA256}  $d/plugin-qt.AppImage" | sha256sum -c -; \
    chmod +x "$d"/*.AppImage; \
    mkdir -p /opt/linuxdeploy/ld /opt/linuxdeploy/qt; \
    (cd /opt/linuxdeploy/ld && "$d/linuxdeploy.AppImage" --appimage-extract >/dev/null); \
    (cd /opt/linuxdeploy/qt && "$d/plugin-qt.AppImage" --appimage-extract >/dev/null); \
    ln -s /opt/linuxdeploy/ld/squashfs-root/AppRun /usr/local/bin/linuxdeploy; \
    ln -s /opt/linuxdeploy/qt/squashfs-root/AppRun /usr/local/bin/linuxdeploy-plugin-qt; \
    rm -rf "$d"; chmod -R a+rX /opt/linuxdeploy
