//! What the app hands to `Engine::publish_clip`, and what it gets back.

use fsync_proto::{ClipKind, NodeId};
use std::path::PathBuf;
use uuid::Uuid;

pub struct RepInput {
    pub mime: String,
    pub bytes: Vec<u8>,
}

pub struct FileInput {
    pub path: PathBuf,
    /// Relative destination path on the receiver (last component is the file name).
    pub rel_path: String,
}

impl FileInput {
    /// Top-level file named like the source.
    pub fn new(path: impl Into<PathBuf>) -> Self {
        let path = path.into();
        let rel_path = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        Self { path, rel_path }
    }
}

pub struct ClipInput {
    pub kind: ClipKind,
    pub reps: Vec<RepInput>,
    pub files: Vec<FileInput>,
    /// Shown in history; cut to 200 chars.
    pub preview: Option<String>,
    /// Concealed / transient content (password managers): published as asked but never
    /// written to history on this device.
    pub sensitive: bool,
}

impl ClipInput {
    /// Mark as sensitive: kept out of history.
    pub fn sensitive(mut self) -> Self {
        self.sensitive = true;
        self
    }

    pub fn text(s: &str) -> Self {
        Self {
            kind: ClipKind::Text,
            reps: vec![RepInput {
                mime: "text/plain".into(),
                bytes: s.as_bytes().to_vec(),
            }],
            files: vec![],
            preview: Some(s.chars().take(200).collect()),
            sensitive: false,
        }
    }

    pub fn files(files: Vec<FileInput>) -> Self {
        Self {
            kind: ClipKind::Files,
            reps: vec![],
            files,
            preview: None,
            sensitive: false,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct PublishOpts {
    /// Only these paired devices (default: all). "Send to..." uses this.
    pub targets: Option<Vec<NodeId>>,
    /// "Send to..." intent: receivers surface `IncomingFiles`.
    pub push: bool,
}

#[derive(Clone, Debug)]
pub struct Published {
    pub item_id: Uuid,
    pub delivered: Vec<NodeId>,
    /// Offline, unreachable within the send timeout, or it dropped the offer (rate limit, invalid).
    pub failed: Vec<NodeId>,
    /// The peer no longer trusts us (closed the connection with 403): re-pairing is needed.
    pub refused: Vec<NodeId>,
}

#[derive(Clone, Debug)]
pub enum PublishOutcome {
    /// Content equals what we just applied from a peer: nothing sent.
    Echo,
    Sent(Published),
}
