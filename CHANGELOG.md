# Changelog

## 0.2.0 — 2026-11-01

### Added
- English, Russian, French, German, Spanish, and Italian interface localization across settings, correction guidance, shortcut editing, tray actions, About, and update controls.
- Saved language and light/dark theme dropdowns in the settings window, applied immediately without changing keyboard conversion layouts or shortcut preferences.
- A finite, isolated native settings capture mode for checking localized layouts and both themes.

### Improved
- Theme-aware retained CPU backgrounds, native edit/list colors, accessible button labels, keyboard focus, and owner-drawn button states.
- Migration of existing settings keeps conversion layouts, shortcuts, and the startup update opt-out.
- Catalog checks cover complete translations, unique messages, and matching dynamic placeholders.

## 0.1.3 — 2026-10-31

- Nonblocking update checks on startup, enabled by default with a persistent About opt-out; no-update results and startup errors stay quiet.
- User-initiated Download and install shows byte/percentage and verification/restart progress in the existing window.

- CPU-rasterized settings background retained in a DIB, with invalid-region GDI presentation and no continuous redraw loop.
- Unchanged frames and control text are reused; hidden/minimized windows skip presentation, and DPI changes batch font/bounds updates before repainting.
- Reopening unchanged settings avoids rebuilding keyboard tables and combo-box contents.
- Shortcut recording saves as soon as the keys are released, without waiting for the polling timer.
- Opt-in release-profile diagnostics measure UI callbacks, frame reuse, and idle repaint counts without global input or clipboard changes.
- Standalone EXE assets use `MagicLanguageSwitcher-vVERSION.exe`; architecture suffixes are reserved for releases with multiple executable architecture variants.
- Release packaging, uploads, and generated SHA-256 entries use the same filename.
- The updated release reader accepts standalone, explicit x64, and historical asset names while rejecting x86 and ARM64 variants.
- README links use the new v0.1.3 filename and explain the one-time manual upgrade required for v0.1.2.

## 0.1.2 — 2026-10-30

- Manual GitHub update checks from About and the tray menu, with in-app download and restart.
- HTTPS downloads validated against release metadata and SHA256SUMS, with a temporary helper, EXE backup, and cleanup.
- Isolated integration checks verify actual EXE replacement, restart, cancellation, and temporary-file cleanup.

- One correction shortcut: selected text takes priority; otherwise the last word is corrected.
- Separate, customizable undo shortcut remains available.
- Two shortcut fields and a smaller 464 × 487 logical-pixel settings window.
- Existing last-word and undo bindings migrate automatically; the selection binding is retired.
- Native selection and UI Automation probes avoid treating copy-current-line behavior as a selection.
- A failed or unchanged selection never falls back to another range.
- Clipboard snapshots use the correct GDI APIs for bitmaps and enhanced metafiles, avoiding an invalid memory-size query on image handles.
- Desktop regression checks cover partial/backward selections, empty input, copy failures, clipboard contention, settings migration, and custom shortcuts.

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
