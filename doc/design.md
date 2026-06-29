# Design Reference

Internal notes on architecture decisions, pitfalls, and conventions that are
not obvious from reading the code.

---

## Color Palette (`Pal`)

All colors are defined in `src/app.rs` in the `Pal` struct. Three themes are
available: Dark (Catppuccin Mocha), Light (Catppuccin Latte), HighContrast.

### Field semantics

| Field | Purpose |
|-------|---------|
| `bg_base` | Window / panel background |
| `bg_mantle` | Tab bar, section headers, toolbar frame |
| `bg_surface0` | Raised surfaces (active tab, ghost button hover, code frame) |
| `bg_surface1` | Current match line highlight background |
| `bg_overlay0` | Unused overlay level; reserved |
| `text` | Primary body text, code content |
| `subtext` | Secondary labels, inactive tab text, settings labels |
| `muted` | Tertiary info: context lines, match counts, section separators |
| `placeholder` | Hint text in empty input fields — intentionally very dim |
| `accent` | Interactive highlight: selected file, active tab, search badge, focus ring |
| `yellow` | General purpose warm emphasis (warning messages) |
| `green` | Success / backup confirmation |
| `red` | Destructive actions (Replace / Confirm Replace buttons, delete icons) |
| `match_bg` | Semi-transparent background behind matched words |
| `match_text` | Text color for matched words; differs per theme for contrast |

### Contrast rationale (WCAG AA guidance)

| Theme | Key pair | Ratio |
|-------|----------|-------|
| Light | `accent` on `bg_surface0` (active tab) | 4.9:1 ✓ |
| Light | `accent` on `bg_base` (selected file) | 6.5:1 ✓ |
| Light | `muted` on `bg_base` (context lines) | 3.6:1 (acceptable for de-emphasized secondary text) |
| Light | `match_text` on `match_bg` (match word) | 4.9:1 ✓ |
| Dark | `match_text` on `match_bg` | 5.6:1 ✓ |
| All | `placeholder` vs `text` on bg | ~4× difference — clearly dim hints |

`placeholder` is intentionally below 3:1 to look clearly different from actual
input values. Users need to see it but should instantly recognise it as an example.

---

## egui Pitfalls

Traps discovered through implementation. Read before touching the layout code.

### 1. `right_to_left` inside `Frame::none()` consumes all remaining height

`with_layout(right_to_left(Align::Center))` inside `Frame::none().show()` sets
`cross_justify = true`, which expands the inner `Ui` to the full available height.
This eats up the space below and makes the file list disappear.

**Fix:** Use `ui.horizontal()` + explicit width reservation (`(available_width - reserve).max(min)`) instead.

### 2. `desired_width(available_width() - N)` causes a positive-feedback loop

Every frame: widget measures available width → subtracts N → sets desired width
→ egui expands the panel to fit → available width grows → repeat → panel grows
indefinitely.

**Fix:** Reserve a *constant* right-side budget and compute input width once:
```rust
let reserve = 100.0_f32;
let input_w = (ui.available_width() - reserve).max(120.0);
```

### 3. Icon glyphs — use the bundled `codicon.ttf` only

`assets/codicon.ttf` is loaded in `setup_fonts` as a separate font family.
Codicon codepoints are defined as constants in `app.rs::icons`.

**Never** add new icons from system fonts or emoji — rendering is
environment-dependent and will fail on some OS / egui builds.

To add a new glyph: look up its codepoint in the
[Codicon reference](https://microsoft.github.io/vscode-codicons/dist/codicon.html)
and add a constant to `icons::`.

### 4. Theme / font applied every frame → infinite repaint loop

`apply_theme` and `apply_font_size` call `ctx.set_visuals` / `ctx.set_style`,
which request a repaint. If called unconditionally every frame, the app repaints
forever at 100% CPU.

**Fix:** Track `applied_theme` and `applied_font_size` fields; only call apply
when the value has changed (`ensure_theme_applied`).

### 6. `DragValue` height exceeds `interact_size.y` when `button_padding.y > 0`

A `DragValue` renders as a button with intrinsic height
`galley_height + 2 * button_padding.y`. With the default `button_padding.y = 1`,
the button is 19 px tall while the row height (`interact_size.y`) is 18 px.
egui's anti-overlap logic then shifts it 1 px downward, misaligning it with
adjacent labels.

**Fix:** Zero `button_padding.y` around `DragValue` calls:
```rust
let saved = ui.spacing().button_padding;
ui.spacing_mut().button_padding.y = 0.0;
ui.add(egui::DragValue::new(&mut val)…);
ui.spacing_mut().button_padding = saved;
```

---

## Testing Strategy

Testing Trophy allocation: **Integration tests > Unit tests >> E2E (none)**.

- **Integration tests** (`grep.rs`, `history.rs`): use `tempfile::tempdir()` for
  hermetic FS fixtures. Cover `search()` pipeline, replace + backup, and
  `Config`/`History` serde round-trips (backward-compat guard).
- **Unit tests**: pure functions only — `build_regex`, `is_excluded`,
  `matches_glob`, `is_binary`, `count_*`, char-boundary truncation, `short_dir`,
  `format_ts`.
- **E2E (GUI)**: none currently. GUI is confirmed by release build + screenshot on host.
  Debug builds crash on macOS due to a winit/icrate ABI issue; always run release on macOS.

---

## Icon Reference (Codicon)

Selected glyphs in use (`app.rs::icons`):

| Constant | Codepoint | Visual |
|----------|-----------|--------|
| `SEARCH` | U+EA6D | magnifying glass |
| `SETTINGS` | U+EB51 | gear |
| `HISTORY` | U+EA82 | clock |
| `REPLACE` | U+EA64 | replace arrows |
| `COPY` | U+EA78 | clipboard |
| `PLAY` | U+EA72 | triangle play |
| `TRASH` | U+EA81 | trash bin |
| `FOLDER` | U+EA83 | folder |
| `ADD` | U+EA60 | plus |
| `CLOSE` | U+EA76 | × |
| `EXPORT` | U+EA98 | upload arrow |
| `WRAP` | U+EB80 | word wrap (on) |
| `WRAP_OFF` | U+EB25 | no-newline (off) |
| `LIST_FLAT` | U+EB84 | flat list |
| `LIST_TREE` | U+EB86 | tree view |
| `CHEVRON_RIGHT` | U+EAB6 | › |
| `CHEVRON_DOWN` | U+EAB4 | ˅ |
