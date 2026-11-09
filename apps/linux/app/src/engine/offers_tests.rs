use super::*;
use fsync_linux_platform::config_file::AppConfig;
use fsync_linux_platform::paths::Paths;
use fsync_proto::ClipKind;

fn core() -> Arc<Core> {
    let dir = PathBuf::from("/nonexistent/fsync-test");
    let paths = Paths {
        data_dir: dir.clone(),
        config_file: dir.join("config.toml"),
        autostart_dir: dir.join("autostart"),
        runtime_dir: dir.join("run"),
    };
    let cfg = AppConfig {
        api: "http://127.0.0.1:9".into(),
        relays: Vec::new(),
        device_name: "t".into(),
        allow_insecure_http: true,
    };
    Arc::new(Core::new(paths, dir, cfg, None))
}

fn offer(id: &str) -> ClipItem {
    ClipItem {
        item_id: id.into(),
        from: "n".into(),
        from_name: "phone".into(),
        kind: ClipKind::Files,
        ts_ms: 0,
        preview: None,
        reps: vec![],
        files: vec![],
        total_size: 100,
        push: true,
    }
}

fn event(id: &str, state: TransferState, done: u64) -> TransferEvent {
    TransferEvent {
        transfer_id: 7,
        item_id: id.into(),
        file_id: Some(0),
        direction: TransferDirection::Receive,
        state,
        bytes_done: done,
        bytes_total: 100,
        path: None,
        message: None,
    }
}

fn state_of(c: &Core, id: &str) -> Option<OfferState> {
    lock(&c.files.offers)
        .iter()
        .find(|o| o.item_id == id)
        .map(|o| o.state)
}

#[test]
fn progress_only_moves_an_offer_that_is_being_received() {
    let c = core();
    lock(&c.files.offers).push(OfferRow::new(&offer("a")));
    on_transfer(&c, event("a", TransferState::Progress, 40));
    assert_eq!(lock(&c.files.offers)[0].active_bytes, 0, "still pending");
    lock(&c.files.offers)[0].state = OfferState::Receiving;
    on_transfer(&c, event("a", TransferState::Progress, 40));
    assert_eq!(lock(&c.files.offers)[0].done_bytes(), 40);
    on_transfer(&c, event("other", TransferState::Progress, 90));
    assert_eq!(lock(&c.files.offers)[0].done_bytes(), 40);
}

#[test]
fn decline_cancel_and_dismiss_follow_the_state_rules() {
    let c = core();
    lock(&c.files.offers).push(OfferRow::new(&offer("a")));
    lock(&c.files.offers).push(OfferRow::new(&offer("b")));
    dismiss(&c, "a");
    assert_eq!(
        state_of(&c, "a"),
        Some(OfferState::Pending),
        "pending stays"
    );
    decline(&c, "a");
    assert_eq!(state_of(&c, "a"), Some(OfferState::Declined));
    decline(&c, "a");
    dismiss(&c, "a");
    assert_eq!(state_of(&c, "a"), None);
    // Cancel only reaches a running offer.
    cancel(&c, "b");
    assert!(!lock(&c.files.offers)[0].cancel.load(Ordering::SeqCst));
    lock(&c.files.offers)[0].state = OfferState::Receiving;
    cancel(&c, "b");
    assert!(lock(&c.files.offers)[0].cancel.load(Ordering::SeqCst));
}

fn with_offer_and_history_row(c: &Core) {
    lock(&c.files.offers).push(OfferRow::new(&offer("a")));
    c.files.hold_large(offer("b"));
}

#[test]
fn every_sign_out_path_forgets_the_account_and_stales_history_loads() {
    use super::super::ops_account::on_event;
    use crate::core_state::gate::OpKind;
    use fsync_core::service::AccountEvent;
    for ev in [AccountEvent::SessionExpired, AccountEvent::SelfRemoved] {
        let c = core();
        with_offer_and_history_row(&c);
        let flag = lock(&c.files.offers)[0].cancel.clone();
        let load = c.gate.begin(OpKind::History);
        on_event(&c, ev);
        assert!(lock(&c.files.offers).is_empty());
        assert!(!c.files.is_large_held("b"));
        assert!(
            flag.load(Ordering::SeqCst),
            "running transfers are cancelled"
        );
        assert!(
            !c.gate.is_current(OpKind::History, load),
            "an in-flight load is stale"
        );
    }
}

#[test]
fn reset_account_files_alone_is_enough_for_logout() {
    use crate::core_state::gate::OpKind;
    let c = core();
    with_offer_and_history_row(&c);
    let load = c.gate.begin(OpKind::History);
    super::super::inbox_runtime::reset_account_files(&c);
    assert!(lock(&c.files.offers).is_empty());
    assert!(!c.gate.is_current(OpKind::History, load));
}
