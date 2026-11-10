//! Test-only control commands (feature `e2e-control`): parse and serialize. Never compiled
//! into release builds (compile guard in the app crate).

use super::protocol::{ProtocolError, Wire, invalid};

#[cfg(feature = "e2e-control")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum E2eCommand {
    Status,
    Login {
        email: String,
        code: Option<String>,
    },
    PairOffer,
    PairJoin {
        input: String,
    },
    PairConfirm {
        request_id: String,
        yes: bool,
    },
    Cancel,
    /// Core `clip_mode`: "auto" or "manual".
    ClipMode {
        mode: String,
    },
    /// Accepts the pushed-files offer `item_id`, or the newest pending one.
    OfferAccept {
        item_id: Option<String>,
    },
    /// History copy-back of one item.
    HistoryCopy {
        item_id: String,
    },
    /// Starts the download of a held large file clip into the inbox (no file manager is opened);
    /// it runs in the background.
    HistoryDownload {
        item_id: String,
    },
    /// Answers the DevicePicker opened by `send`: sends the staged paths to the device with
    /// this name.
    PickDevice {
        name: String,
    },
}

pub(super) fn parse_e2e(cmd: &str, w: &Wire) -> Result<E2eCommand, ProtocolError> {
    let need =
        |v: &Option<String>, n: &str| v.clone().ok_or_else(|| invalid(format!("{cmd} needs {n}")));
    Ok(match cmd {
        "status" => E2eCommand::Status,
        "login" => E2eCommand::Login {
            email: need(&w.email, "email")?,
            code: w.code.clone(),
        },
        "pair-offer" => E2eCommand::PairOffer,
        "pair-join" => E2eCommand::PairJoin {
            input: need(&w.input, "input")?,
        },
        "pair-confirm" => E2eCommand::PairConfirm {
            request_id: need(&w.request_id, "request_id")?,
            yes: w.yes.ok_or_else(|| invalid("pair-confirm needs yes"))?,
        },
        "cancel" => E2eCommand::Cancel,
        "clip-mode" => E2eCommand::ClipMode {
            mode: need(&w.input, "input")?,
        },
        "offer-accept" => E2eCommand::OfferAccept {
            item_id: w.input.clone(),
        },
        "history-copy" => E2eCommand::HistoryCopy {
            item_id: need(&w.input, "input")?,
        },
        "history-download" => E2eCommand::HistoryDownload {
            item_id: need(&w.input, "input")?,
        },
        "pick-device" => E2eCommand::PickDevice {
            name: need(&w.input, "input")?,
        },
        other => return Err(invalid(format!("unknown command {other:?}"))),
    })
}

pub(super) fn fill_e2e(w: &mut Wire, c: &E2eCommand) {
    match c {
        E2eCommand::Status => w.cmd = "status".into(),
        E2eCommand::Login { email, code } => {
            w.cmd = "login".into();
            w.email = Some(email.clone());
            w.code = code.clone();
        }
        E2eCommand::PairOffer => w.cmd = "pair-offer".into(),
        E2eCommand::PairJoin { input } => {
            w.cmd = "pair-join".into();
            w.input = Some(input.clone());
        }
        E2eCommand::PairConfirm { request_id, yes } => {
            w.cmd = "pair-confirm".into();
            w.request_id = Some(request_id.clone());
            w.yes = Some(*yes);
        }
        E2eCommand::Cancel => w.cmd = "cancel".into(),
        E2eCommand::ClipMode { mode } => {
            w.cmd = "clip-mode".into();
            w.input = Some(mode.clone());
        }
        E2eCommand::OfferAccept { item_id } => {
            w.cmd = "offer-accept".into();
            w.input = item_id.clone();
        }
        E2eCommand::HistoryCopy { item_id } => {
            w.cmd = "history-copy".into();
            w.input = Some(item_id.clone());
        }
        E2eCommand::HistoryDownload { item_id } => {
            w.cmd = "history-download".into();
            w.input = Some(item_id.clone());
        }
        E2eCommand::PickDevice { name } => {
            w.cmd = "pick-device".into();
            w.input = Some(name.clone());
        }
    }
}
