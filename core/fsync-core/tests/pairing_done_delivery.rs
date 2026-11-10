//! pair/1 end: the new device reports `Completed` only after the existing device has the final
//! `Done` frame. A new device that tears its node down as soon as it sees `Completed` (the CLI
//! `pair offer` exits) must not cut that frame off: before the fix the existing device then
//! failed with "network: connection lost" although the list was published and the new device
//! had pinned it (ADR 014, known issue "pairing connection lost").

mod common;
use common::{MemBackend, pairing::new_info, peer};
use fsync_core::{EngineEvent, PairSecret, PairingEvent};
use std::time::Duration;
use uuid::Uuid;
use zeroize::Zeroizing;

const ROUNDS: usize = 12;

async fn accept_then_wait_completed(
    ev: &mut tokio::sync::mpsc::Receiver<EngineEvent>,
) -> Option<()> {
    loop {
        match ev.recv().await? {
            EngineEvent::Pairing(PairingEvent::ConfirmRequest { decision, .. }) => {
                decision.accept()
            }
            EngineEvent::Pairing(PairingEvent::Completed { .. }) => return Some(()),
            EngineEvent::Pairing(PairingEvent::Failed { .. }) => return None,
            _ => {}
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn existing_device_succeeds_when_new_device_shuts_down_on_completed() {
    for round in 0..ROUNDS {
        let (mut e, mut n) = (peer().await, peer().await);
        let backend = MemBackend::with(e.genesis(Uuid::new_v4()));
        let secret = PairSecret::Code(Zeroizing::new("246810".to_owned()));
        let (n_id, e_id, addr) = (n.nid(), e.nid(), [n.loopback()]);
        let pending = n
            .node
            .open_pairing(secret.clone(), new_info(), Duration::from_secs(20))
            .unwrap();
        let new_side = async {
            // Like the CLI: the moment `Completed` arrives, the process shuts its node down.
            let done = accept_then_wait_completed(&mut n.events).await;
            n.node.close().await;
            done
        };
        let e_side = e.node.pair_with(&n_id, None, &addr, &secret, &backend);
        let drive_e = async {
            while let Some(ev) = e.events.recv().await {
                if let EngineEvent::Pairing(PairingEvent::ConfirmRequest { decision, .. }) = ev {
                    decision.accept();
                    return;
                }
            }
        };
        let (n_done, e_res, _, n_res) = tokio::time::timeout(Duration::from_secs(20), async {
            tokio::join!(new_side, e_side, drive_e, pending.wait())
        })
        .await
        .expect("pairing round timed out");
        assert!(
            n_done.is_some(),
            "round {round}: new device saw no Completed"
        );
        assert!(
            n.trust.is_trusted(&e_id),
            "round {round}: new device pinned nothing"
        );
        let seq = e_res.unwrap_or_else(|err| panic!("round {round}: existing device: {err}"));
        assert_eq!(n_res.expect("new device outcome"), seq, "round {round}");
    }
}
