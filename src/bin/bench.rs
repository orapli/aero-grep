/// Minimal benchmark binary: measures grep.rs::search() against `rg`.
///
/// Usage: cargo run --bin bench --release -- <dir> <pattern> [--regex] [--case] [--word] [--no-limit]
///
/// Reports elapsed time, file count, and match counts so you can compare with:
///   time rg [opts] '<pattern>' '<dir>'
///
/// `--no-limit` disables `max_result_files` (default config caps at 2000).
/// Use it when comparing against `rg` (which has no such cap) on corpora
/// with more than 2000 matching files — otherwise aero-grep silently
/// searches fewer files than rg and the timings aren't comparable.
#[path = "../config.rs"]
pub mod config;
#[path = "../grep.rs"]
pub mod grep;
#[path = "../models.rs"]
pub mod models;

use config::Config;
use grep::search;
use models::{FileMatch, SearchParams};
use std::sync::{
    atomic::{AtomicBool, AtomicUsize},
    Arc,
};
use std::time::Instant;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!(
            "Usage: {} <directory> <pattern> [--regex] [--case] [--word] [--no-limit]",
            args[0]
        );
        std::process::exit(1);
    }
    let dir = &args[1];
    let pattern = &args[2];
    let is_regex = args.iter().any(|a| a == "--regex");
    let case_sensitive = args.iter().any(|a| a == "--case");
    let word_boundary = args.iter().any(|a| a == "--word");
    let no_limit = args.iter().any(|a| a == "--no-limit");

    let config = Config {
        max_threads: std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4),
        max_result_files: if no_limit {
            0
        } else {
            Config::default().max_result_files
        },
        ..Config::default()
    };

    let params = SearchParams {
        pattern: pattern.clone(),
        directory: dir.clone(),
        is_regex,
        case_sensitive,
        word_boundary,
        ..SearchParams::default()
    };

    // Warm-up (discarded)
    run_once(&params, &config);

    // Timed run
    let t0 = Instant::now();
    let (files, truncated) = run_once(&params, &config);
    let elapsed = t0.elapsed();

    let match_lines: usize = files
        .iter()
        .map(|f| f.matches.iter().filter(|m| m.is_match).count())
        .sum();
    let occurrences: usize = files
        .iter()
        .map(|f| {
            f.matches
                .iter()
                .filter(|m| m.is_match)
                .map(|m| m.ranges.len())
                .sum::<usize>()
        })
        .sum();

    println!(
        "aero-grep  {:.1} ms  |  {} files  /  {} match-lines  /  {} occurrences{}",
        elapsed.as_secs_f64() * 1000.0,
        files.len(),
        match_lines,
        occurrences,
        if truncated { "  (TRUNCATED)" } else { "" },
    );
    println!(
        "Compare:   time rg {}{}{}'{}' '{}'",
        if !is_regex { "--fixed-strings " } else { "" },
        if case_sensitive {
            "--case-sensitive "
        } else {
            ""
        },
        if word_boundary { "--word-regexp " } else { "" },
        pattern,
        dir,
    );
}

fn run_once(params: &SearchParams, config: &Config) -> (Vec<FileMatch>, bool) {
    let (tx, rx) = std::sync::mpsc::sync_channel(100_000);
    let truncated = search(
        params,
        config,
        Arc::new(AtomicBool::new(false)),
        Arc::new(AtomicUsize::new(0)),
        Arc::new(AtomicUsize::new(0)),
        Arc::new(AtomicUsize::new(0)),
        tx,
    )
    .unwrap_or(false);
    (rx.try_iter().collect(), truncated)
}
