use fsync_linux_platform::instance_lock::InstanceLock;
use fsync_linux_platform::ipc::{
    IpcCommand, IpcError, IpcRequest, IpcServer, MAX_LINE_BYTES, Reply, RuntimeDirError,
    ensure_runtime_dir, forward, protocol::read_line_bounded, socket_path,
};
use std::io::{BufReader, Read, Write};
use std::os::unix::fs::{PermissionsExt, symlink};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

const HASH: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

fn private_dir(p: &Path) {
    std::fs::create_dir_all(p).unwrap();
    std::fs::set_permissions(p, std::fs::Permissions::from_mode(0o700)).unwrap();
}

type Seen = Arc<Mutex<Vec<IpcRequest>>>;

fn start(rt: &Path, lock: &InstanceLock) -> (IpcServer, Seen) {
    let seen: Seen = Arc::default();
    let s2 = Arc::clone(&seen);
    let srv = IpcServer::start(
        rt,
        HASH,
        lock,
        Arc::new(move |r| {
            let reply = if r.command == IpcCommand::ClipSend {
                Reply::err("signed-out")
            } else {
                Reply::ok()
            };
            s2.lock().unwrap().push(r);
            reply
        }),
    )
    .unwrap();
    (srv, seen)
}

fn raw(rt: &Path, bytes: &[u8]) -> String {
    let mut s = UnixStream::connect(socket_path(rt, HASH)).unwrap();
    s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    s.write_all(bytes).unwrap();
    let mut out = String::new();
    BufReader::new(s).read_to_string(&mut out).ok();
    out
}

#[test]
fn roundtrip_show_with_token_and_handler_error_reply() {
    let d = tempfile::tempdir().unwrap();
    let rt = d.path().join("run");
    let lock = InstanceLock::acquire(d.path()).unwrap();
    let (srv, seen) = start(&rt, &lock);
    let mut req = IpcRequest::new(IpcCommand::Show);
    req.activation_token = Some("tok".into());
    assert_eq!(forward(&rt, d.path(), HASH, &req).unwrap(), Reply::ok());
    let r = forward(&rt, d.path(), HASH, &IpcRequest::new(IpcCommand::ClipSend)).unwrap();
    assert_eq!(r, Reply::err("signed-out"));
    assert_eq!(
        seen.lock().unwrap()[0].activation_token.as_deref(),
        Some("tok")
    );
    let mode = std::fs::metadata(srv.socket())
        .unwrap()
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(mode, 0o600);
    srv.stop();
    assert!(!socket_path(&rt, HASH).exists());
}

#[test]
fn send_validates_paths_server_side() {
    let d = tempfile::tempdir().unwrap();
    let rt = d.path().join("run");
    let lock = InstanceLock::acquire(d.path()).unwrap();
    let (_srv, seen) = start(&rt, &lock);
    let real = d.path().join("f.txt");
    std::fs::write(&real, "x").unwrap();
    let ok = format!(r#"{{"cmd":"send","paths":[{:?}]}}"#, real.to_str().unwrap());
    assert_eq!(
        raw(&rt, format!("{ok}\n").as_bytes()).trim(),
        r#"{"ok":true}"#
    );
    assert!(
        matches!(seen.lock().unwrap()[0].command, IpcCommand::Send(ref p) if p == std::slice::from_ref(&real))
    );
    for bad in [
        r#"{"cmd":"send","paths":["relative/x"]}"#,
        // A path that does not exist passes the syntax check: the app resolves paths on an op
        // thread (a stat here could hang the IPC thread on a dead mount) and reports it.
        r#"{"cmd":"send","paths":[]}"#,
        r#"{"cmd":"nope"}"#,
        "not json",
    ] {
        let out = raw(&rt, format!("{bad}\n").as_bytes());
        assert!(out.contains(r#""ok":false"#), "{bad} -> {out}");
    }
    assert_eq!(
        seen.lock().unwrap().len(),
        1,
        "invalid requests never reach the handler"
    );
    let many: Vec<String> = (0..1001)
        .map(|_| format!("{:?}", real.to_str().unwrap()))
        .collect();
    let out = raw(
        &rt,
        format!("{{\"cmd\":\"send\",\"paths\":[{}]}}\n", many.join(",")).as_bytes(),
    );
    assert!(out.contains("more than 1000"), "{out}");
}

#[test]
fn oversize_line_rejected_without_buffering_it_all() {
    let d = tempfile::tempdir().unwrap();
    let rt = d.path().join("run");
    let lock = InstanceLock::acquire(d.path()).unwrap();
    let (_srv, seen) = start(&rt, &lock);
    let mut big = vec![b'a'; MAX_LINE_BYTES + 10];
    big.push(b'\n');
    let out = raw(&rt, &big);
    assert!(out.contains("too long"), "{out}");
    assert!(seen.lock().unwrap().is_empty());
    let mut r = BufReader::new(&b"abc"[..]);
    assert!(read_line_bounded(&mut r).is_err()); // no newline
}

#[test]
fn stale_socket_unlinked_by_lock_holder_only() {
    let d = tempfile::tempdir().unwrap();
    let rt = d.path().join("run");
    ensure_runtime_dir(&rt).unwrap();
    // Leave a stale socket file behind (listener dropped without unlink).
    drop(std::os::unix::net::UnixListener::bind(socket_path(&rt, HASH)).unwrap());
    // A client never removes it, and reports "no server" when nobody holds the lock.
    let req = IpcRequest::new(IpcCommand::Show);
    assert!(matches!(
        forward(&rt, d.path(), HASH, &req),
        Err(IpcError::NoServer)
    ));
    assert!(socket_path(&rt, HASH).exists());
    // The holder replaces it.
    let lock = InstanceLock::acquire(d.path()).unwrap();
    let (_srv, _) = start(&rt, &lock);
    assert_eq!(forward(&rt, d.path(), HASH, &req).unwrap(), Reply::ok());
}

#[test]
fn non_socket_at_socket_path_is_not_deleted() {
    let d = tempfile::tempdir().unwrap();
    let rt = d.path().join("run");
    ensure_runtime_dir(&rt).unwrap();
    std::fs::write(socket_path(&rt, HASH), "precious").unwrap();
    let lock = InstanceLock::acquire(d.path()).unwrap();
    let r = IpcServer::start(&rt, HASH, &lock, Arc::new(|_| Reply::ok()));
    assert!(matches!(r, Err(IpcError::Socket(_))));
    assert!(socket_path(&rt, HASH).exists());
}

#[test]
fn runtime_dir_checks() {
    let d = tempfile::tempdir().unwrap();
    let ok = d.path().join("ok");
    ensure_runtime_dir(&ok).unwrap(); // created 0700
    assert_eq!(
        std::fs::metadata(&ok).unwrap().permissions().mode() & 0o777,
        0o700
    );
    ensure_runtime_dir(&ok).unwrap();

    let target = d.path().join("target");
    private_dir(&target);
    let link = d.path().join("link");
    symlink(&target, &link).unwrap();
    assert!(matches!(
        ensure_runtime_dir(&link),
        Err(RuntimeDirError::Symlink(_))
    ));

    let wide = d.path().join("wide");
    std::fs::create_dir(&wide).unwrap();
    std::fs::set_permissions(&wide, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert!(matches!(
        ensure_runtime_dir(&wide),
        Err(RuntimeDirError::WrongMode { mode: 0o755, .. })
    ));
    assert_eq!(
        std::fs::metadata(&wide).unwrap().permissions().mode() & 0o777,
        0o755,
        "never chmod"
    );

    let file = d.path().join("file");
    std::fs::write(&file, "").unwrap();
    assert!(matches!(
        ensure_runtime_dir(&file),
        Err(RuntimeDirError::NotDir(_))
    ));

    // Server and client both refuse a bad dir.
    let lock = InstanceLock::acquire(d.path()).unwrap();
    assert!(IpcServer::start(&link, HASH, &lock, Arc::new(|_| Reply::ok())).is_err());
    assert!(forward(&link, d.path(), HASH, &IpcRequest::new(IpcCommand::Show)).is_err());
}

#[test]
fn client_retries_while_holder_is_still_binding() {
    let d = tempfile::tempdir().unwrap();
    let rt = d.path().join("run");
    let lock = InstanceLock::acquire(d.path()).unwrap();
    std::thread::scope(|s| {
        s.spawn(|| {
            std::thread::sleep(Duration::from_millis(400));
            let (srv, _) = start(&rt, &lock);
            std::thread::sleep(Duration::from_millis(800));
            srv.stop();
        });
        std::thread::sleep(Duration::from_millis(50));
        let r = forward(&rt, d.path(), HASH, &IpcRequest::new(IpcCommand::Show)).unwrap();
        assert_eq!(r, Reply::ok());
    });
}

#[test]
fn client_gives_up_when_holder_never_binds() {
    let d = tempfile::tempdir().unwrap();
    let rt = d.path().join("run");
    let _lock = InstanceLock::acquire(d.path()).unwrap();
    let t = std::time::Instant::now();
    let r = forward(&rt, d.path(), HASH, &IpcRequest::new(IpcCommand::Show));
    assert!(matches!(r, Err(IpcError::NoServer)));
    assert!(
        t.elapsed() >= Duration::from_millis(1500),
        "20 x 100 ms retries"
    );
}

#[test]
fn stop_returns_even_if_the_socket_file_was_removed() {
    let d = tempfile::tempdir().unwrap();
    let rt = d.path().join("run");
    let lock = InstanceLock::acquire(d.path()).unwrap();
    let (srv, _seen) = start(&rt, &lock);
    std::fs::remove_file(socket_path(&rt, HASH)).unwrap();
    let t = std::time::Instant::now();
    srv.stop();
    assert!(t.elapsed() < Duration::from_secs(2));
}

#[test]
fn client_refuses_a_send_path_that_is_not_utf8() {
    use std::os::unix::ffi::OsStringExt;
    let p = std::path::PathBuf::from(std::ffi::OsString::from_vec(b"/tmp/bad-\xff".to_vec()));
    let err = IpcRequest::new(IpcCommand::Send(vec![p]))
        .to_line()
        .expect_err("lossy copy must not be sent");
    assert!(err.to_string().contains("path-unsupported"), "{err}");
}
