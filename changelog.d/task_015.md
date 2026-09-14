- Replace now captures exact file bytes, refuses external edits, validates again
  immediately before writing, and commits through a same-directory atomic
  replacement while preserving file permissions. Backups are verified before
  replacement, and manifest persistence failures remain visible with the
  recoverable backup location.
