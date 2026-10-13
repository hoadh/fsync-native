//! Size policy: clip cap and the inline budget.

use crate::error::{CoreError, Result};
use fsync_proto::clip::MAX_INLINE_TOTAL;

pub fn check_total(total: u64, max: u64) -> Result<()> {
    if total > max {
        return Err(CoreError::TooLarge { size: total, max });
    }
    Ok(())
}

/// Greedy in rep order: a rep rides inline if it still fits the shared 256 KiB budget.
pub fn choose_inline(sizes: &[usize]) -> Vec<bool> {
    let mut left = MAX_INLINE_TOTAL;
    sizes
        .iter()
        .map(|&s| {
            let fits = s <= left;
            if fits {
                left -= s;
            }
            fits
        })
        .collect()
}
