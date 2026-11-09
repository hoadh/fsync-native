#!/usr/bin/env bash
# Headless display sessions for clipboard tests, so the user's real clipboard is never touched:
# a nested headless sway (own XDG_RUNTIME_DIR + WAYLAND_DISPLAY) and an Xvfb (own DISPLAY).
# Sourced. Callers keep the PIDs it appends to HS_PIDS and call hs_stop_all on exit.
# Every command run against a session must use hs_wayland_env / hs_x11_env (they unset the
# other session's variables), never the ambient WAYLAND_DISPLAY / DISPLAY.

HS_PIDS=()
HS_DIRS=()

hs_prereq() { # exits 2 with an install hint when a tool is missing (not a test failure)
  local t missing=()
  for t in sway swaymsg wl-copy wl-paste Xvfb xclip dbus-run-session; do
    command -v "$t" >/dev/null 2>&1 || missing+=("$t")
  done
  if [[ ${#missing[@]} -gt 0 ]]; then
    echo "[e2e] missing prerequisites: ${missing[*]}" >&2
    echo "[e2e] Arch: pacman -S sway wl-clipboard xorg-server-xvfb xclip dbus" >&2
    echo "[e2e] Debian/Ubuntu: apt install sway wl-clipboard xvfb xclip dbus-daemon" >&2
    exit 2
  fi
}

# hs_start_sway NAME: sets HS_<NAME>_RT (runtime dir) and HS_<NAME>_WL (socket name).
hs_start_sway() {
  local name="$1" rt wl i
  rt="$(mktemp -d /tmp/fsync-hs.XXXXXX)"; chmod 700 "$rt"; HS_DIRS+=("$rt")
  cat >"$rt/sway.conf" <<'CONF'
# Minimal: no bar, no keybindings, no autostart. Only the compositor protocols matter.
default_border none
CONF
  env -u DISPLAY -u WAYLAND_DISPLAY -u SWAYSOCK -u HYPRLAND_INSTANCE_SIGNATURE \
    XDG_RUNTIME_DIR="$rt" WLR_BACKENDS=headless WLR_LIBINPUT_NO_DEVICES=1 WLR_RENDERER=pixman \
    sway -c "$rt/sway.conf" >"$rt/sway.log" 2>&1 &
  HS_PIDS+=($!)
  for i in $(seq 1 100); do
    wl="$(cd "$rt" && ls wayland-* 2>/dev/null | grep -v '\.lock$' | head -1 || true)"
    [[ -n "$wl" && -S "$rt/$wl" ]] && break
    sleep 0.1
  done
  [[ -n "${wl:-}" && -S "$rt/$wl" ]] || { echo "[e2e] headless sway did not start:" >&2; tail -5 "$rt/sway.log" >&2; return 1; }
  printf -v "HS_${name}_RT" %s "$rt"
  printf -v "HS_${name}_WL" %s "$wl"
}

# hs_start_xvfb NAME: sets HS_<NAME>_DISPLAY (":NN").
hs_start_xvfb() {
  local name="$1" d fifo
  d="$(mktemp -d /tmp/fsync-hs.XXXXXX)"; chmod 700 "$d"; HS_DIRS+=("$d")
  fifo="$d/displayfd"; mkfifo "$fifo"
  Xvfb -displayfd 3 -nolisten tcp -screen 0 800x600x24 3>"$fifo" >"$d/xvfb.log" 2>&1 &
  HS_PIDS+=($!)
  local num=""
  read -r -t 15 num <"$fifo" || true
  [[ -n "$num" ]] || { echo "[e2e] Xvfb did not start" >&2; return 1; }
  printf -v "HS_${name}_DISPLAY" %s ":$num"
}

# Env prefixes for `env ... cmd`: only the named session is visible to the command.
hs_wayland_env() { # NAME
  local rt="HS_$1_RT" wl="HS_$1_WL"
  echo "XDG_RUNTIME_DIR=${!rt} WAYLAND_DISPLAY=${!wl}"
}
hs_x11_env() { # NAME
  local d="HS_$1_DISPLAY"
  echo "DISPLAY=${!d}"
}

hs_stop_all() {
  local p d
  for p in ${HS_PIDS[@]+"${HS_PIDS[@]}"}; do kill "$p" 2>/dev/null || true; done
  for p in ${HS_PIDS[@]+"${HS_PIDS[@]}"}; do wait "$p" 2>/dev/null || true; done
  # Never let a failed rm abort the caller's cleanup (set -e): the rest must still run.
  for d in ${HS_DIRS[@]+"${HS_DIRS[@]}"}; do rm -rf "$d" 2>/dev/null || true; done
  HS_PIDS=(); HS_DIRS=()
}
