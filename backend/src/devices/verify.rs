//! Decide how a submitted list relates to the stored one, using `fsync_proto::verify`.

use crate::error::ApiError;
use fsync_proto::{DeviceList, Outcome, ProtoError, SignedDeviceList, verify_genesis, verify_next};
use uuid::Uuid;

#[derive(Debug)]
pub enum Decision {
    /// Byte-identical to stored: idempotent success.
    Unchanged,
    /// First list for the account.
    Genesis(DeviceList),
    /// Valid successor of the stored list.
    Next(DeviceList),
    /// New chain root replacing the old chain (explicit trust reset).
    Reset(DeviceList),
}

fn invalid(e: ProtoError) -> ApiError {
    ApiError::Unprocessable {
        code: "invalid_chain",
        message: e.to_string(),
    }
}

fn genesis_for(account: Uuid, new: &SignedDeviceList) -> Result<DeviceList, ApiError> {
    let list = verify_genesis(new).map_err(invalid)?;
    if list.account_id != account {
        return Err(invalid(ProtoError::WrongAccount));
    }
    Ok(list)
}

pub fn decide(
    account: Uuid,
    stored: Option<&SignedDeviceList>,
    new: &SignedDeviceList,
    trust_reset: bool,
) -> Result<Decision, ApiError> {
    let Some(stored) = stored else {
        return genesis_for(account, new).map(Decision::Genesis);
    };
    if new == stored {
        return Ok(Decision::Unchanged);
    }
    if trust_reset {
        return genesis_for(account, new).map(Decision::Reset);
    }
    match verify_next(new, stored) {
        Ok(Outcome::Unchanged) => Ok(Decision::Unchanged),
        Ok(Outcome::Advanced(list)) => Ok(Decision::Next(list)),
        Err(ProtoError::TrustReset) => Err(ApiError::Unprocessable {
            code: "trust_reset_required",
            message: "a new genesis list requires header X-Fsync-Trust-Reset: 1".into(),
        }),
        Err(e) => Err(invalid(e)),
    }
}
