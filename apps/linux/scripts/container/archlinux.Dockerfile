# Runtime-only image: runs a binary built elsewhere against Arch's Qt 6 (rolling, newest).
FROM archlinux:latest
RUN pacman -Syu --noconfirm --needed qt6-base qt6-declarative qt6-wayland ttf-dejavu \
    && pacman -Scc --noconfirm
