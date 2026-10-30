# Contributing

Keep code, UI text, documentation, and commit messages in English. Prefer small changes with a concrete before/after example.

## Development

Use stable Rust and the Windows MSVC toolchain. Install the Windows SDK and Visual Studio C++ Build Tools for the native resources and linker.

```powershell
cargo fmt --all -- --check
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked --all-features
```

Tests of conversion rules should exercise behavior across layouts, not duplicate their implementation. Keep unsafe Win32 calls inside the Windows modules and document pointer validity and handle ownership. Never log selected text or add a continuous typing hook to solve a correction problem. Shortcut recording may use a temporary hook only while a recording field is active; remove it on save, cancel, focus loss, and exit.

For native input changes, also run the opt-in desktop tests described in the README. They must not change startup registration or write to the real user preferences; desktop tests use a private temporary settings file. Visual-check the compact settings window and About dialog at the display scaling you use.

## Report a bug

Include the Windows version, editor, source and destination keyboard layout names, chosen From/To settings, shortcut, and a small non-sensitive example with expected output. Mention whether the editor runs as administrator and whether the clipboard contains text, files, or an image.

## Publish a release

1. Update `package.version` in `Cargo.toml` and regenerate `Cargo.lock` with `cargo check`. Windows resource versions derive from the package version.
2. Compare the target with the preceding published stable release. Add readable English notes in `docs/releases/vX.Y.Z.md` describing verified user-visible changes, update `CHANGELOG.md`, and refresh README download links and accurate application/settings screenshots. Group notes under `Added`, `Improved`, and `Fixed` when useful; a compare link supplements the description.
3. Run the checks above, the Windows desktop test, and `tools/package-release.ps1 -Version X.Y.Z`. Check the generated EXE on Windows.
4. Publish the version bump to `main` through the connected GitHub plugin. **Publish version** creates the `vX.Y.Z` tag after validation and publishes the assets. A version tag for a commit whose package version and release notes already match can also trigger the release workflow.
5. Wait for **Publish version** and its reusable **Release** job. It publishes only after checks, portable EXE validation, and asset uploads succeed. Confirm that the release contains the EXE and `SHA256SUMS.txt`.

### Executable naming

For the current Windows x64-only distribution, publish **`MagicLanguageSwitcher-vX.Y.Z.exe`**. Include the exact same name in the upload step, README links, release notes, updater lookup, and `SHA256SUMS.txt`.

Use architecture suffixes only when the **same release** offers multiple executable architectures, for example `MagicLanguageSwitcher-vX.Y.Z-x64.exe` and `MagicLanguageSwitcher-vX.Y.Z-arm64.exe`. Source archives, checksums, symbols, and documentation do not count as architecture variants. State Windows x64 support in the release description even when the EXE filename has no suffix.

Published v0.1.0–v0.1.2 retain their historical `-windows-x64.exe` names. The v0.1.2 updater only recognizes that historical name. The first release using the new convention therefore needs a clear one-time manual download instruction for v0.1.2 users; the updated release reader supports both conventions. Verify the exact filename in checksum lookup, including rejection of incompatible architectures.

### Distribution checks

- Use the connected GitHub plugin for remote operations and source uploads. Local Git inspection, staging, and commits are fine. If a required plugin capability is absent, explain the gap and request only the missing decision or capability. Do not start a credential or login flow as a fallback. The existing authorized release workflow can build and upload assets.
- Keep **Check for updates** discoverable in About, show the installed version, and check only this repository's compatible stable releases over normal HTTPS without requiring login.
- For future application releases, provide a nonblocking startup check enabled by default, a persistent **Check for updates on startup** opt-out that prevents startup requests, and quiet handling of no-update results and transient startup errors. Keep manual checks available.
- Provide user-initiated **Download and install**, visible progress and useful errors, checksum verification, settings/data preservation, and restart when needed. Never replace a working EXE with an incomplete download.
- Confirm release notes describe the actual changes since the preceding stable release, and that download links, executable names, checksums, platform details, and real README screenshots agree.

Do not move an existing release tag to a different commit. Fix a published release in a new version.
