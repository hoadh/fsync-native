//! Postgres access for device lists.

use crate::error::{ApiError, ApiResult};
use crate::util::now_ms;
use fsync_proto::{NodeId, SignedDeviceList};
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

/// Rows of history kept per (account, epoch).
pub const HISTORY_KEEP: i64 = 100;

pub struct Stored {
    pub epoch: i32,
    pub seq: i64,
    pub signed: SignedDeviceList,
}

fn parse(bytes: &[u8]) -> ApiResult<SignedDeviceList> {
    SignedDeviceList::from_bytes(bytes).map_err(ApiError::internal)
}

pub async fn get(db: &PgPool, account: Uuid) -> ApiResult<Option<Stored>> {
    let row: Option<(i32, i64, Vec<u8>)> =
        sqlx::query_as("SELECT epoch, seq, signed FROM device_lists WHERE account_id = $1")
            .bind(account)
            .fetch_optional(db)
            .await?;
    row.map(|(epoch, seq, b)| {
        Ok(Stored {
            epoch,
            seq,
            signed: parse(&b)?,
        })
    })
    .transpose()
}

/// Lock the account's row (serializes concurrent PUTs for an existing list).
pub async fn get_for_update(conn: &mut PgConnection, account: Uuid) -> ApiResult<Option<Stored>> {
    let row: Option<(i32, i64, Vec<u8>)> = sqlx::query_as(
        "SELECT epoch, seq, signed FROM device_lists WHERE account_id = $1 FOR UPDATE",
    )
    .bind(account)
    .fetch_optional(conn)
    .await?;
    row.map(|(epoch, seq, b)| {
        Ok(Stored {
            epoch,
            seq,
            signed: parse(&b)?,
        })
    })
    .transpose()
}

/// Insert (first list) or replace (successor / reset) the latest list and append history.
/// Returns false if a concurrent first insert won the race.
pub async fn store(
    conn: &mut PgConnection,
    account: Uuid,
    epoch: i32,
    seq: i64,
    signed: &SignedDeviceList,
) -> ApiResult<bool> {
    let bytes = signed.to_bytes().map_err(ApiError::internal)?;
    let now = now_ms();
    let n = sqlx::query(
        "INSERT INTO device_lists (account_id, epoch, seq, signed, updated_at)
         VALUES ($1, $2, $3, $4, $5)
         ON CONFLICT (account_id) DO UPDATE
           SET epoch = EXCLUDED.epoch, seq = EXCLUDED.seq,
               signed = EXCLUDED.signed, updated_at = EXCLUDED.updated_at
         WHERE device_lists.epoch < EXCLUDED.epoch
            OR (device_lists.epoch = EXCLUDED.epoch AND device_lists.seq < EXCLUDED.seq)",
    )
    .bind(account)
    .bind(epoch)
    .bind(seq)
    .bind(&bytes)
    .bind(now)
    .execute(&mut *conn)
    .await?
    .rows_affected();
    if n == 0 {
        return Ok(false);
    }
    sqlx::query(
        "INSERT INTO device_list_history (account_id, epoch, seq, signed, created_at)
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(account)
    .bind(epoch)
    .bind(seq)
    .bind(&bytes)
    .bind(now)
    .execute(&mut *conn)
    .await?;
    // Keep the last HISTORY_KEEP rows of this epoch (seq is consecutive within an epoch) and
    // drop every older epoch: the chain endpoint only serves the current one.
    sqlx::query(
        "DELETE FROM device_list_history
         WHERE account_id = $1 AND (epoch < $2 OR (epoch = $2 AND seq <= $3))",
    )
    .bind(account)
    .bind(epoch)
    .bind(seq - HISTORY_KEEP)
    .execute(conn)
    .await?;
    Ok(true)
}

/// Retained slice of the current epoch's chain.
pub struct Chain {
    pub lists: Vec<SignedDeviceList>,
    /// Lowest seq still in history (older rows were pruned).
    pub earliest_seq: i64,
    /// More rows with `seq >= from_seq` exist beyond `lists`.
    pub truncated: bool,
}

/// Lists of the current epoch with `seq >= from_seq`, ascending, at most `limit`.
pub async fn chain(
    db: &PgPool,
    account: Uuid,
    epoch: i32,
    from_seq: i64,
    limit: i64,
) -> ApiResult<Chain> {
    let rows: Vec<(Vec<u8>,)> = sqlx::query_as(
        "SELECT signed FROM device_list_history
         WHERE account_id = $1 AND epoch = $2 AND seq >= $3 ORDER BY seq LIMIT $4",
    )
    .bind(account)
    .bind(epoch)
    .bind(from_seq)
    .bind(limit + 1)
    .fetch_all(db)
    .await?;
    let (earliest,): (Option<i64>,) = sqlx::query_as(
        "SELECT min(seq) FROM device_list_history WHERE account_id = $1 AND epoch = $2",
    )
    .bind(account)
    .bind(epoch)
    .fetch_one(db)
    .await?;
    let truncated = rows.len() as i64 > limit;
    let lists = rows
        .iter()
        .take(usize::try_from(limit).unwrap_or(0))
        .map(|(b,)| parse(b))
        .collect::<ApiResult<_>>()?;
    Ok(Chain {
        lists,
        earliest_seq: earliest.unwrap_or(0),
        truncated,
    })
}

/// Node ids in the account's current list (empty if none).
pub async fn node_ids(db: &PgPool, account: Uuid) -> ApiResult<Vec<NodeId>> {
    let Some(stored) = get(db, account).await? else {
        return Ok(vec![]);
    };
    let list = stored.signed.decode().map_err(ApiError::internal)?;
    Ok(list.devices.iter().map(|d| d.node_id).collect())
}
