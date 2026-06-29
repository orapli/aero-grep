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

#[derive(Clone, Debug)]
pub enum ViewMode {
    Tree,
    Flat,
}
