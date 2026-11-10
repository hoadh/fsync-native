//! Focused window from the compositor IPC (Hyprland, Sway): the best-effort source identity on
//! Wayland, where data-control reports none. Both queries use a 200 ms budget and never block
//! the clipboard thread longer. Other compositors give `None` (documented limit).

use super::SourceApp;
use super::source_app::for_pid;
use serde_json::Value;
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const IPC_TIMEOUT: Duration = Duration::from_millis(200);
const MAX_REPLY: u64 = 4 * 1024 * 1024;

fn pid_of(v: &Value) -> Option<u32> {
    v.get("pid")?
        .as_u64()
        .and_then(|p| u32::try_from(p).ok())
        .filter(|p| *p > 0)
}

fn push_class(classes: &mut Vec<String>, v: Option<&Value>) {
    if let Some(s) = v.and_then(Value::as_str).filter(|s| !s.is_empty())
        && !classes.iter().any(|c| c == s)
    {
        classes.push(s.to_owned());
    }
}

/// `hyprctl -j activewindow`: `class`, `initialClass`, `pid`; `{}` when nothing has focus.
pub fn parse_hyprland_activewindow(json: &str) -> Option<SourceApp> {
    let v: Value = serde_json::from_str(json).ok()?;
    let mut classes = Vec::new();
    push_class(&mut classes, v.get("class"));
    push_class(&mut classes, v.get("initialClass"));
    let pid = pid_of(&v);
    (pid.is_some() || !classes.is_empty()).then(|| for_pid(pid, classes))
}

fn find_focused(node: &Value) -> Option<&Value> {
    if node.get("focused").and_then(Value::as_bool) == Some(true)
        && (node.get("app_id").is_some_and(|a| !a.is_null())
            || node.get("window_properties").is_some())
    {
        return Some(node);
    }
    for key in ["nodes", "floating_nodes"] {
        if let Some(list) = node.get(key).and_then(Value::as_array)
            && let Some(found) = list.iter().find_map(find_focused)
        {
            return Some(found);
        }
    }
    None
}

/// `swaymsg -t get_tree`: the focused leaf's `app_id` (or XWayland `window_properties.class`).
pub fn parse_sway_tree(json: &str) -> Option<SourceApp> {
    let tree: Value = serde_json::from_str(json).ok()?;
    let node = find_focused(&tree)?;
    let mut classes = Vec::new();
    push_class(&mut classes, node.get("app_id"));
    if let Some(wp) = node.get("window_properties") {
        push_class(&mut classes, wp.get("class"));
        push_class(&mut classes, wp.get("instance"));
    }
    let pid = pid_of(node);
    (pid.is_some() || !classes.is_empty()).then(|| for_pid(pid, classes))
}

fn connect(path: &std::path::Path) -> Option<UnixStream> {
    let s = UnixStream::connect(path).ok()?;
    s.set_read_timeout(Some(IPC_TIMEOUT)).ok()?;
    s.set_write_timeout(Some(IPC_TIMEOUT)).ok()?;
    Some(s)
}

fn hyprland_query(env: &FocusEnv) -> Option<SourceApp> {
    let sig = env.get("HYPRLAND_INSTANCE_SIGNATURE")?;
    // Reject separators: the signature is a directory name, never a path.
    if sig.is_empty() || sig.contains('/') || sig == ".." {
        return None;
    }
    let runtime = env.get("XDG_RUNTIME_DIR")?;
    let sock = std::path::Path::new(&runtime)
        .join("hypr")
        .join(sig)
        .join(".socket.sock");
    let mut s = connect(&sock)?;
    s.write_all(b"j/activewindow").ok()?;
    let mut out = String::new();
    s.take(MAX_REPLY).read_to_string(&mut out).ok()?;
    parse_hyprland_activewindow(&out)
}

/// i3-ipc: "i3-ipc" + payload length (u32, native) + type (u32, 4 = GET_TREE).
fn sway_query(env: &FocusEnv) -> Option<SourceApp> {
    let mut s = connect(std::path::Path::new(&env.get("SWAYSOCK")?))?;
    let mut req = b"i3-ipc".to_vec();
    req.extend_from_slice(&0u32.to_ne_bytes());
    req.extend_from_slice(&4u32.to_ne_bytes());
    s.write_all(&req).ok()?;
    let mut head = [0u8; 14];
    s.read_exact(&mut head).ok()?;
    if &head[..6] != b"i3-ipc" {
        return None;
    }
    let len = u64::from(u32::from_ne_bytes(head[6..10].try_into().ok()?));
    if len > MAX_REPLY {
        return None;
    }
    let mut body = Vec::new();
    s.take(len).read_to_end(&mut body).ok()?;
    parse_sway_tree(std::str::from_utf8(&body).ok()?)
}

/// The environment the compositor lookup needs, read ONCE at backend start: `std::env::var`
/// on the dispatch thread would race with `setenv` on the GUI thread (activation token).
#[derive(Clone, Debug, Default)]
pub struct FocusEnv {
    hypr_signature: Option<String>,
    sway_socket: Option<String>,
    runtime_dir: Option<String>,
}

impl FocusEnv {
    pub fn capture(env: &dyn Fn(&str) -> Option<String>) -> Self {
        Self {
            hypr_signature: env("HYPRLAND_INSTANCE_SIGNATURE"),
            sway_socket: env("SWAYSOCK"),
            runtime_dir: env("XDG_RUNTIME_DIR"),
        }
    }

    fn get(&self, key: &str) -> Option<String> {
        match key {
            "HYPRLAND_INSTANCE_SIGNATURE" => self.hypr_signature.clone(),
            "SWAYSOCK" => self.sway_socket.clone(),
            "XDG_RUNTIME_DIR" => self.runtime_dir.clone(),
            _ => None,
        }
    }
}

/// Total budget of one lookup (connect, write and read together).
const TOTAL_BUDGET: Duration = Duration::from_millis(250);

fn query(env: &FocusEnv) -> Option<SourceApp> {
    if env.hypr_signature.is_some() {
        return hyprland_query(env);
    }
    if env.sway_socket.is_some() {
        return sway_query(env);
    }
    None
}

struct Job {
    env: FocusEnv,
    reply: std::sync::mpsc::Sender<Option<SourceApp>>,
    queued: Instant,
}

struct Worker {
    tx: std::sync::mpsc::Sender<Job>,
    /// When the query it is running now started (`None` while idle).
    busy_since: Arc<Mutex<Option<Instant>>>,
}

/// A worker busy longer than this is considered hung and replaced.
const HUNG_AFTER: Duration = Duration::from_secs(2);

static WORKER: Mutex<Option<Worker>> = Mutex::new(None);
static FAILING: AtomicBool = AtomicBool::new(false);
static WARNED: AtomicBool = AtomicBool::new(false);

/// True while the last lookups failed or timed out (the filter then sees hints only).
pub fn lookup_failing() -> bool {
    FAILING.load(Ordering::SeqCst)
}

fn spawn_worker() -> Option<Worker> {
    let (tx, rx) = std::sync::mpsc::channel::<Job>();
    let busy_since = Arc::new(Mutex::new(None));
    let busy = Arc::clone(&busy_since);
    std::thread::Builder::new()
        .name("fsync-focus-query".into())
        .spawn(move || {
            while let Ok(mut job) = rx.recv() {
                // Only the newest request matters; older ones (and ones that already timed out
                // on the caller's side) are dropped, their callers get "unknown".
                while let Ok(newer) = rx.try_recv() {
                    job = newer;
                }
                if job.queued.elapsed() > TOTAL_BUDGET {
                    continue;
                }
                *busy.lock().unwrap_or_else(|p| p.into_inner()) = Some(Instant::now());
                let found = query(&job.env);
                *busy.lock().unwrap_or_else(|p| p.into_inner()) = None;
                let _ = job.reply.send(found);
            }
        })
        .ok()?;
    Some(Worker { tx, busy_since })
}

fn submit(job: Job) -> bool {
    let mut slot = WORKER.lock().unwrap_or_else(|p| p.into_inner());
    let hung = slot.as_ref().is_some_and(|w| {
        w.busy_since
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .is_some_and(|t| t.elapsed() > HUNG_AFTER)
    });
    if hung && !WARNED.swap(true, Ordering::SeqCst) {
        tracing::warn!("the compositor focus lookup hung; starting a fresh worker");
    }
    if slot.is_none() || hung {
        *slot = spawn_worker(); // the old worker exits when its query returns
    }
    slot.as_ref().is_some_and(|w| w.tx.send(job).is_ok())
}

/// The focused app of the running Wayland compositor, if it is one we can ask; the whole
/// lookup is bounded by `TOTAL_BUDGET`.
pub fn focused_app(env: &FocusEnv) -> Option<SourceApp> {
    if env.hypr_signature.is_none() && env.sway_socket.is_none() {
        return None;
    }
    let (reply, answer) = std::sync::mpsc::channel();
    let job = Job {
        env: env.clone(),
        reply,
        queued: Instant::now(),
    };
    let got = submit(job)
        .then(|| answer.recv_timeout(TOTAL_BUDGET).ok())
        .flatten();
    FAILING.store(got.is_none(), Ordering::SeqCst);
    got.flatten()
}

#[cfg(test)]
#[path = "source_app_ipc_tests.rs"]
mod tests;
