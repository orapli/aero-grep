use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Copy, Default)]
pub enum ReplaceScope {
    #[default]
    Selected,
    All,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Default)]
pub struct SearchParams {
    pub pattern: String,
    pub directory: String,
    pub is_regex: bool,
    pub case_sensitive: bool,
    pub file_glob: String,
    pub replace_text: String,
    #[serde(default)]
    pub context_lines: usize,
    /// Comma-separated glob patterns / directory names to exclude (e.g. "node_modules,*.min.js")
    #[serde(default)]
    pub exclude_glob: String,
    #[serde(default)]
    pub replace_scope: ReplaceScope,
    #[serde(default)]
    pub max_depth: Option<usize>,
    #[serde(default)]
    pub word_boundary: bool,
    /// Additional search roots beyond `directory`. Empty = single-root mode.
    #[serde(default)]
    pub roots: Vec<String>,
}

impl SearchParams {
    /// Builds fresh params seeded from the user's persisted defaults (#30):
    /// Context lines / Max depth / Case / Regex / Word. Only for genuinely
    /// new searches (app startup, new tab) — never call this on params
    /// belonging to an in-progress or already-run search.
    pub fn seeded_from_config(config: &crate::config::Config) -> Self {
        let mut params = Self::default();
        params.apply_default_search_options(config);
        params
    }

    /// Overwrites just the persisted-default fields (Context lines / Max
    /// depth / Case / Regex / Word), leaving pattern/directory/glob/etc.
    /// untouched. Used when opening a new tab (#30) — the search inputs
    /// carry over by design, only the advanced-row defaults reset.
    pub fn apply_default_search_options(&mut self, config: &crate::config::Config) {
        self.context_lines = config.default_context_lines;
        self.max_depth = (config.default_max_depth != 0).then_some(config.default_max_depth);
        self.case_sensitive = config.default_case_sensitive;
        self.is_regex = config.default_is_regex;
        self.word_boundary = config.default_word_boundary;
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct MatchRange {
    pub start: usize,
    pub end: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LineMatch {
    pub line_number: usize,
    pub content: String,
    pub ranges: Vec<MatchRange>,
    /// true = this line matched the pattern; false = context-only line
    #[serde(default = "bool_true")]
    pub is_match: bool,
}

fn bool_true() -> bool {
    true
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FileMatch {
    pub path: PathBuf,
    pub matches: Vec<LineMatch>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SearchResult {
    pub id: u64,
    pub params: SearchParams,
    pub files: Vec<FileMatch>,
    pub timestamp: String,
    pub duration_ms: u64,
    pub total_matches: usize,
    #[serde(default)]
    pub truncated: bool,
}

impl SearchResult {
    pub fn file_count(&self) -> usize {
        self.files.len()
    }
}

/// Lightweight, persisted record of a past search — params and summary counts
/// only. Deliberately does NOT hold `files` (the full match text/ranges):
/// history can accumulate many entries, and storing full results there would
/// bloat history.json and make every history-panel redraw clone megabytes of
/// match text. Re-running the search (cheap; this tool is fast) recovers the
/// full results if needed.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub id: u64,
    pub params: SearchParams,
    pub timestamp: String,
    pub duration_ms: u64,
    pub total_matches: usize,
    pub file_count: usize,
    #[serde(default)]
    pub truncated: bool,
}

impl From<&SearchResult> for HistoryEntry {
    fn from(result: &SearchResult) -> Self {
        Self {
            id: result.id,
            params: result.params.clone(),
            timestamp: result.timestamp.clone(),
            duration_ms: result.duration_ms,
            total_matches: result.total_matches,
            file_count: result.file_count(),
            truncated: result.truncated,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Default)]
pub enum ViewMode {
    #[default]
    Tree,
    Flat,
}
