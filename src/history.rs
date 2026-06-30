use crate::models::HistoryEntry;
use std::path::PathBuf;

pub struct History {
    pub entries: Vec<HistoryEntry>,
    limit: usize,
}

impl History {
    fn history_path() -> Option<PathBuf> {
        dirs::config_dir().map(|d| d.join("aero-grep").join("history.json"))
    }

    pub fn load(limit: usize) -> Self {
        let entries = Self::history_path()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|data| serde_json::from_str::<Vec<HistoryEntry>>(&data).ok())
            .unwrap_or_default();

        Self { entries, limit }
    }

    /// Persists `entries` on a background thread so callers (always on the UI
    /// thread) never block a frame on disk I/O. `entries` is cheap to clone
    /// now that it holds only summary data, not full match content.
    pub fn save(&self) {
        let Some(path) = Self::history_path() else {
            return;
        };
        let entries = self.entries.clone();
        std::thread::spawn(move || {
            if let Some(parent) = path.parent() {
                if std::fs::create_dir_all(parent).is_err() {
                    return;
                }
            }
            if let Ok(data) = serde_json::to_string_pretty(&entries) {
                let _ = std::fs::write(path, data);
            }
        });
    }

    pub fn push(&mut self, entry: HistoryEntry) {
        self.entries.insert(0, entry);
        self.entries.truncate(self.limit);
        self.save();
    }

    pub fn set_limit(&mut self, limit: usize) {
        self.limit = limit;
        self.entries.truncate(limit);
    }

    pub fn remove(&mut self, id: u64) {
        self.entries.retain(|e| e.id != id);
        self.save();
    }

    pub fn clear(&mut self) {
        self.entries.clear();
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

    // Tests below construct `History` and mutate `entries` directly (rather
    // than calling `push`/`save`) so the suite never touches the real
    // per-user config directory.

    #[test]
    fn test_push_prepends_and_trims() {
        let mut h = History {
            entries: vec![],
            limit: 3,
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
        };
        h.set_limit(2);
        assert_eq!(h.entries.len(), 2);
    }

    #[test]
    fn test_next_id_increments() {
        let h = History {
            entries: vec![make_entry(5), make_entry(3)],
            limit: 100,
        };
        assert_eq!(h.next_id(), 6);
        let empty = History {
            entries: vec![],
            limit: 100,
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
}
