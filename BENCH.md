# Performance Baseline — aero-grep vs rg

This document describes how to measure aero-grep's search performance and compare it with ripgrep.

## Quick run

```bash
# Release build (required for fair comparison)
cargo build --bin bench --release

# Usage: ./target/release/bench <directory> <pattern> [--regex] [--case] [--word]
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
