# aero-grep

A fast, cross-project full-text search GUI built with Rust and [egui](https://github.com/emilk/egui).

## Features

- **ripgrep-powered search** — uses the same battle-tested libraries as ripgrep for `.gitignore` support, encoding detection, binary-file skipping, and whole-word matching
- **Multiple result tabs** — run several searches side by side
- **Tree / Flat view** — browse matched files as a folder tree or a flat list
- **Replace with preview** — per-file intra-line diff highlighting before any file is written
- **Backup on Replace** — automated backups before replace operations, with configurable backup directories and customizable retention policy (cleanup)
- **Context lines** — show N lines of context around each match (like `grep -C`)
- **Command palette** (`⌘K` / `Ctrl+K`) — keyboard-driven access to all actions
- **History** — recent searches with pattern, directory, and match counts
- **Pattern & directory suggestions** — dropdown autocomplete from past searches
- **Multi-root search** — search across multiple directories in one pass
- **File type presets** — filter by language (Rust, Python, JS/TS, Java, …) without typing globs. Support for adding, editing, enabling/disabling, and reordering presets (via drag & drop or action buttons)
- **Flexible Export Formatting** — copy search results to clipboard using custom templates with placeholders (e.g. `%f` for path, `%l` for line number, `%c` for content) and flat or grouped layout options
- **Word wrap** — toggle line wrapping in the result panel to fit the display area
- **Themes** — Dark (Catppuccin Mocha), Light (Catppuccin Latte), High Contrast

## Download

Pre-built installers are available on the [Releases](https://github.com/orapli/aero-grep/releases) page:

| Platform | File |
|----------|------|
| macOS (Universal) | `aero-grep-*-macos.dmg` |
| Windows 64-bit | `aero-grep-*.msi` |

> **macOS note:** The app is not code-signed. On first launch, right-click the app and choose **Open** to bypass Gatekeeper.

## Requirements

- macOS 12+ or Windows 10/11 (64-bit)
- To build from source: Rust toolchain (`cargo`)

## Build from source

```bash
cargo build --release
```

```
# macOS / Linux
./target/release/aero-grep

# Windows
.\target\release\aero-grep.exe
```

> **macOS only:** The debug build crashes due to a winit/icrate ABI issue. Always run the release binary on macOS.

## Usage

1. Enter a **directory** to search in
2. Enter a **pattern** (literal or regex)
3. Press **Search** (`Enter`)
4. Click a file in the left panel to view its matches
5. Click a line number or double-click a file to open it in your editor

### Keyboard shortcuts

| Key | Action |
|-----|--------|
| `Ctrl+F` / `⌘F` | Focus pattern input |
| `F3` / `Ctrl+G` | Next match |
| `Shift+F3` / `Ctrl+Shift+G` | Previous match |
| `↑` / `↓` | Move between files in the list |
| `Enter` | Open current file in editor |
| `⌘K` / `Ctrl+K` | Command palette |
| `Esc` | Close palette / dialogs |

## Configuration

Settings are stored in:
- macOS: `~/Library/Application Support/aero-grep/config.json`
- Windows: `%APPDATA%\aero-grep\config.json`
- Linux: `~/.config/aero-grep/config.json`

Configure your editor command, theme, font size, default excluded directories, and more from the Settings panel (⚙ icon, top-right).
