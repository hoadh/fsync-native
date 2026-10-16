#!/usr/bin/env bash
# shellcheck disable=SC2034  # globals are consumed by the sourcing scripts
# Network namespace topology for the netns matrix. Sourced inside the privileged runner container
# (never on the host). All names are fixed; everything lives in the container's own namespaces.
#
#   root ns: br-wan 100.64.0.1/24 (backend)    ns relay: wan 100.64.0.2 (relay server)
#   ns r-lan : wan .10, br-lan 192.168.10.1/24 -> h-la .10, h-lb .11
#   ns r-a   : wan .21, br-lan 10.1.0.1/24     -> h-na .10
#   ns r-b   : wan .22, br-lan 10.2.0.1/24     -> h-nb .10

WAN_NET=100.64.0.0/24
ALL_NS="relay r-lan r-a r-b h-la h-lb h-na h-nb"

nsx() { local ns="$1"; shift; ip netns exec "$ns" "$@"; }

topo_destroy() { # idempotent; only touches this container
  local ns
  for ns in $ALL_NS; do ip netns del "$ns" 2>/dev/null || true; done
  ip link del br-wan 2>/dev/null || true
}

# wan_attach NS IFNAME-IN-ROOT IP
wan_attach() {
  ip link add "$2" master br-wan type veth peer name wan netns "$1"
  ip link set "$2" up
  nsx "$1" ip addr add "$3/24" dev wan
  nsx "$1" ip link set wan up
  nsx "$1" ip link set lo up
}

# lan_build ROUTER LAN_PREFIX(e.g. 10.1.0) HOSTNS:LASTOCTET...
lan_build() {
  local r="$1" pfx="$2"; shift 2
  nsx "$r" ip link add br-lan type bridge
  # no IGMP snooping: mDNS (224.0.0.251) must be flooded to every port
  echo 0 | nsx "$r" tee /sys/class/net/br-lan/bridge/multicast_snooping >/dev/null 2>&1 || true
  nsx "$r" ip addr add "$pfx.1/24" dev br-lan
  nsx "$r" ip link set br-lan up
  nsx "$r" sysctl -qw net.ipv4.ip_forward=1
  local h ns oct
  for h in "$@"; do
    ns="${h%%:*}"; oct="${h##*:}"
    ip link add "l-$ns" netns "$r" type veth peer name eth0 netns "$ns"
    nsx "$r" ip link set "l-$ns" master br-lan up
    nsx "$ns" ip addr add "$pfx.$oct/24" dev eth0
    nsx "$ns" ip link set eth0 up
    nsx "$ns" ip link set lo up
    nsx "$ns" ip route add default via "$pfx.1"
  done
}

topo_build() {
  topo_destroy
  local ns
  for ns in $ALL_NS; do ip netns add "$ns"; done
  ip link add br-wan type bridge
  ip addr add 100.64.0.1/24 dev br-wan
  ip link set br-wan up
  wan_attach relay vw-relay 100.64.0.2
  wan_attach r-lan vw-rlan 100.64.0.10
  wan_attach r-a vw-ra 100.64.0.21
  wan_attach r-b vw-rb 100.64.0.22
  lan_build r-lan 192.168.10 h-la:10 h-lb:11
  lan_build r-a 10.1.0 h-na:10
  lan_build r-b 10.2.0 h-nb:10
  # The relay process listens on loopback only (spike server): expose its port on the WAN address.
  nsx relay sysctl -qw net.ipv4.conf.all.route_localnet=1 net.ipv4.conf.wan.route_localnet=1
  nat_mode r-lan masq
  nat_mode r-a masq
  nat_mode r-b masq
}

# site_routes add|del: make the two NAT sites' private LANs routable across the WAN (no translation).
# Used only by the `stateful` mode; every other mode on r-a/r-b removes the routes again.
site_routes() {
  local op="$1"
  ip route "$op" 10.1.0.0/24 via 100.64.0.21 2>/dev/null || true
  ip route "$op" 10.2.0.0/24 via 100.64.0.22 2>/dev/null || true
  nsx relay ip route "$op" 10.1.0.0/24 via 100.64.0.21 2>/dev/null || true
  nsx relay ip route "$op" 10.2.0.0/24 via 100.64.0.22 2>/dev/null || true
  nsx r-a ip route "$op" 10.2.0.0/24 via 100.64.0.22 2>/dev/null || true
  nsx r-b ip route "$op" 10.1.0.0/24 via 100.64.0.21 2>/dev/null || true
}

# nat_mode ROUTER masq|full-cone|symmetric|stateful: flush and reapply.
# stateful = no translation, inbound only for established flows (a firewall, not a NAT): the one
# hole-punch shape that works without relay QUIC address discovery (see scenarios/n3-*.sh).
nat_mode() {
  local r="$1" mode="$2" wanip host
  if [[ "$mode" != stateful && "$r" != r-lan ]]; then site_routes del; fi
  wanip="$(nsx "$r" ip -4 -o addr show dev wan | sed -n 's/.* inet \([0-9.]*\)\/.*/\1/p')"
  case "$r" in r-a) host=10.1.0.10;; r-b) host=10.2.0.10;; *) host="";; esac
  nsx "$r" iptables -t nat -F
  nsx "$r" iptables -F FORWARD
  case "$mode" in
    masq) nsx "$r" iptables -t nat -A POSTROUTING -o wan -j MASQUERADE;;
    full-cone) # endpoint-independent mapping and filtering: same port out, anything in goes to the host
      nsx "$r" iptables -t nat -A POSTROUTING -o wan -j SNAT --to-source "$wanip"
      nsx "$r" iptables -t nat -A PREROUTING -i wan -j DNAT --to-destination "$host";;
    symmetric) # new random source port per flow; unsolicited inbound dropped
      nsx "$r" iptables -t nat -A POSTROUTING -o wan -j MASQUERADE --random-fully
      nsx "$r" iptables -A FORWARD -i wan -m conntrack --ctstate ESTABLISHED,RELATED -j ACCEPT
      nsx "$r" iptables -A FORWARD -i wan -j DROP;;
    stateful)
      site_routes add
      nsx "$r" iptables -A FORWARD -i wan -m conntrack --ctstate ESTABLISHED,RELATED -j ACCEPT
      nsx "$r" iptables -A FORWARD -i wan -j DROP;;
    *) fail "nat_mode: unknown mode $mode";;
  esac
}

# hosts_reset: put every host back on its built address + default route (N6 moves h-la's address).
# Only touches a host whose eth0 differs from the built state.
hosts_reset() {
  local h ns ip
  for h in h-la:192.168.10.10 h-lb:192.168.10.11 h-na:10.1.0.10 h-nb:10.2.0.10; do
    ns="${h%%:*}"; ip="${h#*:}"
    if [[ "$(nsx "$ns" ip -4 -o addr show dev eth0 | awk '{print $4}')" != "$ip/24" ]]; then
      log "reset address of $ns to $ip"
      nsx "$ns" ip addr flush dev eth0
      nsx "$ns" ip addr add "$ip/24" dev eth0
      nsx "$ns" ip route replace default via "${ip%.*}.1"
    fi
  done
}

wan_down() { nsx "$1" ip link set wan down; }
wan_up() { nsx "$1" ip link set wan up; }

# if_bytes NS IFACE: rx+tx bytes.
if_bytes() {
  nsx "$1" cat "/sys/class/net/$2/statistics/rx_bytes" "/sys/class/net/$2/statistics/tx_bytes" \
    | awk '{s+=$1} END {print s+0}'
}

# counters_snap FILE: relay wan, A router wan, B router wan, A host eth0 (needs A_NS A_RTR B_RTR).
counters_snap() {
  printf '%s %s %s %s\n' "$(if_bytes relay wan)" "$(if_bytes "$A_RTR" wan)" \
    "$(if_bytes "$B_RTR" wan)" "$(if_bytes "$A_NS" eth0)" >"$1"
}
