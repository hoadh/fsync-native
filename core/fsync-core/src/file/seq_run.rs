//! Sequential-read detection for read-ahead.

#[derive(Default)]
pub(super) struct SeqRun {
    /// End of the previous read; `None` before the first, which is never sequential.
    last_end: Option<u64>,
    /// Consecutive sequential reads.
    pub run: u32,
}

impl SeqRun {
    /// A read is sequential only if it starts where the last one ended.
    pub fn note_read(&mut self, offset: u64, end: u64) {
        if self.last_end == Some(offset) {
            self.run = self.run.saturating_add(1);
        } else {
            self.run = 0;
        }
        self.last_end = Some(end);
    }
}

#[cfg(test)]
mod tests {
    use super::SeqRun;

    #[test]
    fn first_read_is_never_sequential_even_at_offset_zero() {
        let mut s = SeqRun::default();
        s.note_read(0, 4096);
        assert_eq!(s.run, 0, "a header read must not start prefetching");
        s.note_read(4096, 8192);
        assert_eq!(s.run, 1);
        s.note_read(8192, 12288);
        assert_eq!(s.run, 2);
        s.note_read(0, 4096);
        assert_eq!(s.run, 0, "a seek resets the run");
    }
}
