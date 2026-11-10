//! Files staged by `--send` / a window drop while the DevicePicker waits for the user's answer.
//! `id` changes with every stage and every clear: an answer is valid only for the set the picker
//! displayed (a set swapped in meanwhile is never confirmed by a click that did not see it).

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Staged {
    id: u64,
    /// Counts stage requests (taken when one starts): a verdict older than the latest request
    /// is discarded, so a slow check cannot replace a set the user already sees.
    requests: u64,
    /// Validated canonical UTF-8 paths: exactly what core receives.
    pub paths: Vec<String>,
    /// What the picker shows (counts, first names, size); GUI only, never logged.
    pub summary: String,
}

impl Staged {
    pub fn id(&self) -> u64 {
        self.id
    }

    pub fn is_empty(&self) -> bool {
        self.paths.is_empty()
    }

    /// A stage request starts; the returned number identifies it.
    pub fn begin_request(&mut self) -> u64 {
        self.requests += 1;
        self.requests
    }

    pub fn is_latest(&self, request: u64) -> bool {
        request == self.requests
    }

    /// Sign-out: every request still being checked is void.
    pub fn void_requests(&mut self) {
        self.requests += 1;
    }

    pub fn set(&mut self, paths: Vec<String>, summary: String) {
        self.id += 1;
        self.paths = paths;
        self.summary = summary;
    }

    /// Empties the set; true when something was staged (the picker must be told).
    pub fn clear(&mut self) -> bool {
        if self.paths.is_empty() {
            return false;
        }
        self.id += 1;
        self.paths.clear();
        self.summary.clear();
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_stage_and_clear_changes_the_id() {
        let mut s = Staged::default();
        s.set(vec!["/a".into()], "one".into());
        let first = s.id();
        s.set(vec!["/b".into()], "two".into());
        assert_ne!(first, s.id());
        let second = s.id();
        assert!(s.clear());
        assert_ne!(second, s.id());
        assert!(s.is_empty());
        let after = s.id();
        assert!(!s.clear());
        assert_eq!(after, s.id());
    }

    #[test]
    fn only_the_latest_request_is_current_and_sign_out_voids_all() {
        let mut s = Staged::default();
        let (a, b) = (s.begin_request(), s.begin_request());
        assert!(!s.is_latest(a) && s.is_latest(b));
        s.void_requests();
        assert!(!s.is_latest(b));
    }
}
