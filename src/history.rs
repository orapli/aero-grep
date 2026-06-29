use crate::models::SearchResult;
use anyhow::Result;
use std::path::PathBuf;

pub struct History {
    pub entries: Vec<SearchResult>,
    limit: usize,
}

impl History {
    fn history_path() -> Option<PathBuf> {
        dirs::config_dir().map(|d| d.join("aero-grep").join("history.json"))
    }

    pub fn load(limit: usize) -> Self {
        let entries = Self::history_path()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|data| serde_json::from_str::<Vec<SearchResult>>(&data).ok())
            .unwrap_or_default();

        Self { entries, limit }
    }

    pub fn save(&self) -> Result<()> {
        let Some(path) = Self::history_path() else {
            return Ok(());
        };
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let data = serde_json::to_string_pretty(&self.entries)?;
        std::fs::write(path, data)?;
        Ok(())
    }

    pub fn push(&mut self, result: SearchResult) {
        self.entries.insert(0, result);
        self.entries.truncate(self.limit);
        let _ = self.save();
    }

    pub fn set_limit(&mut self, limit: usize) {
        self.limit = limit;
        self.entries.truncate(limit);
    }

    pub fn remove(&mut self, id: u64) {
        self.entries.retain(|e| e.id != id);
        let _ = self.save();
    }

    pub fn clear(&mut self) {
        self.entries.clear();
        let _ = self.save();
    }

    pub fn next_id(&self) -> u64 {
        self.entries.iter().map(|e| e.id).max().unwrap_or(0) + 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{SearchParams, SearchResult};

    fn make_result(id: u64) -> SearchResult {
        SearchResult {
            id,
            params: SearchParams::default(),
            files: vec![],
            timestamp: "2026-01-01T00:00:00".to_string(),
            duration_ms: 0,
            total_matches: 0,
            truncated: false,
        }
    }

    #[test]
    fn test_push_prepends_and_trims() {
        let mut h = History {
            entries: vec![],
            limit: 3,
        };
        h.entries.push(make_result(1));
        h.entries.push(make_result(2));
        h.entries.insert(0, make_result(3)); // simulate push behaviour
        h.entries.truncate(h.limit);
        assert_eq!(h.entries[0].id, 3);
        assert_eq!(h.entries.len(), 3);
    }

    #[test]
    fn test_set_limit_trims_entries() {
        let mut h = History {
            entries: vec![make_result(1), make_result(2), make_result(3)],
            limit: 10,
        };
        h.set_limit(2);
        assert_eq!(h.entries.len(), 2);
    }

    #[test]
    fn test_next_id_increments() {
        let h = History {
            entries: vec![make_result(5), make_result(3)],
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
    fn test_search_result_serde_roundtrip() {
        let r = make_result(42);
        let json = serde_json::to_string(&r).unwrap();
        let restored: SearchResult = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.id, 42);
        assert!(!restored.truncated);
    }

    #[test]
    fn test_search_result_missing_truncated_defaults_false() {
        // Old history.json without the truncated field
        let old_json = r#"{"id":1,"params":{"pattern":"","directory":"","is_regex":false,"case_sensitive":false,"file_glob":"","replace_text":""},"files":[],"timestamp":"2026-01-01T00:00:00","duration_ms":0,"total_matches":0}"#;
        let r: SearchResult = serde_json::from_str(old_json).unwrap();
        assert!(!r.truncated);
    }
}
