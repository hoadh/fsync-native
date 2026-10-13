//! Command line shape.

use clap::{Args, Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "fsync-cli",
    about = "Headless fsync harness (JSON output for scripting)"
)]
pub struct Cli {
    #[command(flatten)]
    pub global: Global,
    #[command(subcommand)]
    pub cmd: Cmd,
}

#[derive(Args, Clone)]
pub struct Global {
    /// Directory for keys, session, history and blobs (created 0700).
    #[arg(long)]
    pub data: PathBuf,
    #[arg(long, env = "FSYNC_API", default_value = "http://127.0.0.1:8080")]
    pub api: String,
    #[arg(long, env = "FSYNC_RELAY")]
    pub relay: Option<String>,
    /// Test flag: no direct paths, everything goes through the relay.
    #[arg(long)]
    pub force_relay: bool,
    /// Allow plain http backends (dev only).
    #[arg(long)]
    pub insecure_http: bool,
    /// Device name shown to peers.
    #[arg(long, default_value = "fsync-cli")]
    pub name: String,
}

#[derive(Subcommand)]
pub enum Cmd {
    /// Sign in with an e-mail code, then create or join the account.
    Login {
        #[arg(long)]
        email: String,
        /// Code from the e-mail; prompted on stdin when absent.
        #[arg(long)]
        code: Option<String>,
        /// Only send the code and exit.
        #[arg(long)]
        send_only: bool,
    },
    #[command(subcommand)]
    Pair(PairCmd),
    Devices {
        #[arg(long)]
        json: bool,
    },
    #[command(subcommand)]
    Clip(ClipCmd),
    /// Push files or folders to one paired device.
    Send {
        /// Hex node id or device name.
        node: String,
        paths: Vec<String>,
        /// Seconds to wait for the peer to come online.
        #[arg(long, default_value_t = 30)]
        wait: u64,
    },
    /// Wait for pushed files and download them.
    Fetch {
        #[arg(long, default_value = ".")]
        out: PathBuf,
        /// Offers to accept before exiting.
        #[arg(long, default_value_t = 1)]
        count: u32,
        #[arg(long, default_value_t = 300)]
        timeout: u64,
    },
    History(HistoryArgs),
    /// Push an N MiB generated file to a device and report throughput.
    Bench {
        node: String,
        #[arg(long, default_value_t = 64)]
        mib: u64,
        #[arg(long, default_value_t = 30)]
        wait: u64,
    },
}

#[derive(Subcommand)]
pub enum PairCmd {
    /// Show a code (or QR payload) for an existing device to join.
    Offer {
        #[arg(long)]
        qr: bool,
        #[arg(long)]
        yes: bool,
        #[arg(long, default_value_t = 300)]
        timeout: u64,
    },
    /// Join a new device that shows a code or fsync:// payload.
    Join {
        input: String,
        #[arg(long)]
        yes: bool,
    },
}

#[derive(Subcommand)]
pub enum ClipCmd {
    Put {
        #[arg(long)]
        text: String,
        /// Seconds to wait for a peer to take it.
        #[arg(long, default_value_t = 30)]
        wait: u64,
    },
    Watch {
        #[arg(long)]
        json: bool,
        /// Clips to receive before exiting (0: forever).
        #[arg(long, default_value_t = 0)]
        count: u32,
        #[arg(long, default_value_t = 300)]
        timeout: u64,
    },
}

/// `history [--json] [--query Q] [--limit N]` lists; `history reset --yes` starts a new history.
#[derive(Args)]
#[command(args_conflicts_with_subcommands = true)]
pub struct HistoryArgs {
    #[command(subcommand)]
    pub cmd: Option<HistoryCmd>,
    #[arg(long)]
    pub json: bool,
    #[arg(long)]
    pub query: Option<String>,
    #[arg(long, default_value_t = 50)]
    pub limit: u32,
}

#[derive(Subcommand)]
pub enum HistoryCmd {
    /// Move the history aside and start a new one (use after a history key mismatch). Pinned
    /// trust kept in the history is lost.
    Reset {
        /// Required: confirms the history is abandoned.
        #[arg(long)]
        yes: bool,
    },
}
