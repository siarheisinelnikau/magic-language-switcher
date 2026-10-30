# Changelog

## 0.1.1 — 2026-10-30

- Editable shortcut buttons in the existing settings rows; window dimensions are unchanged.
- Persistent custom bindings, with Esc to cancel and Backspace to restore the default.
- Duplicate and global conflict detection, plus rollback if registration or saving fails.
- Recording is confined to the selected shortcut field and stops on focus loss.
- Dynamic undo hints and automatic migration of older layout-only settings.
- Desktop regression coverage for capture, conflicts, cancellation, resets, and persistence.
- Version-based release automation for future Cargo version bumps on `main`.

## 0.1.0 — 2026-10-30

First public release.

- Native Rust Windows app, distributed as a single portable x64 EXE.
- Last-word correction with F8, selection correction with Shift+F8, and verified undo with Ctrl+F8.
- System-provided keyboard mappings for installed layouts, including Shift and AltGr.
- Automatic source selection, next-layout cycling, and explicit From/To choices.
- Compact English settings UI and a language indicator in the system tray.
- Pause/resume, optional per-user startup, and a native About dialog with an expandable license.
- Clipboard format preservation, selection limits, and native password-field exclusion.
- Automated CI, portable EXE inspection, release packaging, and SHA-256 checksums.
