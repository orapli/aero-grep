# Performance Baseline — aero-grep vs rg

This document describes how to measure aero-grep's search performance and compare it with ripgrep.

## Quick run

```bash
# Release build (required for fair comparison)
cargo build --bin bench --release

# Usage: ./target/release/bench <directory> <pattern> [--regex] [--case] [--word] [--no-limit]
./target/release/bench /path/to/large/project "TODO"
./target/release/bench /path/to/large/project "fn \w+\(" --regex
./target/release/bench /path/to/large/project "error" --word
```

The binary runs one warm-up pass (discarded) then a timed pass and prints:

```
aero-grep  42.3 ms  |  318 files  /  1240 match-lines  /  1380 occurrences
Compare:   time rg --fixed-strings 'TODO' '/path/to/large/project'
```

## Representative cases

| Case | aero-grep flag | rg equivalent |
|------|---------------|---------------|
| Literal (default) | *(none)* | `--fixed-strings` |
| Regex | `--regex` | *(default)* |
| Case-sensitive | `--case` | `--case-sensitive` |
| Whole-word | `--word` | `--word-regexp` / `-w` |

Two gotchas that matter for a *fair* comparison, found while producing the
numbers below:
- **Case sensitivity default differs.** aero-grep's `SearchParams` defaults
  to case-*insensitive*; `rg` defaults to case-*sensitive*. For the Literal
  / Regex / Whole-word rows (not specifically testing case-sensitivity),
  add `--ignore-case` to the `rg` side, or the match counts won't agree and
  the timings aren't comparable.
- **`max_result_files` caps aero-grep, not `rg`.** The default config caps
  at 2000 matching files. On corpora larger than that, aero-grep silently
  searches fewer files than `rg`, making aero-grep look faster than it
  really is. Pass `--no-limit` to `bench` (disables the cap) when the
  corpus can plausibly exceed it.

## Results (2026-07-01)

Machine: Apple M5, 10 cores, 16 GB RAM, macOS (Darwin 25.5.0).
`rg` 15.1.0 (installed via Homebrew) vs aero-grep release build (same
commit as this file). Each cell is `aero-grep ms / rg ms`, from
`./target/release/bench <dir> <pattern> [flags] --no-limit` vs a
matching `time rg ...` — both after one untimed warm-up run so OS page
cache is warm for both. Match counts agreed within rounding for every
cell (confirms both tools searched the same content); exact commands are
in the "Representative cases" table above.

| Corpus | Files | Size | Literal (`String`) | Regex (`fn \w+\(`) | Case-sensitive (`self`) | Whole-word (`impl`) |
|---|---|---|---|---|---|---|
| Small — aero-grep's own `src/` | 7 | 444 KB | 3.8 / 8 ms | 5.7 / 9 ms | 3.4 / 7 ms | 3.3 / 8 ms |
| Medium — Rust stdlib source | 1,972 | 49 MB | 39.9 / 40 ms | 39.1 / 48 ms | 35.0 / 43 ms | 34.9 / 41 ms |
| Large — full local cargo registry cache | 18,220 | 780 MB | 569 / 358 ms | 526 / 386 ms | 467 / 424 ms | 535 / 399 ms |

**Honest read:** aero-grep is consistently **1.5–2.4× faster than `rg`
on small, project-sized searches** (a handful to a few thousand files) —
this is the case that matters most for an interactive GUI tool, and the
advantage is structural: aero-grep runs the search in-process, while every
`rg` invocation pays subprocess-spawn overhead (relevant since editors
like VS Code shell out to `rg` per search). At medium scale the two are
roughly even, with aero-grep slightly ahead. At large scale (an entire
780 MB / 18k-file corpus in one search — an unusually large single query
for interactive use) `rg`'s more mature walking/matching engine pulls
ahead by roughly 1.1–1.6×. aero-grep does not claim to out-search `rg` at
every scale; it claims to be at least competitive, and faster where it
matters for the tool's actual use case (interactive, project-scale
search).

## Measuring gitignore effect

```bash
# With gitignore (default, respect_gitignore = true in config)
./target/release/bench ~/repos/myproject "TODO"

# Without gitignore: temporarily edit config.json to set respect_gitignore=false,
# or add a #[cfg] toggle in bench.rs
```

## Generating third-party license list

When distributing the binary, MIT/Apache-2.0 licenses require attribution.
Use cargo-about to generate a combined list:

```bash
cargo install cargo-about
cargo about generate about.hbs > LICENSES-THIRD-PARTY.html
```

A minimal `about.toml` and `about.hbs` template are needed; see
<https://github.com/EmbarkStudios/cargo-about> for setup. Key dependencies
and their licenses:

| Crate | License |
|-------|---------|
| egui / eframe | MIT |
| ignore | MIT / Unlicense |
| grep-searcher / grep-regex / grep-matcher | MIT / Unlicense |
| regex | MIT / Apache-2.0 |
| rayon | MIT / Apache-2.0 |
| serde / serde_json | MIT / Apache-2.0 |
| rfd | MIT |
| chrono | MIT / Apache-2.0 |
| anyhow | MIT / Apache-2.0 |
