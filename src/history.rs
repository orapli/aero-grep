use crate::models::{HistoryEntry, SearchResult};
use std::path::PathBuf;

/// Skip persisting a result snapshot above this size. Bounds disk use to
/// roughly `history_limit * MAX_SNAPSHOT_BYTES`; oversized results still get
/// a summary entry in `entries`, just no "Load result" snapshot (#25).
const MAX_SNAPSHOT_BYTES: usize = 8 * 1024 * 1024;

fn fits_snapshot_cap(byte_len: usize) -> bool {
    byte_len <= MAX_SNAPSHOT_BYTES
}

pub struct History {
    pub entries: Vec<HistoryEntry>,
    limit: usize,
    /// Location of the summary index (history.json). `None` if the platform
    /// config dir is unavailable, in which case history is in-memory only.
    index_path: Option<PathBuf>,
    /// Directory holding one `<id>.json` snapshot per recorded result (#25).
    results_dir: Option<PathBuf>,
}

impl History {
    fn default_index_path() -> Option<PathBuf> {
        dirs::config_dir().map(|d| d.join("aero-grep").join("history.json"))
    }

    fn default_results_dir() -> Option<PathBuf> {
        dirs::config_dir().map(|d| d.join("aero-grep").join("history_results"))
    }

    pub fn load(limit: usize) -> Self {
        let index_path = Self::default_index_path();
        let results_dir = Self::default_results_dir();
        let entries = index_path
            .as_ref()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|data| serde_json::from_str::<Vec<HistoryEntry>>(&data).ok())
            .unwrap_or_default();

        let history = Self {
            entries,
            limit,
            index_path,
            results_dir,
        };
        history.prune_orphan_results();
        history
    }

    /// Deletes any `history_results/<id>.json` whose id is not present in
    /// the loaded index — defensive cleanup (e.g. after a crash mid-write,
    /// or a manually edited history.json). Best-effort; errors are ignored.
    fn prune_orphan_results(&self) {
        let Some(dir) = &self.results_dir else {
            return;
        };
        let Ok(read_dir) = std::fs::read_dir(dir) else {
            return;
        };
        let known: std::collections::HashSet<u64> = self.entries.iter().map(|e| e.id).collect();
        for entry in read_dir.flatten() {
            let path = entry.path();
            let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
                continue;
            };
            let Ok(id) = stem.parse::<u64>() else {
                continue;
            };
            if !known.contains(&id) {
                let _ = std::fs::remove_file(&path);
            }
        }
    }

    /// Persists `entries` (summary index only) on a background thread so
    /// callers — always on the UI thread — never block a frame on disk I/O.
    /// `entries` is cheap to clone: it holds only summary data (#15), not
    /// full match content.
    pub fn save(&self) -> Option<std::thread::JoinHandle<()>> {
        let path = self.index_path.clone()?;
        let entries = self.entries.clone();
        Some(std::thread::spawn(move || {
            if let Some(parent) = path.parent() {
                if std::fs::create_dir_all(parent).is_err() {
                    return;
                }
            }
            if let Ok(data) = serde_json::to_string_pretty(&entries) {
                let _ = std::fs::write(path, data);
            }
        }))
    }

    fn result_path(&self, id: u64) -> Option<PathBuf> {
        self.results_dir
            .as_ref()
            .map(|d| d.join(format!("{id}.json")))
    }

    /// Persists the full result for `result.id` as an on-disk snapshot, so
    /// "Load result" can reopen the exact original results later (#25),
    /// without keeping full match content in the lightweight `entries`
    /// index (#15).
    ///
    /// Serialization (CPU-only, no I/O latency) happens on the caller's
    /// thread, so the — possibly large — result is never cloned just to
    /// move it across threads; only the resulting JSON `String` (one
    /// buffer) needs to move. The actual disk write, which has unpredictable
    /// latency, is what's backgrounded. Skipped silently above the size
    /// cap; the summary entry (already pushed separately) is unaffected —
    /// "Load result" just stays unavailable for that entry.
    pub fn save_result(&self, result: &SearchResult) -> Option<std::thread::JoinHandle<()>> {
        let path = self.result_path(result.id)?;
        let data = serde_json::to_string(result).ok()?;
        if !fits_snapshot_cap(data.len()) {
            return None;
        }
        Some(std::thread::spawn(move || {
            if let Some(parent) = path.parent() {
                if std::fs::create_dir_all(parent).is_err() {
                    return;
                }
            }
            let _ = std::fs::write(path, data);
        }))
    }

    /// Reads a previously-saved snapshot from disk, if present. A plain
    /// synchronous read: triggered by an explicit user click ("Load
    /// result"), not a per-frame hot path, and the file is small (one
    /// result, size-capped at save time).
    pub fn load_result(&self, id: u64) -> Option<SearchResult> {
        let path = self.result_path(id)?;
        let data = std::fs::read_to_string(path).ok()?;
        serde_json::from_str(&data).ok()
    }

    /// Whether a snapshot exists for `id` — used to enable/disable "Load
    /// result" in the history panel without needing to read the file.
    pub fn has_result(&self, id: u64) -> bool {
        self.result_path(id).map(|p| p.is_file()).unwrap_or(false)
    }

    pub fn push(&mut self, entry: HistoryEntry) {
        self.entries.insert(0, entry);
        self.drop_excess();
        self.save();
    }

    /// Drops entries beyond `limit` and deletes their snapshot files, so the
    /// on-disk result store never grows unbounded relative to the index.
    fn drop_excess(&mut self) {
        if self.entries.len() <= self.limit {
            return;
        }
        let dropped: Vec<u64> = self.entries.drain(self.limit..).map(|e| e.id).collect();
        for id in dropped {
            if let Some(path) = self.result_path(id) {
                let _ = std::fs::remove_file(path);
            }
        }
    }

    pub fn set_limit(&mut self, limit: usize) {
        self.limit = limit;
        self.drop_excess();
        // `drop_excess` may delete result files; keep the persisted index in
        // sync with `entries` (previously this only mutated in-memory state
        // and relied on a later push/remove/clear to flush — now that it
        // can also delete files out from under a stale on-disk index, save
        // immediately).
        self.save();
    }

    pub fn remove(&mut self, id: u64) {
        self.entries.retain(|e| e.id != id);
        if let Some(path) = self.result_path(id) {
            let _ = std::fs::remove_file(path);
        }
        self.save();
    }

    pub fn clear(&mut self) {
        self.entries.clear();
        if let Some(dir) = &self.results_dir {
            let _ = std::fs::remove_dir_all(dir);
        }
        self.save();
    }

    pub fn next_id(&self) -> u64 {
        self.entries.iter().map(|e| e.id).max().unwrap_or(0) + 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{FileMatch, SearchParams, SearchResult};
    use std::path::PathBuf;
    use std::time::{Duration, Instant};

    fn make_entry(id: u64) -> HistoryEntry {
        HistoryEntry {
            id,
            params: SearchParams::default(),
            timestamp: "2026-01-01T00:00:00".to_string(),
            duration_ms: 0,
            total_matches: 0,
            file_count: 0,
            truncated: false,
        }
    }

    fn make_result(id: u64, files: Vec<FileMatch>) -> SearchResult {
        SearchResult {
            id,
            params: SearchParams::default(),
            files,
            timestamp: "2026-01-01T00:00:00".to_string(),
            duration_ms: 0,
            total_matches: 0,
            truncated: false,
        }
    }

    /// History pointed at a fresh tempdir, so these tests never touch the
    /// real per-user config directory. The tempdir is returned too so it
    /// isn't dropped (and deleted) while the test still needs it.
    fn temp_history(limit: usize) -> (History, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let h = History {
            entries: vec![],
            limit,
            index_path: Some(dir.path().join("history.json")),
            results_dir: Some(dir.path().join("history_results")),
        };
        (h, dir)
    }

    fn wait_until(mut pred: impl FnMut() -> bool, timeout: Duration) -> bool {
        let start = Instant::now();
        loop {
            if pred() {
                return true;
            }
            if start.elapsed() > timeout {
                return false;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    // Tests below construct `History` directly (rather than via `load`) and
    // mutate `entries` without calling `push`/`save`, so the suite never
    // touches the real per-user config directory.

    #[test]
    fn test_push_prepends_and_trims() {
        let mut h = History {
            entries: vec![],
            limit: 3,
            index_path: None,
            results_dir: None,
        };
        h.entries.push(make_entry(1));
        h.entries.push(make_entry(2));
        h.entries.insert(0, make_entry(3)); // simulate push behaviour
        h.entries.truncate(h.limit);
        assert_eq!(h.entries[0].id, 3);
        assert_eq!(h.entries.len(), 3);
    }

    #[test]
    fn test_set_limit_trims_entries() {
        let mut h = History {
            entries: vec![make_entry(1), make_entry(2), make_entry(3)],
            limit: 10,
            index_path: None,
            results_dir: None,
        };
        h.set_limit(2);
        assert_eq!(h.entries.len(), 2);
    }

    #[test]
    fn test_next_id_increments() {
        let h = History {
            entries: vec![make_entry(5), make_entry(3)],
            limit: 100,
            index_path: None,
            results_dir: None,
        };
        assert_eq!(h.next_id(), 6);
        let empty = History {
            entries: vec![],
            limit: 100,
            index_path: None,
            results_dir: None,
        };
        assert_eq!(empty.next_id(), 1);
    }

    #[test]
    fn test_history_entry_from_search_result() {
        let result = SearchResult {
            id: 7,
            params: SearchParams {
                pattern: "foo".to_string(),
                directory: "/tmp/proj".to_string(),
                ..SearchParams::default()
            },
            files: vec![
                FileMatch {
                    path: PathBuf::from("a.rs"),
                    matches: vec![],
                },
                FileMatch {
                    path: PathBuf::from("b.rs"),
                    matches: vec![],
                },
            ],
            timestamp: "2026-01-01T00:00:00".to_string(),
            duration_ms: 12,
            total_matches: 5,
            truncated: true,
        };
        let entry = HistoryEntry::from(&result);
        assert_eq!(entry.id, 7);
        assert_eq!(entry.params, result.params);
        assert_eq!(entry.timestamp, result.timestamp);
        assert_eq!(entry.duration_ms, 12);
        assert_eq!(entry.total_matches, 5);
        assert_eq!(entry.file_count, 2); // derived from files.len(), files itself dropped
        assert!(entry.truncated);
    }

    #[test]
    fn test_history_entry_serde_roundtrip() {
        let e = make_entry(42);
        let json = serde_json::to_string(&e).unwrap();
        let restored: HistoryEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.id, 42);
        assert!(!restored.truncated);
    }

    #[test]
    fn test_history_entry_missing_truncated_defaults_false() {
        // Old-style JSON without the truncated field.
        let old_json = r#"{"id":1,"params":{"pattern":"","directory":"","is_regex":false,"case_sensitive":false,"file_glob":"","replace_text":""},"timestamp":"2026-01-01T00:00:00","duration_ms":0,"total_matches":0,"file_count":0}"#;
        let e: HistoryEntry = serde_json::from_str(old_json).unwrap();
        assert!(!e.truncated);
    }

    #[test]
    fn test_load_falls_back_to_empty_on_legacy_schema() {
        // A history.json written by the pre-#15 format (entries had `files`,
        // no `file_count`) should fail HistoryEntry deserialization, so
        // `History::load`'s `.ok()` chain takes the `unwrap_or_default()`
        // branch for old files instead of panicking.
        let legacy = r#"[{"id":1,"params":{"pattern":"x","directory":"/d","is_regex":false,"case_sensitive":false,"file_glob":"","replace_text":""},"files":[],"timestamp":"2026-01-01T00:00:00","duration_ms":0,"total_matches":0}]"#;
        let parsed = serde_json::from_str::<Vec<HistoryEntry>>(legacy).ok();
        assert!(parsed.is_none());
    }

    // ── #25: full-result snapshot store ─────────────────────────────────────

    #[test]
    fn test_search_result_json_roundtrip() {
        let result = make_result(
            1,
            vec![FileMatch {
                path: PathBuf::from("a.rs"),
                matches: vec![],
            }],
        );
        let json = serde_json::to_string(&result).unwrap();
        let restored: SearchResult = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.id, 1);
        assert_eq!(restored.files.len(), 1);
        assert_eq!(restored.files[0].path, PathBuf::from("a.rs"));
    }

    #[test]
    fn test_fits_snapshot_cap() {
        assert!(fits_snapshot_cap(0));
        assert!(fits_snapshot_cap(MAX_SNAPSHOT_BYTES));
        assert!(!fits_snapshot_cap(MAX_SNAPSHOT_BYTES + 1));
    }

    #[test]
    fn test_save_result_then_load_result_roundtrip() {
        let (h, _dir) = temp_history(10);
        let result = make_result(
            5,
            vec![FileMatch {
                path: PathBuf::from("a.rs"),
                matches: vec![],
            }],
        );
        let handle = h.save_result(&result);
        assert!(
            handle.is_some(),
            "should spawn a write for an under-cap result"
        );
        handle.unwrap().join().unwrap();

        assert!(h.has_result(5));
        let loaded = h.load_result(5).expect("snapshot should be readable");
        assert_eq!(loaded.id, 5);
        assert_eq!(loaded.files.len(), 1);
    }

    #[test]
    fn test_save_result_over_cap_is_skipped() {
        let (h, _dir) = temp_history(10);
        // One huge match line easily exceeds the 8MB cap.
        let huge_content = "x".repeat(MAX_SNAPSHOT_BYTES + 1024);
        let result = make_result(
            9,
            vec![FileMatch {
                path: PathBuf::from("big.txt"),
                matches: vec![crate::models::LineMatch {
                    line_number: 1,
                    content: huge_content,
                    ranges: vec![],
                    is_match: true,
                }],
            }],
        );
        let handle = h.save_result(&result);
        assert!(handle.is_none(), "over-cap result must not spawn a write");
        assert!(!h.has_result(9));
        assert!(h.load_result(9).is_none());
    }

    #[test]
    fn test_load_result_missing_returns_none() {
        let (h, _dir) = temp_history(10);
        assert!(!h.has_result(123));
        assert!(h.load_result(123).is_none());
    }

    #[test]
    fn test_remove_deletes_snapshot_file() {
        let (mut h, _dir) = temp_history(10);
        let result = make_result(1, vec![]);
        h.save_result(&result).unwrap().join().unwrap();
        assert!(h.has_result(1));

        h.entries.push(make_entry(1));
        h.remove(1);

        assert!(
            wait_until(|| !h.has_result(1), Duration::from_secs(2)),
            "remove() should delete the snapshot file"
        );
    }

    #[test]
    fn test_clear_deletes_all_snapshot_files() {
        let (mut h, _dir) = temp_history(10);
        for id in 1..=3 {
            h.save_result(&make_result(id, vec![]))
                .unwrap()
                .join()
                .unwrap();
            h.entries.push(make_entry(id));
        }
        assert!(h.has_result(1) && h.has_result(2) && h.has_result(3));

        h.clear();

        assert!(!h.has_result(1));
        assert!(!h.has_result(2));
        assert!(!h.has_result(3));
    }

    #[test]
    fn test_push_truncation_deletes_dropped_snapshot_files() {
        let (mut h, _dir) = temp_history(2);
        // Pre-existing entries 1 and 2 (oldest), each with a snapshot.
        h.save_result(&make_result(1, vec![]))
            .unwrap()
            .join()
            .unwrap();
        h.save_result(&make_result(2, vec![]))
            .unwrap()
            .join()
            .unwrap();
        h.entries = vec![make_entry(2), make_entry(1)]; // newest-first, as push() maintains

        // Pushing a 3rd entry over the limit=2 should drop the oldest (id 1)
        // and delete its snapshot file, while keeping 2's.
        h.save_result(&make_result(3, vec![]))
            .unwrap()
            .join()
            .unwrap();
        h.push(make_entry(3));

        assert_eq!(
            h.entries.iter().map(|e| e.id).collect::<Vec<_>>(),
            vec![3, 2]
        );
        assert!(
            wait_until(|| !h.has_result(1), Duration::from_secs(2)),
            "push() truncation should delete the dropped entry's snapshot"
        );
        assert!(h.has_result(2));
        assert!(h.has_result(3));
    }

    #[test]
    fn test_set_limit_deletes_dropped_snapshot_files() {
        let (mut h, _dir) = temp_history(10);
        for id in 1..=3 {
            h.save_result(&make_result(id, vec![]))
                .unwrap()
                .join()
                .unwrap();
            h.entries.push(make_entry(id));
        }

        h.set_limit(1);

        assert_eq!(h.entries.len(), 1);
        assert!(
            wait_until(
                || !h.has_result(2) && !h.has_result(3),
                Duration::from_secs(2)
            ),
            "set_limit() truncation should delete dropped entries' snapshots"
        );
    }

    #[test]
    fn test_load_prunes_orphan_result_files() {
        let dir = tempfile::tempdir().unwrap();
        let index_path = dir.path().join("history.json");
        let results_dir = dir.path().join("history_results");
        std::fs::create_dir_all(&results_dir).unwrap();

        // Index only references id 1.
        let entries = vec![make_entry(1)];
        std::fs::write(&index_path, serde_json::to_string(&entries).unwrap()).unwrap();

        // But both 1.json and 2.json (orphan) exist on disk.
        std::fs::write(results_dir.join("1.json"), "{}").unwrap();
        std::fs::write(results_dir.join("2.json"), "{}").unwrap();

        let h = History {
            entries,
            limit: 10,
            index_path: Some(index_path),
            results_dir: Some(results_dir.clone()),
        };
        h.prune_orphan_results();

        assert!(results_dir.join("1.json").exists());
        assert!(!results_dir.join("2.json").exists());
    }
}
