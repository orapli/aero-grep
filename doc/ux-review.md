# UX and design review

This review records the high priority usability findings addressed in task 011
and the evidence based follow up work that remains.

## Addressed in this task

1. Search results now retain the exact parameters sent to the worker. Editing
   the toolbar while a scan is running cannot change the result provenance.
2. Tab navigation is locked during a scan, and completion retains the owning
   tab. Replacement refuses to use a result after its search criteria change;
   replacement text and scope remain deliberate operation choices.
3. Preview and confirmation capture the operation parameters and target files.
   Confirmation executes that snapshot, even if a later event edits the
   toolbar. Replacement is unavailable while a scan is pending.
4. Preview, confirmation, shortcuts, history, and settings overlays now block
   background controls and global navigation consistently. Escape dismisses the
   highest priority overlay first, then cancels an active scan.
5. Empty results, empty file filters, empty content filters, and empty
   selections have distinct messages and recovery actions. The content panel
   offers pattern and directory focus actions after a zero hit search.
6. Opening a selected match in an editor uses the matching line index when
   context lines are interleaved. The previous raw vector index could open a
   context line instead of the selected match.

## Remaining priorities

- Font sizing is not yet applied uniformly: `show_content_panel` contains
  fixed `FontId::monospace(12.5)` and 20 px row-height paths while the settings
  panel exposes a configurable font size. This needs a layout pass and should
  be tested at the smallest and largest supported sizes.
- Toolbar Tab navigation covers search and filter inputs but does not provide
  a documented focus trap for modal dialogs. Modal focus order and return focus
  should be verified with keyboard-only interaction.
- `GrepApp` remains a large stateful UI type (roughly 9,500 lines in
  `src/app.rs`) that combines search, replacement, history, tabs, settings,
  and rendering. Extracting testable controllers should be planned as several
  small tasks to avoid destabilizing behavior.
- Editor process launch errors are currently not surfaced to the user. A
  future change should report the command and preserve the selected result.
- Replacement still performs synchronous file reads and writes in the UI
  update path. Large replacement sessions can block repainting; an asynchronous
  cancellable operation should be designed separately.

The review does not propose a palette or broad font refactor. The new empty and
recovery states use `pal.subtext` for readable guidance. The Light Search
button's `pal.bg_mantle` text on `pal.accent` measures 6.02:1 and does not need
a color change. The Light Replace button measures 4.46:1 and remains a
candidate for an on-accent role in a dedicated palette task. Existing muted
counts and paths remain an identified contrast follow-up (Dark 2.46:1; Light
3.63:1), rather than being changed incidentally in this task.
