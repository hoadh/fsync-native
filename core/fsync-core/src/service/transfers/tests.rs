use super::*;

fn states(v: &[TransferEvent]) -> Vec<TransferState> {
    v.iter().map(|e| e.state).collect()
}

#[test]
fn push_completes_when_every_file_was_served() {
    let (item, mut t) = (Uuid::new_v4(), Transfers::default());
    let (id, evs) = t.register_send(item, vec![10, 30]);
    assert_eq!(states(&evs), [TransferState::Started]);
    assert_eq!(evs[0].bytes_total, 40);
    let evs = t.on_served(item, 1);
    assert_eq!(states(&evs), [TransferState::Progress]);
    assert_eq!(evs[0].bytes_done, 30);
    assert!(t.on_served(item, 1).is_empty(), "duplicate ignored");
    let evs = t.on_served(item, 0);
    assert_eq!(states(&evs), [TransferState::Progress, TransferState::Done]);
    assert_eq!((evs[1].transfer_id, evs[1].bytes_done), (id, 40));
    assert!(t.cancel(id).is_none(), "finished transfers are forgotten");
}

#[test]
fn events_before_registration_are_replayed() {
    let (item, mut t) = (Uuid::new_v4(), Transfers::default());
    assert!(t.on_served(item, 0).is_empty());
    let (_, evs) = t.register_send(item, vec![5]);
    assert_eq!(
        states(&evs),
        [
            TransferState::Started,
            TransferState::Progress,
            TransferState::Done
        ]
    );
}

#[test]
fn out_of_range_file_id_is_ignored_and_cancel_reports() {
    let (item, mut t) = (Uuid::new_v4(), Transfers::default());
    let (id, _) = t.register_send(item, vec![5]);
    assert!(t.on_served(item, 9).is_empty());
    assert_eq!(
        t.cancel(id).map(|e| e.state),
        Some(TransferState::Cancelled)
    );
}

#[test]
fn unpulled_pushes_expire() {
    let (item, mut t) = (Uuid::new_v4(), Transfers::default());
    let t0 = Instant::now();
    let (old, _) = t.register_send_at(item, vec![5], t0);
    let (_, evs) = t.register_send_at(Uuid::new_v4(), vec![5], t0 + SEND_TTL);
    assert_eq!(
        states(&evs),
        [TransferState::Failed, TransferState::Started]
    );
    assert_eq!(evs[0].transfer_id, old);
    assert!(t.cancel(old).is_none(), "expired push is forgotten");
}
