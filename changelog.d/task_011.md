### Search and replacement safety

- Preserve the parameters used by an in-flight search when publishing results.
- Keep results attached to their tab and block tab mutations during a scan.
- Prevent stale search results from driving replacement, and freeze replacement
  confirmation details before writing files.
- Improve modal keyboard isolation, empty-result recovery, and editor line
  selection when context lines are shown.
