//! Records and enums: clipboard content.

use fsync_core::service as svc;
use fsync_proto as proto;

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum ClipKind {
    Text,
    Rich,
    Image,
    Files,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct RepInfo {
    pub mime: String,
    pub size: u64,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct FileInfo {
    pub file_id: u32,
    pub name: String,
    pub rel_path: String,
    pub size: u64,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct ClipItem {
    pub item_id: String,
    pub from: String,
    pub from_name: String,
    pub kind: ClipKind,
    pub ts_ms: u64,
    pub preview: Option<String>,
    pub reps: Vec<RepInfo>,
    pub files: Vec<FileInfo>,
    pub total_size: u64,
    pub push: bool,
}

#[derive(Clone, uniffi::Record)]
pub struct Rep {
    pub mime: String,
    pub data: Vec<u8>,
}

#[derive(Clone, uniffi::Record)]
pub struct LocalClip {
    pub reps: Vec<Rep>,
    pub files: Vec<String>,
    pub preview: Option<String>,
    pub sensitive: bool,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct Published {
    pub item_id: String,
    pub echo: bool,
    // Plain comments, not docs, so the generated Swift/C# bindings stay byte-identical.
    // Peers that accepted the offer before `publish_clip` returned (it waits about 1.5 s).
    pub delivered: Vec<String>,
    // Peers known not to have it (offline after the full attempt, rate limit, rejected). Peers
    // still being tried when the call returned are in neither list (the core's `pending`, left
    // out of this record so the bindings do not change): that is not a failure.
    pub failed: Vec<String>,
    /// Peers that no longer trust this device (it was removed or reset there).
    pub refused: Vec<String>,
}

impl From<proto::ClipKind> for ClipKind {
    fn from(k: proto::ClipKind) -> Self {
        match k {
            proto::ClipKind::Text => Self::Text,
            proto::ClipKind::Rich => Self::Rich,
            proto::ClipKind::Image => Self::Image,
            proto::ClipKind::Files => Self::Files,
        }
    }
}

impl From<svc::ClipItem> for ClipItem {
    fn from(c: svc::ClipItem) -> Self {
        Self {
            item_id: c.item_id,
            from: c.from,
            from_name: c.from_name,
            kind: c.kind.into(),
            ts_ms: c.ts_ms,
            preview: c.preview,
            reps: c
                .reps
                .into_iter()
                .map(|r| RepInfo {
                    mime: r.mime,
                    size: r.size,
                })
                .collect(),
            files: c
                .files
                .into_iter()
                .map(|f| FileInfo {
                    file_id: f.file_id,
                    name: f.name,
                    rel_path: f.rel_path,
                    size: f.size,
                })
                .collect(),
            total_size: c.total_size,
            push: c.push,
        }
    }
}

impl From<LocalClip> for svc::LocalClip {
    fn from(c: LocalClip) -> Self {
        Self {
            reps: c
                .reps
                .into_iter()
                .map(|r| svc::ClipRep {
                    mime: r.mime,
                    data: r.data,
                })
                .collect(),
            files: c.files,
            preview: c.preview,
            sensitive: c.sensitive,
        }
    }
}

impl From<svc::Published> for Published {
    fn from(p: svc::Published) -> Self {
        // `p.pending` is dropped on purpose (see the record).
        Self {
            item_id: p.item_id,
            echo: p.echo,
            delivered: p.delivered,
            failed: p.failed,
            refused: p.refused,
        }
    }
}
