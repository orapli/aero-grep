# Testing Strategy

This project follows the **Testing Trophy** model (Kent C. Dodds): invest where
confidence-per-cost is highest. For this codebase that ordering is:

```
static  <  unit  <<  integration  >  e2e
(free)    (few)     (THICKEST)      (deferred)
```

Goal: **a minimal set that protects behaviour we'd be sad to break, across
refactors** — not a coverage percentage. Target total ≈ 20–30 tests.

---

## Layers in this project

| Layer | What it covers here | Tooling | Status |
|-------|---------------------|---------|--------|
| **Static** | `cargo build`, `clippy -D warnings`, `fmt --check`, the type system | compiler / CI | partial (no CI yet) |
| **Unit** | Pure functions: `build_regex`, `is_excluded`, `matches_glob`, `count_*`, char-boundary helpers, `common_prefix/suffix_len`, `truncate_path`, `short_dir`, `format_ts` | `#[cfg(test)]` in-module | **good** |
| **Integration** (thickest) | `search()` pipeline over a temp tree, `apply_replace` + `backup_file`, `Config`/`History` serde round-trip & backward-compat | `tempfile` | **good for search/replace; gaps in History & app state** |
| **E2E** | egui GUI interaction | `egui_kittest` | **deferred — see below** |

Run everything with `cargo test`. Tests must pass under parallel execution
(no shared fixed-path fixtures — always use `tempfile::tempdir()`).

---

## Current inventory (51 tests)

- `grep.rs` — search pipeline (literal/regex/case/glob/exclude/default-excludes/
  binary/depth/context/no-match/cancel/multi-root/multibyte), pure helpers
  (`build_regex`, `is_excluded`, `matches_glob`, `count_*`), replace & backup.
- `app.rs` — char-boundary helpers, BL-17 multibyte truncate regression,
  `short_dir`, `format_ts`, intra-line diff helpers (`common_prefix/suffix_len`),
  `truncate_path` (incl. multibyte no-panic).
- `config.rs` — serde round-trip, missing-field backward-compat defaults.
- `history.rs` — `set_limit`/`next_id` trim, `SearchResult` serde round-trip &
  backward-compat.

---

## Known gaps & smells (to address — see `doc/test-plan.md`)

1. **`History` mutators are untestable (smell).**
   `push`/`remove`/`clear` call `self.save()`, which writes to the *real* user
   config dir (`dirs::config_dir()`). Tests can't call them without polluting the
   developer's real history, so `test_push_prepends_and_trims` **re-implements**
   the logic instead of calling `push` — it verifies nothing. Root cause: I/O is
   coupled to logic. Fix: separate the in-memory mutation from persistence.

2. **`GrepApp` state machine has no tests.**
   Search lifecycle (`start_search` → `poll_search`/`drain_search_rx` →
   `finalize_search`), `load_history_entry`, tab management
   (`new_empty_tab`/`switch_to_tab`/`close_tab`), and the replace flow
   (`do_replace_preview` → `do_replace_all` → `execute_replace`) are pure state
   transitions but untested, partly because `GrepApp::new` needs an
   `eframe::CreationContext` and the constructor reads global config from disk.

3. **No CI / static gate.** `clippy -D warnings` and `fmt --check` aren't enforced.

---

## E2E decision: deferred (documented rationale)

E2E for egui means **`egui_kittest`** (headless AccessKit harness — it does *not*
open a real window, so the macOS debug-crash issue does NOT apply).

Per the Testing Trophy, the highest-value coverage is the **state machine** (gap #2
above) via **headless logic-level integration tests of `GrepApp` without any GUI
harness** — these capture most "does the app behave" risk at a fraction of E2E's
cost.

**Plan of record:**
1. Do the decoupling refactor + `GrepApp` state-machine tests (`doc/test-plan.md`
   TP-1…TP-3) — the main confidence win, harness-free.
2. *Then* add one `egui_kittest = "0.34"` E2E smoke test (type pattern → search
   temp dir → assert results render in the AccessKit tree). Single smoke test.

---

## Conventions

- Pure functions → in-module `#[cfg(test)] mod tests`.
- Anything touching the filesystem → `tempfile::tempdir()`, never a shared path.
- One behaviour per test; name `test_<behaviour>`.
- Multibyte (Japanese) inputs for any code that slices strings by byte offset —
  this codebase has repeatedly hit char-boundary panics (BL-17, BL-69).
- Don't test private impl details that change with refactors; test observable
  behaviour and serde compatibility (the backward-compat guarantee is a contract).
