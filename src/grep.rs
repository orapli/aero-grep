use crate::config::Config;
use crate::models::{FileMatch, LineMatch, MatchRange, SearchParams};
use anyhow::{Context, Result};
use glob::Pattern;
use grep_matcher::Matcher;
use grep_regex::{RegexMatcher, RegexMatcherBuilder};
use grep_searcher::{BinaryDetection, SearcherBuilder, Sink, SinkContext, SinkMatch};
use ignore::WalkState;
use rayon::prelude::*;
use regex::Regex;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

fn escape_pattern(params: &SearchParams) -> String {
    if params.is_regex {
        params.pattern.clone()
    } else {
        regex::escape(&params.pattern)
    }
}

pub fn build_regex(params: &SearchParams) -> Result<Regex> {
    let pattern = escape_pattern(params);

    let pattern = if params.word_boundary {
        format!(r"\b(?:{})\b", pattern)
    } else {
        pattern
    };

    let pattern = if params.case_sensitive {
        pattern
    } else {
        format!("(?i){}", pattern)
    };

    Regex::new(&pattern).with_context(|| format!("Invalid regex: {}", params.pattern))
}

fn matches_glob(glob: &str, path: &Path) -> bool {
    if glob.is_empty() {
        return true;
    }
    let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    glob.split(',')
        .map(|pat| pat.trim())
        .filter(|pat| !pat.is_empty())
        .filter_map(|pat| Pattern::new(pat).ok())
        .any(|p| p.matches(file_name))
}

/// Returns true if the path should be excluded by any of the comma-separated patterns.
/// Each token is matched against every path component (directory segment or file name).
fn is_excluded(exclude: &str, path: &Path) -> bool {
    if exclude.is_empty() {
        return false;
    }
    let components: Vec<&str> = path
        .components()
        .filter_map(|c| c.as_os_str().to_str())
        .collect();

    exclude
        .split(',')
        .map(|token| token.trim())
        .filter(|token| !token.is_empty())
        .filter_map(|token| Pattern::new(token).ok())
        .any(|pat| components.iter().any(|seg| pat.matches(seg)))
}

fn build_grep_matcher(params: &SearchParams) -> Result<RegexMatcher> {
    let pattern = escape_pattern(params);
    RegexMatcherBuilder::new()
        .case_insensitive(!params.case_sensitive)
        .word(params.word_boundary)
        .build(&pattern)
        .map_err(|e| anyhow::anyhow!("Invalid regex '{}': {}", params.pattern, e))
}

fn strip_newline(bytes: &[u8]) -> &[u8] {
    bytes
        .strip_suffix(b"\n")
        .map(|b| b.strip_suffix(b"\r").unwrap_or(b))
        .unwrap_or(bytes)
}

struct FileSink<'a> {
    matcher: &'a RegexMatcher,
    cancel: &'a Arc<std::sync::atomic::AtomicBool>,
    matches: Vec<LineMatch>,
}

impl<'a> Sink for FileSink<'a> {
    type Error = std::io::Error;

    fn matched(
        &mut self,
        _searcher: &grep_searcher::Searcher,
        mat: &SinkMatch<'_>,
    ) -> Result<bool, Self::Error> {
        if self.cancel.load(std::sync::atomic::Ordering::Relaxed) {
            return Ok(false);
        }
        let bytes = mat.bytes();
        let line_num = mat.line_number().unwrap_or(0) as usize;
        let content_bytes = strip_newline(bytes);
        let content = String::from_utf8_lossy(content_bytes).into_owned();

        let mut ranges = Vec::new();
        let _ = self.matcher.find_iter(content_bytes, |m| {
            ranges.push(MatchRange {
                start: m.start(),
                end: m.end(),
            });
            true
        });

        self.matches.push(LineMatch {
            line_number: line_num,
            content,
            ranges,
            is_match: true,
        });
        Ok(true)
    }

    fn context(
        &mut self,
        _searcher: &grep_searcher::Searcher,
        ctx: &SinkContext<'_>,
    ) -> Result<bool, Self::Error> {
        if self.cancel.load(std::sync::atomic::Ordering::Relaxed) {
            return Ok(false);
        }
        let bytes = ctx.bytes();
        let line_num = ctx.line_number().unwrap_or(0) as usize;
        let content = String::from_utf8_lossy(strip_newline(bytes)).into_owned();
        self.matches.push(LineMatch {
            line_number: line_num,
            content,
            ranges: vec![],
            is_match: false,
        });
        Ok(true)
    }
}

fn search_file(
    path: &Path,
    matcher: &RegexMatcher,
    max_size_mb: u64,
    context: usize,
    cancel: &Arc<std::sync::atomic::AtomicBool>,
) -> Option<FileMatch> {
    let metadata = std::fs::metadata(path).ok()?;
    if metadata.len() > max_size_mb * 1024 * 1024 {
        return None;
    }

    let mut sink = FileSink {
        matcher,
        cancel,
        matches: Vec::new(),
    };
    let mut searcher = SearcherBuilder::new()
        .binary_detection(BinaryDetection::quit(b'\x00'))
        .line_number(true)
        .before_context(context)
        .after_context(context)
        .build();

    if searcher.search_path(matcher, path, &mut sink).is_err() {
        return None;
    }
    if sink.matches.is_empty() {
        return None;
    }

    sink.matches.sort_by_key(|m| m.line_number);
    Some(FileMatch {
        path: path.to_path_buf(),
        matches: sink.matches,
    })
}

fn is_bak_file(path: &Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .map(|n| n.ends_with(".bak"))
        .unwrap_or(false)
}

/// Run the search, streaming each matched file via `tx`.
/// Returns true if results were truncated due to `config.max_result_files`.
pub fn search(
    params: &SearchParams,
    config: &Config,
    cancel: std::sync::Arc<std::sync::atomic::AtomicBool>,
    scanned: Arc<std::sync::atomic::AtomicUsize>,
    hits: Arc<std::sync::atomic::AtomicUsize>,
    total: Arc<std::sync::atomic::AtomicUsize>,
    tx: std::sync::mpsc::SyncSender<FileMatch>,
) -> Result<bool> {
    let matcher = build_grep_matcher(params)?;

    // Collect all search roots: primary directory + any additional roots.
    let all_roots: Vec<&str> = std::iter::once(params.directory.as_str())
        .chain(params.roots.iter().map(|s| s.as_str()))
        .filter(|s| !s.is_empty())
        .collect();

    if all_roots.is_empty() {
        anyhow::bail!("No directory specified");
    }
    for root in &all_roots {
        if !Path::new(root).exists() {
            anyhow::bail!("Directory does not exist: {}", root);
        }
    }

    let excludes: Vec<String> = config
        .default_exclude_dirs
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    let mut builder = ignore::WalkBuilder::new(all_roots[0]);
    for root in &all_roots[1..] {
        builder.add(root);
    }
    builder
        .git_ignore(config.respect_gitignore)
        .git_global(config.respect_gitignore)
        .git_exclude(config.respect_gitignore)
        .hidden(false)
        .follow_links(false)
        .threads(config.effective_threads());
    if let Some(depth) = params.max_depth {
        if depth > 0 {
            builder.max_depth(Some(depth));
        }
    }

    let file_glob = params.file_glob.clone();
    let exclude_glob = params.exclude_glob.clone();
    let collected: Arc<Mutex<Vec<PathBuf>>> = Arc::new(Mutex::new(Vec::new()));

    {
        let collected = collected.clone();
        let cancel = cancel.clone();
        let excludes = excludes.clone();
        builder.build_parallel().run(|| {
            let collected = collected.clone();
            let cancel = cancel.clone();
            let excludes = excludes.clone();
            let file_glob = file_glob.clone();
            let exclude_glob = exclude_glob.clone();
            Box::new(move |result| {
                if cancel.load(std::sync::atomic::Ordering::Relaxed) {
                    return WalkState::Quit;
                }
                let entry = match result {
                    Ok(e) => e,
                    Err(_) => return WalkState::Continue,
                };
                let Some(ft) = entry.file_type() else {
                    return WalkState::Continue;
                };
                if ft.is_dir() {
                    let name = entry.file_name().to_str().unwrap_or("");
                    let skip = excludes.iter().any(|ex| {
                        let ex = ex.trim();
                        if ex.is_empty() {
                            return false;
                        }
                        if let Ok(pat) = glob::Pattern::new(ex) {
                            pat.matches(name)
                        } else {
                            name == ex
                        }
                    });
                    return if skip {
                        WalkState::Skip
                    } else {
                        WalkState::Continue
                    };
                }
                if !ft.is_file() {
                    return WalkState::Continue;
                }
                let path = entry.path();
                if is_bak_file(path)
                    || !matches_glob(&file_glob, path)
                    || is_excluded(&exclude_glob, path)
                {
                    return WalkState::Continue;
                }
                collected.lock().unwrap().push(path.to_path_buf());
                WalkState::Continue
            })
        });
    }

    if cancel.load(std::sync::atomic::Ordering::Relaxed) {
        anyhow::bail!("Search cancelled");
    }

    let mut files = std::mem::take(&mut *collected.lock().unwrap());

    // Enforce max result files limit
    let truncated = if config.max_result_files == 0 {
        false
    } else {
        let truncated = files.len() > config.max_result_files;
        if truncated {
            files.truncate(config.max_result_files);
        }
        truncated
    };
    files.sort();

    // Report total file count so the UI can show a determinate progress bar
    total.store(files.len(), std::sync::atomic::Ordering::Relaxed);

    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(config.effective_threads())
        .build()
        .unwrap_or_else(|_| rayon::ThreadPoolBuilder::new().build().unwrap());

    let max_size = config.max_file_size_mb;
    let context = params.context_lines;
    pool.install(|| {
        files.par_iter().for_each_with(tx, |sender, path| {
            if cancel.load(std::sync::atomic::Ordering::Relaxed) {
                return;
            }
            let result = search_file(path, &matcher, max_size, context, &cancel);
            scanned.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            if let Some(fm) = result {
                let h: usize = fm
                    .matches
                    .iter()
                    .filter(|m| m.is_match)
                    .map(|m| m.ranges.len())
                    .sum();
                hits.fetch_add(h, std::sync::atomic::Ordering::Relaxed);
                let _ = sender.send(fm);
            }
        });
    });

    Ok(truncated)
}

pub fn apply_replace(
    file_match: &FileMatch,
    regex: &Regex,
    replace_text: &str,
) -> Result<(String, usize)> {
    let content = std::fs::read_to_string(&file_match.path)
        .with_context(|| format!("Failed to read {}", file_match.path.display()))?;
    let count = regex.find_iter(&content).count();
    Ok((
        regex.replace_all(&content, replace_text).into_owned(),
        count,
    ))
}

pub fn count_total_matches(files: &[FileMatch]) -> usize {
    files
        .iter()
        .map(|f| f.matches.iter().filter(|m| m.is_match).count())
        .sum()
}

pub fn count_match_instances(files: &[FileMatch]) -> usize {
    files
        .iter()
        .map(|f| {
            f.matches
                .iter()
                .filter(|m| m.is_match)
                .map(|m| m.ranges.len())
                .sum::<usize>()
        })
        .sum()
}

fn make_path_safe_under_dir(path: &Path) -> PathBuf {
    let mut safe_path = PathBuf::new();
    for comp in path.components() {
        match comp {
            std::path::Component::Prefix(_) | std::path::Component::RootDir => {}
            std::path::Component::CurDir | std::path::Component::ParentDir => {} // Ignore any directory traversal dots
            std::path::Component::Normal(c) => {
                safe_path.push(c);
            }
        }
    }
    safe_path
}

pub fn backup_file_to(path: &Path, backup_root: &Path, session_dir_name: &str) -> Result<()> {
    let canonical_path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let dest_dir = backup_root.join(session_dir_name);
    let dest_path = dest_dir.join(make_path_safe_under_dir(&canonical_path));
    if let Some(parent) = dest_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::copy(path, dest_path)?;
    Ok(())
}

pub fn cleanup_expired_backups(backup_dir: &str, retention_days: usize) -> Result<()> {
    if backup_dir.is_empty() || retention_days == 0 {
        return Ok(());
    }
    let path = Path::new(backup_dir);
    if !path.exists() {
        return Ok(());
    }
    let now = chrono::Local::now().naive_local();
    let entries = std::fs::read_dir(path)?;
    for entry in entries {
        let Ok(entry) = entry else {
            continue;
        };
        let Ok(ft) = entry.file_type() else {
            continue;
        };
        if ft.is_dir() {
            if let Some(name_str) = entry.file_name().to_str() {
                if let Ok(parsed_dt) =
                    chrono::NaiveDateTime::parse_from_str(name_str, "%Y%m%d-%H%M%S")
                {
                    let duration = now.signed_duration_since(parsed_dt);
                    if duration.num_days() >= retention_days as i64 {
                        let _ = std::fs::remove_dir_all(entry.path());
                    }
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::models::{LineMatch, MatchRange, SearchParams};
    use std::sync::atomic::{AtomicBool, AtomicUsize};
    use tempfile::tempdir;

    // ── helpers ───────────────────────────────────────────────────────────────

    fn test_config() -> Config {
        Config {
            max_threads: 2,
            max_file_size_mb: 50,
            respect_gitignore: false,
            default_exclude_dirs: String::new(),
            max_result_files: 2000,
            ..Config::default()
        }
    }

    fn test_params(dir: &str, pattern: &str) -> SearchParams {
        SearchParams {
            pattern: pattern.to_string(),
            directory: dir.to_string(),
            ..SearchParams::default()
        }
    }

    fn run_search(params: SearchParams, config: Config) -> anyhow::Result<(Vec<FileMatch>, bool)> {
        let cancel = Arc::new(AtomicBool::new(false));
        let scanned = Arc::new(AtomicUsize::new(0));
        let hits = Arc::new(AtomicUsize::new(0));
        let total = Arc::new(AtomicUsize::new(0));
        let (tx, rx) = std::sync::mpsc::sync_channel(100_000);
        let truncated = search(&params, &config, cancel, scanned, hits, total, tx)?;
        let mut files: Vec<FileMatch> = rx.try_iter().collect();
        files.sort_by_key(|f| f.path.clone());
        Ok((files, truncated))
    }

    // ── BL-47: search() integration ──────────────────────────────────────────

    #[test]
    fn test_literal_match() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join("a.txt"), "hello world\nfoo bar\n").unwrap();
        let (files, _) = run_search(
            test_params(dir.path().to_str().unwrap(), "hello"),
            test_config(),
        )
        .unwrap();
        assert_eq!(files.len(), 1);
        let m = &files[0].matches[0];
        assert!(m.is_match);
        assert_eq!(m.line_number, 1);
        assert_eq!(m.ranges[0], MatchRange { start: 0, end: 5 });
    }

    #[test]
    fn test_regex_match() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join("a.txt"), "foo123\nbar\n").unwrap();
        let mut p = test_params(dir.path().to_str().unwrap(), r"foo\d+");
        p.is_regex = true;
        let (files, _) = run_search(p, test_config()).unwrap();
        assert_eq!(files.len(), 1);
        assert!(files[0].matches[0].is_match);
    }

    #[test]
    fn test_case_insensitive_match() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join("a.txt"), "Hello World\n").unwrap();
        let (files, _) = run_search(
            test_params(dir.path().to_str().unwrap(), "hello"),
            test_config(),
        )
        .unwrap();
        assert_eq!(files.len(), 1);
    }

    #[test]
    fn test_case_sensitive_no_match() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join("a.txt"), "Hello World\n").unwrap();
        let mut p = test_params(dir.path().to_str().unwrap(), "hello");
        p.case_sensitive = true;
        let (files, _) = run_search(p, test_config()).unwrap();
        assert_eq!(files.len(), 0);
    }

    #[test]
    fn test_include_glob_filters_extension() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join("a.rs"), "fn hello() {}\n").unwrap();
        std::fs::write(dir.path().join("b.txt"), "hello world\n").unwrap();
        let mut p = test_params(dir.path().to_str().unwrap(), "hello");
        p.file_glob = "*.rs".to_string();
        let (files, _) = run_search(p, test_config()).unwrap();
        assert_eq!(files.len(), 1);
        assert!(files[0].path.to_str().unwrap().ends_with("a.rs"));
    }

    #[test]
    fn test_exclude_glob_skips_dir() {
        let dir = tempdir().unwrap();
        std::fs::create_dir(dir.path().join("node_modules")).unwrap();
        std::fs::write(dir.path().join("node_modules").join("x.js"), "hello\n").unwrap();
        std::fs::write(dir.path().join("main.rs"), "hello\n").unwrap();
        let mut p = test_params(dir.path().to_str().unwrap(), "hello");
        p.exclude_glob = "node_modules".to_string();
        let (files, _) = run_search(p, test_config()).unwrap();
        assert_eq!(files.len(), 1);
        assert!(files[0].path.to_str().unwrap().ends_with("main.rs"));
    }

    #[test]
    fn test_default_excludes_skip_git_and_target() {
        let dir = tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(".git")).unwrap();
        std::fs::write(dir.path().join(".git").join("config"), "hello\n").unwrap();
        std::fs::create_dir_all(dir.path().join("target").join("debug")).unwrap();
        std::fs::write(
            dir.path().join("target").join("debug").join("out"),
            "hello\n",
        )
        .unwrap();
        std::fs::write(dir.path().join("src.rs"), "hello\n").unwrap();
        let mut cfg = test_config();
        cfg.default_exclude_dirs = ".git,target".to_string();
        let (files, _) =
            run_search(test_params(dir.path().to_str().unwrap(), "hello"), cfg).unwrap();
        assert_eq!(files.len(), 1);
        assert!(!files[0].path.to_str().unwrap().contains(".git"));
    }

    #[test]
    fn test_binary_file_skipped() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join("binary.bin"), b"hello\x00world").unwrap();
        std::fs::write(dir.path().join("text.txt"), "hello world\n").unwrap();
        let (files, _) = run_search(
            test_params(dir.path().to_str().unwrap(), "hello"),
            test_config(),
        )
        .unwrap();
        assert_eq!(files.len(), 1);
        assert!(files[0].path.to_str().unwrap().ends_with("text.txt"));
    }

    #[test]
    fn test_max_depth_limits_walk() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join("top.txt"), "hello\n").unwrap();
        std::fs::create_dir(dir.path().join("sub")).unwrap();
        std::fs::write(dir.path().join("sub").join("deep.txt"), "hello\n").unwrap();
        let mut p = test_params(dir.path().to_str().unwrap(), "hello");
        p.max_depth = Some(1);
        let (files, _) = run_search(p, test_config()).unwrap();
        assert_eq!(files.len(), 1);
        assert!(files[0].path.to_str().unwrap().ends_with("top.txt"));
    }

    #[test]
    fn test_context_lines_included() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join("a.txt"), "before\nhello\nafter\n").unwrap();
        let mut p = test_params(dir.path().to_str().unwrap(), "hello");
        p.context_lines = 1;
        let (files, _) = run_search(p, test_config()).unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].matches.len(), 3);
        let match_line = files[0].matches.iter().find(|m| m.is_match).unwrap();
        assert_eq!(match_line.line_number, 2);
    }

    #[test]
    fn test_no_match_returns_empty() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join("a.txt"), "foo bar baz\n").unwrap();
        let (files, _) = run_search(
            test_params(dir.path().to_str().unwrap(), "NOTFOUND"),
            test_config(),
        )
        .unwrap();
        assert_eq!(files.len(), 0);
    }

    #[test]
    fn test_cancel_stops_search() {
        let dir = tempdir().unwrap();
        for i in 0..100 {
            std::fs::write(dir.path().join(format!("f{i}.txt")), "hello\n").unwrap();
        }
        let p = test_params(dir.path().to_str().unwrap(), "hello");
        let cancel = Arc::new(AtomicBool::new(true)); // pre-cancelled
        let (tx, rx) = std::sync::mpsc::sync_channel(100_000);
        let result = search(
            &p,
            &test_config(),
            cancel,
            Arc::new(AtomicUsize::new(0)),
            Arc::new(AtomicUsize::new(0)),
            Arc::new(AtomicUsize::new(0)),
            tx,
        );
        let files: Vec<FileMatch> = rx.try_iter().collect();
        assert!(
            result.is_err() || files.len() < 100,
            "expected cancel to short-circuit"
        );
    }

    #[test]
    fn test_max_result_files_limit_and_unlimited() {
        let dir = tempdir().unwrap();
        for i in 0..5 {
            std::fs::write(dir.path().join(format!("f{i}.txt")), "hello\n").unwrap();
        }
        let p = test_params(dir.path().to_str().unwrap(), "hello");

        // 1. With max_result_files = 2, it should truncate to 2 and return truncated = true
        let mut cfg_limited = test_config();
        cfg_limited.max_result_files = 2;
        let (files_limited, truncated_limited) = run_search(p.clone(), cfg_limited).unwrap();
        assert_eq!(files_limited.len(), 2);
        assert!(truncated_limited);

        // 2. With max_result_files = 0 (unlimited), it should NOT truncate (returns 5) and return truncated = false
        let mut cfg_unlimited = test_config();
        cfg_unlimited.max_result_files = 0;
        let (files_unlimited, truncated_unlimited) = run_search(p, cfg_unlimited).unwrap();
        assert_eq!(files_unlimited.len(), 5);
        assert!(!truncated_unlimited);
    }

    #[test]
    fn test_multi_root_searches_all_roots() {
        let dir1 = tempdir().unwrap();
        let dir2 = tempdir().unwrap();
        std::fs::write(dir1.path().join("a.txt"), "hello from root1\n").unwrap();
        std::fs::write(dir2.path().join("b.txt"), "hello from root2\n").unwrap();
        let mut p = test_params(dir1.path().to_str().unwrap(), "hello");
        p.roots = vec![dir2.path().to_str().unwrap().to_string()];
        let (files, _) = run_search(p, test_config()).unwrap();
        assert_eq!(files.len(), 2, "expected files from both roots");
        let paths: Vec<_> = files
            .iter()
            .map(|f| f.path.file_name().unwrap().to_str().unwrap())
            .collect();
        assert!(paths.contains(&"a.txt"));
        assert!(paths.contains(&"b.txt"));
    }

    #[test]
    fn test_multibyte_match_ranges_are_char_boundaries() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join("jp.txt"), "日本語テスト\n").unwrap();
        let (files, _) = run_search(
            test_params(dir.path().to_str().unwrap(), "テスト"),
            test_config(),
        )
        .unwrap();
        assert_eq!(files.len(), 1);
        let m = &files[0].matches[0];
        assert!(m.is_match);
        assert!(!m.ranges.is_empty());
        let content = &m.content;
        for r in &m.ranges {
            assert!(
                content.is_char_boundary(r.start),
                "start is not char boundary"
            );
            assert!(content.is_char_boundary(r.end), "end is not char boundary");
        }
    }

    // ── BL-49: pure-function unit tests ───────────────────────────────────────

    #[test]
    fn test_build_regex_literal_escapes() {
        // dots and parens should be escaped in literal mode
        let p = SearchParams {
            pattern: "foo.bar(baz)".to_string(),
            is_regex: false,
            ..SearchParams::default()
        };
        let re = build_regex(&p).unwrap();
        assert!(re.is_match("foo.bar(baz)"));
        assert!(!re.is_match("fooXbar_baz"));
    }

    #[test]
    fn test_build_regex_case_insensitive() {
        let p = SearchParams {
            pattern: "hello".to_string(),
            case_sensitive: false,
            ..SearchParams::default()
        };
        let re = build_regex(&p).unwrap();
        assert!(re.is_match("Hello"));
        assert!(re.is_match("HELLO"));
    }

    #[test]
    fn test_build_regex_word_boundary() {
        let p = SearchParams {
            pattern: "foo".to_string(),
            word_boundary: true,
            ..SearchParams::default()
        };
        let re = build_regex(&p).unwrap();
        assert!(re.is_match("foo bar"));
        assert!(!re.is_match("foobar"));
    }

    #[test]
    fn test_build_regex_invalid_returns_err() {
        let p = SearchParams {
            pattern: "[unclosed".to_string(),
            is_regex: true,
            ..SearchParams::default()
        };
        assert!(build_regex(&p).is_err());
    }

    #[test]
    fn test_is_excluded_matches_component() {
        assert!(is_excluded(
            "node_modules",
            Path::new("/project/node_modules/x.js")
        ));
        assert!(!is_excluded("node_modules", Path::new("/project/src/x.js")));
    }

    #[test]
    fn test_is_excluded_glob_pattern() {
        assert!(is_excluded("*.min.js", Path::new("/project/app.min.js")));
        assert!(!is_excluded("*.min.js", Path::new("/project/app.js")));
    }

    #[test]
    fn test_is_excluded_empty_is_false() {
        assert!(!is_excluded("", Path::new("/project/any.rs")));
    }

    #[test]
    fn test_matches_glob_empty_allows_all() {
        assert!(matches_glob("", Path::new("anything.txt")));
    }

    #[test]
    fn test_matches_glob_single_pattern() {
        assert!(matches_glob("*.rs", Path::new("main.rs")));
        assert!(!matches_glob("*.rs", Path::new("main.txt")));
    }

    #[test]
    fn test_matches_glob_comma_separated() {
        let glob = "*.js,*.ts,*.jsx,*.tsx";
        assert!(matches_glob(glob, Path::new("app.ts")));
        assert!(matches_glob(glob, Path::new("component.tsx")));
        assert!(!matches_glob(glob, Path::new("main.py")));
    }

    #[test]
    fn test_count_match_instances() {
        let files = vec![
            FileMatch {
                path: PathBuf::from("dummy1.txt"),
                matches: vec![
                    LineMatch {
                        line_number: 1,
                        content: "hello world".to_string(),
                        ranges: vec![MatchRange { start: 0, end: 5 }],
                        is_match: true,
                    },
                    LineMatch {
                        line_number: 2,
                        content: "context line".to_string(),
                        ranges: vec![],
                        is_match: false,
                    },
                ],
            },
            FileMatch {
                path: PathBuf::from("dummy2.txt"),
                matches: vec![LineMatch {
                    line_number: 1,
                    content: "test test".to_string(),
                    ranges: vec![
                        MatchRange { start: 0, end: 4 },
                        MatchRange { start: 5, end: 9 },
                    ],
                    is_match: true,
                }],
            },
        ];
        assert_eq!(count_match_instances(&files), 3);
        assert_eq!(count_total_matches(&files), 2); // 2 match lines (context excluded)
    }

    // ── BL-48: replace & backup ───────────────────────────────────────────────

    #[test]
    fn test_backup_creates_bak_file() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("original.txt");
        std::fs::write(&file, "original content").unwrap();
        let backup_root = dir.path().join("backups");
        backup_file_to(&file, &backup_root, "session1").unwrap();

        let canonical_file = file.canonicalize().unwrap();
        let safe_rel = make_path_safe_under_dir(&canonical_file);
        let bak = backup_root.join("session1").join(safe_rel);
        assert!(bak.exists());
        assert_eq!(std::fs::read_to_string(bak).unwrap(), "original content");
    }

    #[test]
    fn test_backup_prevents_directory_traversal() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("original.txt");
        std::fs::write(&file, "original content").unwrap();
        let backup_root = dir.path().join("backups");

        let sub = dir.path().join("sub");
        std::fs::create_dir(&sub).unwrap();
        let relative_with_dots = sub.join("../original.txt");
        backup_file_to(&relative_with_dots, &backup_root, "session_traversal").unwrap();

        let canonical_file = file.canonicalize().unwrap();
        let safe_rel = make_path_safe_under_dir(&canonical_file);
        let bak = backup_root.join("session_traversal").join(safe_rel);
        assert!(bak.exists());
        assert!(!backup_root.join("original.txt").exists());
    }

    #[test]
    fn test_cleanup_expired_backups() {
        let dir = tempdir().unwrap();
        let backup_root = dir.path().join("backups");
        std::fs::create_dir_all(&backup_root).unwrap();

        // 1. Create a directory that is 10 days old
        let old_dt = chrono::Local::now().naive_local() - chrono::Duration::days(10);
        let old_name = old_dt.format("%Y%m%d-%H%M%S").to_string();
        let old_dir = backup_root.join(&old_name);
        std::fs::create_dir_all(&old_dir).unwrap();
        std::fs::write(old_dir.join("old_file.txt"), "old").unwrap();

        // 2. Create a directory that is fresh (now)
        let fresh_dt = chrono::Local::now().naive_local();
        let fresh_name = fresh_dt.format("%Y%m%d-%H%M%S").to_string();
        let fresh_dir = backup_root.join(&fresh_name);
        std::fs::create_dir_all(&fresh_dir).unwrap();
        std::fs::write(fresh_dir.join("fresh_file.txt"), "fresh").unwrap();

        // 3. Clean up with 7 days retention
        cleanup_expired_backups(&backup_root.to_string_lossy(), 7).unwrap();

        // Old directory should be deleted, fresh directory should remain
        assert!(!old_dir.exists());
        assert!(fresh_dir.exists());
    }

    #[test]
    fn test_apply_replace_substitutes_text() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("code.rs");
        std::fs::write(&file, "let foo = 1;\nlet foo2 = 2;\n").unwrap();
        let re = build_regex(&SearchParams {
            pattern: "foo".to_string(),
            directory: dir.path().to_str().unwrap().to_string(),
            ..SearchParams::default()
        })
        .unwrap();
        let fm = FileMatch {
            path: file,
            matches: vec![],
        };
        let (result, count) = apply_replace(&fm, &re, "bar").unwrap();
        assert_eq!(result, "let bar = 1;\nlet bar2 = 2;\n");
        assert_eq!(count, 2);
    }

    #[test]
    fn test_apply_replace_actual_count_differs_from_snapshot() {
        // File on disk has 3 matches; snapshot has 1 — reported count must be 3 (actual).
        let dir = tempdir().unwrap();
        let file = dir.path().join("data.txt");
        std::fs::write(&file, "foo foo foo").unwrap();
        let re = build_regex(&SearchParams {
            pattern: "foo".to_string(),
            directory: dir.path().to_str().unwrap().to_string(),
            ..SearchParams::default()
        })
        .unwrap();
        let fm = FileMatch {
            path: file,
            matches: vec![], // snapshot intentionally empty
        };
        let (result, count) = apply_replace(&fm, &re, "bar").unwrap();
        assert_eq!(result, "bar bar bar");
        assert_eq!(count, 3);
    }

    #[test]
    fn test_is_bak_file() {
        assert!(is_bak_file(Path::new("/tmp/foo.txt.bak")));
        assert!(is_bak_file(Path::new("file.bak")));
        assert!(!is_bak_file(Path::new("file.txt")));
        assert!(!is_bak_file(Path::new("backup")));
    }
}
