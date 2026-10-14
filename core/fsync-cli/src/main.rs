//! fsync-cli: headless harness for pairing, clip and file transfer tests. Machine-readable
//! output is one JSON object per line on stdout; diagnostics go to stderr.

mod args;
mod cmd_account;
mod cmd_clip;
mod cmd_files;
mod cmd_pair;
mod cmd_trust;
mod ctx;
mod events;
mod session;

use anyhow::Result;
use args::{AccountCmd, Cli, ClipCmd, Cmd, HistoryArgs, HistoryCmd, PairCmd, TrustCmd};
use clap::Parser;

fn run(cli: Cli) -> Result<()> {
    let g = &cli.global;
    if let Cmd::Login {
        email,
        code,
        send_only,
    } = cli.cmd
    {
        return cmd_account::login(g, &email, code, send_only);
    }
    if let Cmd::History(HistoryArgs {
        cmd: Some(HistoryCmd::Reset { yes }),
        ..
    }) = cli.cmd
    {
        // Must not open the service: it refuses to start on a key mismatch.
        return cmd_account::history_reset(g, yes);
    }
    let c = ctx::open(g)?;
    match cli.cmd {
        Cmd::Login { .. } => Ok(()),
        Cmd::Pair(PairCmd::Offer { qr, yes, timeout }) => cmd_pair::offer(&c, qr, yes, timeout),
        Cmd::Pair(PairCmd::Join { input, yes }) => cmd_pair::join(&c, &input, yes),
        Cmd::Account(AccountCmd::ResetTrust { yes }) => cmd_trust::reset(&c, yes),
        Cmd::Trust(TrustCmd::Resets { json }) => cmd_trust::resets(&c, json),
        Cmd::Trust(TrustCmd::Confirm { id }) => cmd_trust::confirm(&c, &id),
        Cmd::Trust(TrustCmd::Decline { id }) => cmd_trust::decline(&c, &id),
        Cmd::Devices { json } => cmd_account::devices(&c, json),
        Cmd::Clip(ClipCmd::Put { text, wait }) => cmd_clip::put(&c, &text, wait),
        Cmd::Clip(ClipCmd::Watch {
            json,
            count,
            timeout,
        }) => cmd_clip::watch(&c, json, count, timeout),
        Cmd::Send { node, paths, wait } => cmd_files::send(&c, &node, paths, wait),
        Cmd::Fetch {
            out,
            count,
            timeout,
        } => cmd_files::fetch(&c, &out, count, timeout),
        Cmd::History(h) => cmd_account::history(&c, h.json, h.query, h.limit),
        Cmd::Bench { node, mib, wait } => cmd_files::bench(&c, &node, mib, wait),
    }
}

fn main() {
    if let Err(e) = run(Cli::parse()) {
        eprintln!("error: {e:#}");
        std::process::exit(1);
    }
}
