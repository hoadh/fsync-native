//! pair/1 protocol handler (new-device side).

use crate::{
    events::EventSink,
    identity::Identity,
    pair::{PairingWindows, run_responder},
    trust::TrustManager,
};
use iroh::{
    endpoint::Connection,
    protocol::{AcceptError, ProtocolHandler},
};
use std::sync::Arc;

#[derive(Clone)]
pub struct PairHandler {
    pub windows: Arc<PairingWindows>,
    pub me: Identity,
    pub trust: Arc<TrustManager>,
    pub events: EventSink,
}

impl std::fmt::Debug for PairHandler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PairHandler")
    }
}

impl ProtocolHandler for PairHandler {
    async fn accept(&self, conn: Connection) -> Result<(), AcceptError> {
        // One unclaimed attempt per source; wrong-code guesses are capped per window.
        let Ok(ticket) = self.windows.admit(super::convert::node_id_of(&conn)) else {
            conn.close(super::gate::CLOSE_UNTRUSTED.into(), b"no pairing window");
            return Ok(());
        };
        // Outcome is reported through the window channel and engine events.
        let _ = run_responder(conn, ticket, &self.me, &self.trust, &self.events).await;
        Ok(())
    }
}
