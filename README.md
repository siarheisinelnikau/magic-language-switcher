# Magic Language Switcher

[![CI](https://github.com/siarheisinelnikau/magic-language-switcher/actions/workflows/ci.yml/badge.svg)](https://github.com/siarheisinelnikau/magic-language-switcher/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/siarheisinelnikau/magic-language-switcher)](https://github.com/siarheisinelnikau/magic-language-switcher/releases/latest)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

A lightweight **Windows app** that fixes text typed in the wrong keyboard layout with one hotkey. Written in Rust, with a compact native interface and a **single portable EXE**.

`ghbdtn vbh` → `привет мир` is one example. Conversion uses the keyboard layouts installed in Windows, rather than a hardcoded English/Russian pair.

**[Download the latest release](https://github.com/siarheisinelnikau/magic-language-switcher/releases/latest)** · [Report an issue](https://github.com/siarheisinelnikau/magic-language-switcher/issues)

![Compact settings with layout selection and correction shortcuts](assets/settings.png)

## Get started

1. Download `MagicLanguageSwitcher-v0.1.0-windows-x64.exe` from [Releases](https://github.com/siarheisinelnikau/magic-language-switcher/releases/latest).
2. Put it in a folder you want to keep and run it. No installer, .NET, browser engine, or VC++ redistributable is needed.
3. Leave the caret after a mistyped word and press **F8**, or select some text and press **Shift+F8**.

Requires **Windows 10 version 1607 or newer, or Windows 11, x64**. This first release is unsigned.

## Shortcuts

| Shortcut | Action |
| --- | --- |
| **F8** | Convert the last word, using the editor's word-selection behavior |
| **Shift+F8** | Convert the selected text |
| **Ctrl+F8** | Undo the last correction immediately, in the same editor |

Undo checks that the text before the caret still matches the last correction. If the text or caret has changed, it refuses to replace anything. A selection can contain up to **4,096 UTF-16 code units**, including line breaks and tabs.

The system tray icon shows the active language. It stays in the tray; nothing follows your mouse or text caret. Click the icon to open settings, or right-click for pause/resume and quit. Closing the settings window keeps the app running in the tray.

## Choose any installed keyboard layout

The **From** and **To** lists come from Windows and distinguish layout variants, such as US and Dvorak. Direct character mappings include Shift and AltGr states.

- **From → Auto (active layout)** detects the source by character coverage. If several layouts can produce the same letters, the active layout wins the tie.
- **To → Next installed layout** cycles from the detected source to the next layout in Windows' list. With two installed layouts, it switches between them.
- Choose explicit **From** and **To** entries for a fixed pair. This is useful for layouts sharing an alphabet, such as US QWERTY, German QWERTZ, and French AZERTY.

Add additional keyboards in Windows language settings, then reopen the app's settings window to refresh the lists. The app does not install languages or change the Windows language list. After a correction, it requests the destination layout for the active editor.

This is physical-key conversion, not translation or transliteration. **IME candidate composition** and **multi-key dead-key composition** cannot be reconstructed from committed text. Characters without a direct mapping remain unchanged. For ambiguous text, choose the source explicitly.

## A few settings

- **Pause** temporarily disables correction shortcuts; the language icon remains available.
- **Start with Windows** adds this EXE to your own user's startup entries. Keep the EXE in its final folder before enabling it; disable the option before moving or deleting it.
- Layout choices are saved in `%LOCALAPPDATA%\MagicLanguageSwitcher\settings.conf`.
- Run with `--background` to start directly in the tray.
- **About** contains the version, project link, and an expandable MIT license.

## Privacy and compatibility

The app works offline. It has no telemetry, network service, continuous keyboard hook, or typing log. It reads text only when you invoke a correction and keeps one undo record in memory.

Correction temporarily uses the clipboard to read selected text; multiline text is pasted back as plain text. Existing clipboard formats are duplicated and restored, and a newer clipboard value is not overwritten. Windows clipboard history or clipboard managers may retain the temporary text. Unusual app-owned clipboard formats can prevent a correction if they cannot be preserved.

Use an editable field that supports normal selection and copy/paste. Native password fields are excluded. Read-only controls, games, terminals, custom editors, and apps running as administrator may reject or reinterpret input. Text formatting is not preserved when replacing a rich-text selection. If a global shortcut is already registered by another app, settings and a tray notification report the conflict.

## Build from source

On Windows, install the [Rust MSVC toolchain](https://www.rust-lang.org/tools/install), Visual Studio C++ Build Tools, and the Windows SDK. Rust **1.88 or newer** is required; current stable Rust is recommended.

```powershell
git clone https://github.com/siarheisinelnikau/magic-language-switcher.git
cd magic-language-switcher
cargo build --release --locked
```

The executable is `target\release\magic-language-switcher.exe`. The MSVC runtime is linked statically through `.cargo/config.toml`; the icon, manifest, version information, and license are embedded. Only Windows system DLLs are required.

Create the same release assets used on GitHub:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File tools/package-release.ps1
```

This builds a Windows x64 EXE, checks its PE imports, and writes the EXE and `SHA256SUMS.txt` to `artifacts\v0.1.0\`. To compare a downloaded file's checksum:

```powershell
Get-FileHash .\MagicLanguageSwitcher-v0.1.0-windows-x64.exe -Algorithm SHA256
```

## Tests

```powershell
cargo fmt --all -- --check
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked --all-features
```

Pure conversion tests also run on Linux. The opt-in Windows desktop test starts an isolated native editor and the real app, exercises the actual global shortcuts, and restores the clipboard afterward. It requires an unlocked desktop, installed US English and Russian layouts for its known examples, and no running app instance:

```powershell
cargo test --locked --features desktop-tests --test desktop -- --ignored --test-threads=1 --nocapture
```

## Project structure

| Path | Purpose |
| --- | --- |
| `src/layout.rs` | Layout-independent physical-key conversion and source selection |
| `src/platform/keyboard.rs` | Installed layout discovery and Windows keyboard tables |
| `src/platform/correction.rs` | Correction and verified undo |
| `src/platform/clipboard.rs` | Clipboard snapshots and restoration |
| `src/platform/ui.rs` | Native settings window, hotkeys, and tray language icon |
| `src/platform/about.rs` | Native About dialog and embedded license |
| `src/platform/smoke.rs` | Opt-in desktop integration checks |
| `tools/` | Portable EXE validation and release packaging |

The Windows mappings use [`MapVirtualKeyExW`](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-mapvirtualkeyexw) and [`ToUnicodeEx`](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-tounicodeex), with the non-mutating keyboard-state flag.

## Releases and contributions

CI checks formatting, Clippy, and tests on Windows and Linux. Pushing a version tag such as `v0.1.0` runs the Windows release workflow: validate, build, inspect DLL imports, create checksums, upload assets to a draft, then publish it. See [CONTRIBUTING.md](CONTRIBUTING.md) for the release procedure.

The first `v0.1.0` release is bootstrapped by the **Publish first release** workflow when that workflow file is initially pushed to `main`. Subsequent releases use version tags.

Inspired by the one-key layout correction workflow of [Mahou](https://github.com/Sippelin/Mahou3). This is an independent implementation.

Licensed under [MIT](LICENSE).
