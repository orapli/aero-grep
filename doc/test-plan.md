# Test Improvement Plan (for implementation)

Hand-off spec for the remaining test work identified in `doc/testing.md`.
Read `doc/testing.md` first for the Testing Trophy rationale. Each task below is
a `cargo build` + `cargo test`-passing unit. Do them in order; TP-1 unblocks TP-3.

---

## TP-1: Decouple `History` persistence from logic (fixes the test smell)

**Why:** `History::push/remove/clear` call `self.save()` → writes to the real
user config dir. Tests can't exercise them, so `test_push_prepends_and_trims`
re-implements the logic and verifies nothing.

**Approach (minimal, backward-compatible):**
- Split each mutator into a pure in-memory part and a persisting wrapper. E.g.:
  ```rust
  // pure: no I/O, returns nothing, fully testable
  fn push_entry(&mut self, result: SearchResult) {
      self.entries.insert(0, result);
      self.entries.truncate(self.limit);
  }
  pub fn push(&mut self, result: SearchResult) {
      self.push_entry(result);
      let _ = self.save();
  }
  ```
  Do the same for `remove`/`clear` (`remove_entry`/`clear_entries`).
- Keep the public API (`push`/`remove`/`clear`) unchanged so `app.rs` callers
  are untouched.

**Tests to add (replace the simulated one):**
- `test_push_entry_prepends_newest` — call the real `push_entry`, assert order.
- `test_push_entry_trims_to_limit` — push > limit, assert length == limit and
  the oldest is dropped.
- `test_remove_entry_by_id`, `test_clear_entries_empties`.
- Delete/replace `test_push_prepends_and_trims` (the one that fakes the logic).

---

## TP-2: `GrepApp` headless test constructor

**Why:** TP-3 needs to build a `GrepApp` without an `eframe::CreationContext`
and without reading the developer's real `config.json` / `history.json`.

**Approach:**
- Add a `#[cfg(test)]` constructor, e.g.:
  ```rust
  #[cfg(test)]
  pub fn new_for_test(config: Config) -> Self {
      let pal = Pal::from_theme(&config.theme);
      // build the same struct as `new`, but:
      //  - take `config` as an argument (no Config::load())
      //  - history = History { entries: vec![], limit: config.history_limit }
      //    (no History::load() disk read)
      //  - skip apply_theme/apply_font_size/setup_fonts (need an egui Context)
      ...
  }
  ```
- Factor the shared struct-literal init out of `new` if it reduces duplication,
  but do **not** change the production `new(cc)` behaviour.
- History built in test must not write to disk: with TP-1 done, the test
  constructor simply never calls a persisting method (or set limit and use
  `push_entry`).

**Note:** This needs `pub(crate)` or in-module visibility only; no production API
changes. If a few fields require it, keep them crate-private.

---

## TP-3: `GrepApp` state-machine integration tests (the main E2E substitute)

**Why:** This is where "does the app actually work" risk lives, and it's all
pure state transitions — no GUI harness needed. Covers what an E2E smoke test
would, at a fraction of the cost.

**Setup:** `tempfile::tempdir()` fixture tree + `GrepApp::new_for_test(cfg)`.
Drive the non-UI methods directly. Search runs on a background thread + channel,
so tests must pump: call `start_search`, then loop `poll_search`/`drain_search_rx`
(with a bounded `std::thread::sleep` + timeout) until `search_state` is `Done`.
Add a small test helper `run_search_to_completion(&mut app)` for this.

**Behaviours to lock (one test each):**
1. **Search populates a result** — set `params` to a temp dir + pattern, run to
   completion, assert `current_result` is `Some` with the expected file/match
   counts. (Mirrors the `grep::search` integration tests but through the app.)
2. **Search creates/updates the active tab** — after a search, `tabs` has one
   entry; a second search with different params updates or adds per current
   semantics (assert whichever the code does — pin the contract).
3. **`load_history_entry` restores params + result** without re-running search,
   and sets `current_result`/`selected_files` consistently.
4. **Tab lifecycle** — `new_empty_tab` then `switch_to_tab`/`close_tab` keep
   `active_tab` valid (no out-of-bounds; closing active picks a neighbour).
5. **Replace preview is non-destructive** — `do_replace_preview` populates
   `replace_preview` but does not modify files on disk (assert file bytes
   unchanged). Use a temp file.
6. **`execute_replace` writes + backs up** — run a real replace against a temp
   file, assert content changed and `.bak` exists (overlaps `grep` tests but
   exercises the app-level scope/snapshot path: `do_replace_all` →
   `replace_confirm_snapshot` → `execute_replace`).

Keep these hermetic and deterministic. If thread timing makes #1–#2 flaky,
prefer exposing a synchronous `search_blocking` test seam over `sleep`.

---

## TP-4: CI / static gate (low effort, high leverage)

- Add a GitHub Actions workflow running `cargo fmt --check`,
  `cargo clippy -- -D warnings`, `cargo test` on push/PR.
- Do **not** gate on coverage. Coverage (`cargo llvm-cov`) optional, for
  visibility only.
- Fix any `clippy -D warnings` findings surfaced (likely a few in `app.rs`).

---

## TP-5: `egui_kittest` E2E smoke test (after TP-1…TP-3)

Add `egui_kittest = "0.34"` as a dev-dependency and write
**one** smoke test: drive `GrepApp` (via the TP-2 test constructor) through the
kittest `Harness`, type a pattern, point at a `tempfile` tree, run the search,
and assert the results appear in the AccessKit tree. One smoke test, not a suite
— per `doc/testing.md`, the state-machine tests (TP-3) carry the main load.

## Explicitly out of scope

- A full E2E suite — TP-5 is a single smoke test; deeper behaviour is covered by
  TP-3 headless state-machine tests.
- Snapshot/visual-regression tests of rendering — not worth it for this app.

---

## Already done (this pass, by Opus)

- Added unit tests for the intra-line diff helpers `common_prefix_len` /
  `common_suffix_len` incl. a multibyte no-panic regression mirroring the
  replace-preview slicing (BL-69 / BL-17 class).
- Added `truncate_path` tests incl. multibyte no-panic.
- (51 tests total, all passing.)
