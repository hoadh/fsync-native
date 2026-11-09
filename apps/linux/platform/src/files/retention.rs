//! Inbox retention (ADR 013 d13): it follows the history settings. Item directories older than
//! the retention days go first, then the oldest ones until the inbox fits `retention_bytes`.
//! The item on the clipboard right now is never removed. Only real directories with a valid item
//! id are touched; links and strangers are skipped, and nothing is ever followed through a link.

use super::inbox::{DownloadGuard, EXPLICIT, valid_item_id};
use std::path::Path;
use std::time::{Duration, SystemTime};

#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub max_age: Duration,
    pub max_bytes: u64,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Report {
    pub removed: usize,
    pub freed: u64,
}

struct Entry {
    name: String,
    /// The user downloaded it on purpose: only the age limit applies.
    explicit: bool,
    modified: SystemTime,
    size: u64,
}

const MAX_DEPTH: u32 = 64;

/// Bytes under `dir`, counting links as themselves (never descending into them).
fn tree_size(dir: &Path, depth: u32) -> u64 {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return 0;
    };
    let mut total = 0u64;
    for e in rd.flatten() {
        let Ok(m) = std::fs::symlink_metadata(e.path()) else {
            continue;
        };
        if m.is_dir() {
            if depth < MAX_DEPTH {
                total = total.saturating_add(tree_size(&e.path(), depth + 1));
            }
        } else {
            total = total.saturating_add(m.len());
        }
    }
    total
}

fn scan(root: &Path) -> std::io::Result<Vec<Entry>> {
    let mut out = Vec::new();
    for e in std::fs::read_dir(root)?.flatten() {
        let Some(name) = e.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        let Ok(m) = std::fs::symlink_metadata(e.path()) else {
            continue;
        };
        if !valid_item_id(&name) || !m.is_dir() {
            continue;
        }
        out.push(Entry {
            explicit: std::fs::symlink_metadata(e.path().join(EXPLICIT)).is_ok(),
            size: tree_size(&e.path(), 0),
            modified: m.modified().unwrap_or(SystemTime::UNIX_EPOCH),
            name,
        });
    }
    Ok(out)
}

/// Applies `limits` to `root`; `keep` names the items that must stay (the clipboard item);
/// `claim` takes the download claim of an item right before it is removed. Explicitly
/// downloaded items count towards neither the size budget nor its removals (ADR 014).
/// A missing root is not an error (nothing was ever received).
pub fn sweep(
    root: &Path,
    limits: Limits,
    keep: &[&str],
    claim: &dyn Fn(&str) -> Option<DownloadGuard>,
    now: SystemTime,
) -> std::io::Result<Report> {
    let mut entries = match scan(root) {
        Ok(v) => v,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Report::default()),
        Err(e) => return Err(e),
    };
    entries.sort_by_key(|e| e.modified);
    let mut report = Report::default();
    let mut remaining: Vec<Entry> = Vec::new();
    for e in entries {
        let old = now
            .duration_since(e.modified)
            .is_ok_and(|age| age > limits.max_age);
        if old && !keep.contains(&e.name.as_str()) && remove(root, &e, claim, &mut report) {
            continue;
        }
        remaining.push(e);
    }
    let mut total: u64 = remaining
        .iter()
        .filter(|e| !e.explicit)
        .map(|e| e.size)
        .sum();
    for e in remaining.into_iter().filter(|e| !e.explicit) {
        if total <= limits.max_bytes {
            break;
        }
        if !keep.contains(&e.name.as_str()) && remove(root, &e, claim, &mut report) {
            total = total.saturating_sub(e.size);
        }
    }
    Ok(report)
}

/// Removes one item while holding its download claim: a download that started after the scan
/// (or is running) makes the claim fail and the item is skipped.
fn remove(
    root: &Path,
    e: &Entry,
    claim: &dyn Fn(&str) -> Option<DownloadGuard>,
    report: &mut Report,
) -> bool {
    let Some(_claim) = claim(&e.name) else {
        return false;
    };
    // remove_dir_all unlinks links instead of following them.
    match std::fs::remove_dir_all(root.join(&e.name)) {
        Ok(()) => {
            report.removed += 1;
            report.freed = report.freed.saturating_add(e.size);
            true
        }
        Err(err) => {
            tracing::warn!("inbox retention: cannot remove an item: {err}");
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// A claim that always succeeds (nothing is downloading).
    fn free_claim() -> impl Fn(&str) -> Option<DownloadGuard> {
        let inbox = super::super::inbox::Inbox::new(PathBuf::new());
        move |id| inbox.begin_download(id).ok()
    }

    const DAY: Duration = Duration::from_secs(86_400);

    fn item(root: &Path, id: &str, bytes: usize, age_days: u64, now: SystemTime) {
        let d = root.join(id);
        std::fs::create_dir_all(&d).expect("mkdir");
        std::fs::write(d.join("f"), vec![0u8; bytes]).expect("write");
        let f = std::fs::File::open(&d).expect("open");
        f.set_modified(now - DAY * age_days as u32).expect("mtime");
    }

    fn limits(days: u32, bytes: u64) -> Limits {
        Limits {
            max_age: DAY * days,
            max_bytes: bytes,
        }
    }

    #[test]
    fn age_then_size_oldest_first_and_current_item_kept() {
        let d = tempfile::tempdir().expect("tmp");
        let now = SystemTime::now();
        item(d.path(), "aa", 100, 40, now);
        item(d.path(), "bb", 100, 5, now);
        item(d.path(), "cc", 100, 3, now);
        item(d.path(), "dd", 100, 1, now);
        // 30 days: "aa" goes by age. 150 bytes: "bb" (oldest left) then "cc" go by size.
        let r = sweep(d.path(), limits(30, 150), &[], &free_claim(), now).expect("sweep");
        assert_eq!(
            r,
            Report {
                removed: 3,
                freed: 300
            }
        );
        assert!(d.path().join("dd").exists());
        assert!(!d.path().join("bb").exists() && !d.path().join("cc").exists());

        // The item on the clipboard stays even when it is old and over the budget.
        item(d.path(), "ee", 500, 90, now);
        let r = sweep(d.path(), limits(30, 10), &["ee"], &free_claim(), now).expect("sweep");
        assert_eq!(r.removed, 1, "only dd");
        assert!(d.path().join("ee").exists());
    }

    #[test]
    fn links_strangers_and_a_missing_root_are_left_alone() {
        let d = tempfile::tempdir().expect("tmp");
        let now = SystemTime::now();
        assert_eq!(
            sweep(
                &d.path().join("none"),
                limits(1, 1),
                &[],
                &free_claim(),
                now
            )
            .expect("missing"),
            Report::default()
        );
        let root = d.path().join("inbox");
        std::fs::create_dir(&root).expect("root");
        let victim = d.path().join("victim");
        std::fs::create_dir(&victim).expect("victim");
        std::fs::write(victim.join("keep"), vec![0u8; 1000]).expect("keep");
        std::os::unix::fs::symlink(&victim, root.join("ab")).expect("link");
        std::fs::create_dir(root.join("Not-An-Item")).expect("stranger");
        std::fs::write(root.join("cd"), b"file").expect("plain file");
        item(&root, "ef", 10, 60, now);
        // Inside an item, a link to a big outside dir counts as a link (not its target).
        std::os::unix::fs::symlink(&victim, root.join("ef").join("lnk")).expect("inner link");
        let r = sweep(&root, limits(30, 0), &[], &free_claim(), now).expect("sweep");
        assert_eq!(r.removed, 1);
        assert!(
            victim.join("keep").exists(),
            "link targets are never followed"
        );
        assert!(
            root.join("ab").exists()
                && root.join("Not-An-Item").exists()
                && root.join("cd").exists()
        );
    }

    #[test]
    fn explicit_downloads_and_items_in_keep_survive_the_size_pass() {
        let d = tempfile::tempdir().expect("tmp");
        let now = SystemTime::now();
        item(d.path(), "aa", 900, 2, now); // explicit big download
        std::fs::write(d.path().join("aa").join(EXPLICIT), b"").expect("marker");
        item(d.path(), "bb", 100, 3, now); // still downloading (kept)
        item(d.path(), "cc", 100, 1, now);
        let r = sweep(d.path(), limits(30, 50), &["bb"], &free_claim(), now).expect("sweep");
        assert_eq!(r.removed, 1, "only cc: aa is exempt from size, bb is kept");
        assert!(d.path().join("aa").exists() && d.path().join("bb").exists());
        // Age still applies to an explicit download.
        item(d.path(), "dd", 10, 90, now);
        std::fs::write(d.path().join("dd").join(EXPLICIT), b"").expect("marker");
        // Adding the marker touched the dir: age it again.
        std::fs::File::open(d.path().join("dd"))
            .expect("open")
            .set_modified(now - DAY * 90)
            .expect("mtime");
        sweep(d.path(), limits(30, 50), &["bb"], &free_claim(), now).expect("sweep");
        assert!(!d.path().join("dd").exists());
    }

    #[test]
    fn an_item_whose_download_starts_mid_sweep_is_skipped() {
        let d = tempfile::tempdir().expect("tmp");
        let now = SystemTime::now();
        item(d.path(), "aa", 100, 90, now);
        item(d.path(), "bb", 100, 90, now);
        // "aa" is claimed by a download (the claim fails), "bb" is free.
        let inbox = super::super::inbox::Inbox::new(PathBuf::new());
        let _held = inbox.begin_download("aa").expect("claim");
        let claim = |id: &str| inbox.begin_download(id).ok();
        let r = sweep(d.path(), limits(30, 1 << 30), &[], &claim, now).expect("sweep");
        assert_eq!(r.removed, 1);
        assert!(d.path().join("aa").exists() && !d.path().join("bb").exists());
        assert!(
            !inbox.is_downloading("bb"),
            "the claim is released after the removal"
        );
    }
}
