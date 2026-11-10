# Runtime-only image: runs a binary built elsewhere against Fedora's Qt 6 (newest Qt line).
FROM fedora:latest
RUN dnf install -y --setopt=install_weak_deps=False \
      qt6-qtbase qt6-qtbase-gui qt6-qtdeclarative qt6-qtwayland mesa-libGL fontconfig \
      libxkbcommon dejavu-sans-fonts \
    && dnf clean all
