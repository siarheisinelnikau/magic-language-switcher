# Contributing

Keep code, UI text, documentation, and commit messages in English. Prefer small changes with a concrete before/after example.

## Development

Use stable Rust and the Windows MSVC toolchain. Install the Windows SDK and Visual Studio C++ Build Tools for the native resources and linker.

```powershell
cargo fmt --all -- --check
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked --all-features
```

Tests of conversion rules should exercise behavior across layouts, not duplicate their implementation. Keep unsafe Win32 calls inside the Windows modules and document pointer validity and handle ownership. Never log selected text or add a continuous typing hook to solve a correction problem.

For native input changes, also run the opt-in desktop test described in the README. It should not change startup registration or persist test layout choices. Visual-check the compact settings window and About dialog at the display scaling you use.

## Report a bug

Include the Windows version, editor, source and destination keyboard layout names, chosen From/To settings, shortcut, and a small non-sensitive example with expected output. Mention whether the editor runs as administrator and whether the clipboard contains text, files, or an image.

## Publish a release

1. Update `package.version` in `Cargo.toml` and regenerate `Cargo.lock` with `cargo check`. Windows resource versions derive from the package version.
2. Add `docs/releases/vX.Y.Z.md`, update `CHANGELOG.md`, and update version-specific examples in the README.
3. Run the checks above, the Windows desktop test, and `tools/package-release.ps1 -Version X.Y.Z`. Check the generated EXE on Windows.
4. Commit and push the changes to `main`, then create and push an annotated `vX.Y.Z` tag.
5. Wait for the **Release** workflow. It publishes only after checks, portable EXE validation, and asset uploads succeed. Confirm that the release contains the EXE and `SHA256SUMS.txt`.

Do not move an existing release tag to a different commit. Fix a published release in a new version.
