//! Endpoint construction shared by all subcommands.
use iroh::{
    address_lookup::{AddrFilter, MemoryLookup},
    endpoint::presets,
    Endpoint, EndpointAddr, EndpointId, RelayMode, RelayUrl, SecretKey,
};
use iroh_mdns_address_lookup::MdnsAddressLookup;

pub type Res<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;
pub const ALPN: &[u8] = b"spike/0";

#[derive(Default, Clone, Debug)]
pub struct Opts {
    pub relay: Option<RelayUrl>,
    pub mdns: bool,
    /// Remove IP transports: relay-only endpoint (no direct paths possible).
    pub no_ip: bool,
    /// Publish only IP addrs over mDNS (so a dialer cannot learn the relay from mDNS).
    pub mdns_ip_only: bool,
    /// Static addr hints for `dial`.
    pub hint_relay: bool,
    pub hint_ip: Vec<std::net::SocketAddr>,
    /// Feed hints through a custom lookup (simulates backend presence) instead of the dial addr.
    pub static_lookup: bool,
    pub alpns: Vec<Vec<u8>>,
}

/// Parse `--flag value` style args; returns (opts, positionals).
pub fn parse(args: &[String]) -> Res<(Opts, Vec<String>)> {
    let mut o = Opts::default();
    let mut pos = vec![];
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--relay" => { i += 1; o.relay = Some(args[i].parse()?); }
            "--mdns" => o.mdns = true,
            "--no-ip" => o.no_ip = true,
            "--mdns-ip-only" => o.mdns_ip_only = true,
            "--hint-relay" => o.hint_relay = true,
            "--hint-ip" => { i += 1; o.hint_ip.push(args[i].parse()?); }
            "--static-lookup" => o.static_lookup = true,
            a if a.starts_with("--") => return Err(format!("unknown flag {a}").into()),
            a => pos.push(a.to_string()),
        }
        i += 1;
    }
    Ok((o, pos))
}

/// Build an endpoint: no n0 presets, no DNS/pkarr, custom relay only.
pub async fn bind(o: &Opts, key: SecretKey) -> Res<(Endpoint, MemoryLookup)> {
    let lookup = MemoryLookup::new();
    let mut b = Endpoint::builder(presets::Minimal) // Minimal = crypto provider only
        .secret_key(key)
        .alpns(o.alpns.clone())
        .address_lookup(lookup.clone());
    b = match &o.relay {
        Some(u) => b.relay_mode(RelayMode::custom([u.clone()])),
        None => b.relay_mode(RelayMode::Disabled),
    };
    if o.no_ip {
        b = b.clear_ip_transports();
    }
    let ep = b.bind().await?;
    if o.mdns {
        let mut mb = MdnsAddressLookup::builder();
        if o.mdns_ip_only {
            mb = mb.addr_filter(AddrFilter::ip_only());
        }
        let mdns = mb.build(ep.id())?;
        ep.address_lookup()?.add(mdns); // can also be added after bind
    }
    Ok((ep, lookup))
}

/// Compose the address to dial from hints; optionally route via the lookup.
pub fn target(o: &Opts, id: EndpointId, lookup: &MemoryLookup) -> EndpointAddr {
    let mut addr = EndpointAddr::new(id);
    if o.hint_relay {
        if let Some(r) = &o.relay { addr = addr.with_relay_url(r.clone()); }
    }
    for ip in &o.hint_ip { addr = addr.with_ip_addr(*ip); }
    if o.static_lookup {
        lookup.add_endpoint_info(addr);
        EndpointAddr::new(id)
    } else {
        addr
    }
}
