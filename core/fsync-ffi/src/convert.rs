//! Result mapping shared by the adapters.

use crate::error::FsyncError;
use fsync_core::service as svc;

pub(crate) type FfiResult<T> = Result<T, FsyncError>;

pub(crate) fn map<T, U: From<T>>(r: svc::SvcResult<T>) -> FfiResult<U> {
    r.map(U::from).map_err(FsyncError::from)
}

pub(crate) fn unit(r: svc::SvcResult<()>) -> FfiResult<()> {
    r.map_err(FsyncError::from)
}
