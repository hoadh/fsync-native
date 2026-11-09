//! Outbound pipeline: a `ClipChange` becomes a `LocalClip` or a `Skip`. Order (ADR 013
//! decision 4, skip before read): own marker and hints on names, source app, then the MIME plan;
//! bytes are read only for allowed MIME types, with a size cap and a timeout. Never logs
//! content or file names.

use super::SyncSettings;
use crate::clipboard::denylist_builtin::effective;
use crate::clipboard::echo::WriteGuard;
use crate::clipboard::filters::{Skip, classify_mimes, classify_source};
use crate::clipboard::formats::{FormatSkip, RawReps, ReadPlan, build_reps, parse_uri_list};
use crate::clipboard::path_denylist::PathDenylist;
use crate::clipboard::{ClipChange, ReadError};
use fsync_core::service::{LocalClip, Published};
use std::time::Duration;

/// Per-representation read budget.
pub const READ_TIMEOUT: Duration = Duration::from_secs(2);
/// A uri-list of 10 000 files is well below this.
const URI_LIST_CAP: usize = 1 << 20;

pub struct OutCtx<'a> {
    pub settings: &'a SyncSettings,
    pub paths: &'a PathDenylist,
    pub guard: &'a WriteGuard,
}

fn skip_of(e: ReadError) -> Skip {
    match e {
        ReadError::TooLarge => Skip::TooLarge,
        ReadError::Timeout | ReadError::Stale | ReadError::Io(_) => Skip::Unreadable,
    }
}

fn cap_of(settings: &SyncSettings) -> usize {
    usize::try_from(settings.max_clip_bytes).unwrap_or(usize::MAX)
}

/// Local, allowed files of a file-manager copy; `NoUsableFiles` when none survive.
fn collect_files(change: &ClipChange, plan: &ReadPlan, ctx: &OutCtx) -> Result<Vec<String>, Skip> {
    let (mime, gnome) = match (&plan.uri_list, &plan.gnome_files) {
        (Some(m), _) => (m, false),
        (None, Some(m)) => (m, true),
        (None, None) => return Err(Skip::NoContent),
    };
    let raw = change
        .reader
        .read(mime, URI_LIST_CAP, READ_TIMEOUT)
        .map_err(skip_of)?;
    let mut files = Vec::new();
    for p in parse_uri_list(&raw, gnome) {
        match ctx.paths.check(&p) {
            Ok(canon) => files.push(canon.to_string_lossy().into_owned()),
            Err(_) => tracing::debug!("clipboard: a copied path was refused"),
        }
    }
    if files.is_empty() {
        Err(Skip::NoUsableFiles)
    } else {
        Ok(files)
    }
}

fn collect_reps(change: &ClipChange, plan: &ReadPlan, ctx: &OutCtx) -> Result<RawReps, Skip> {
    let mut left = cap_of(ctx.settings);
    let mut read = |mime: &Option<String>| -> Result<Option<Vec<u8>>, Skip> {
        let Some(mime) = mime else {
            return Ok(None);
        };
        let data = change
            .reader
            .read(mime, left, READ_TIMEOUT)
            .map_err(skip_of)?;
        left = left.saturating_sub(data.len());
        Ok(Some(data))
    };
    Ok(RawReps {
        text: read(&plan.text)?,
        html: read(&plan.html)?,
        png: read(&plan.png)?,
    })
}

pub fn collect(change: &ClipChange, ctx: &OutCtx) -> Result<LocalClip, Skip> {
    classify_mimes(&change.mimes)?;
    classify_source(change.source.as_ref(), &effective(&ctx.settings.denylist))?;
    let plan = ReadPlan::from_mimes(&change.mimes);
    // A file-manager copy also carries the file names as text: it is a files clip.
    if plan.wants_files() {
        let files = collect_files(change, &plan, ctx)?;
        return Ok(LocalClip {
            reps: Vec::new(),
            files,
            preview: None,
            sensitive: false,
        });
    }
    if !plan.wants_content() {
        return Err(Skip::NoContent);
    }
    let raw = collect_reps(change, &plan, ctx)?;
    let reps = build_reps(raw).map_err(|e| match e {
        FormatSkip::NoContent => Skip::NoContent,
        FormatSkip::BadImage => Skip::BadImage,
    })?;
    let parts: Vec<(&str, &[u8])> = reps
        .iter()
        .map(|r| (r.mime.as_str(), r.data.as_slice()))
        .collect();
    if ctx.guard.is_echo(&parts) {
        return Err(Skip::OwnEcho);
    }
    Ok(LocalClip {
        reps,
        files: Vec::new(),
        preview: None,
        sensitive: false,
    })
}

/// One sentence for the status page.
pub fn describe(p: &Published) -> Option<String> {
    if p.echo {
        return None;
    }
    let n = p.delivered.len();
    Some(if n > 0 {
        format!("Sent to {n} device{}.", if n == 1 { "" } else { "s" })
    } else if !p.refused.is_empty() && p.failed.is_empty() {
        "Not sent: the other devices no longer trust this one.".to_owned()
    } else {
        "Not sent: no other device is reachable.".to_owned()
    })
}
