# UI performance checks

The application uses native Win32 controls and a CPU-rasterized settings background. `src/raster.rs` retains pixels at the physical window size, and `src/platform/surface.rs` retains a CPU-addressable DIB. GDI presents only the invalid rectangle. No OpenGL, Direct3D, or Vulkan context is created by the application.

The frame key includes dimensions, scale, and scene colors/geometry. Exposing an unchanged window copies the retained bitmap; it does not repeat the rasterization or upload. Hidden/minimized windows skip presentation. Size or DPI changes regenerate the frame when Windows next requests paint. Native controls continue to handle their normal input, caret, focus, and theme drawing.

## Automated checks

```powershell
cargo fmt --all -- --check
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked --all-features
```

The raster tests cover unchanged-frame reuse, dimension/DPI/theme invalidation, color order, rounded corners, invalid surface sizes, and antialiasing at 100%, 125%, 150%, and 200%. A Windows memory-surface test verifies top-down row order, RGB colors, and preservation of pixels outside the invalid rectangle. These checks do not replace a visual check on monitors with those DPI settings.

## Release-build UI profile

Close any running instance first. Run this on an unlocked Windows desktop:

```powershell
$env:MAGIC_LANGUAGE_SWITCHER_PROFILE_OUTPUT = "$PWD\artifacts\ui-profile\current.json"
cargo test --release --locked --features desktop-tests --test desktop native_ui_responsiveness_profile -- --ignored --test-threads=1 --nocapture
```

The probe starts the real app with private temporary preferences. It sends 20 Open commands, toggles Pause 20 times, requests 20 full-window repaints through Windows, checks one second of hidden and visible idle, and quits through the hidden app's normal command path. It does not inject global keyboard/mouse input, change the clipboard, or change Windows settings. It avoids foreground activation so it does not pull focus from other apps.

The JSON separates command round-trip timings from UI-thread callback timings. It includes counts for `paint`, `cpu_raster`, `cpu_upload`, and keyboard table preparation. With an unchanged window, the forced paints should reuse one rasterization and upload. Idle paints should be examined separately from deliberate invalidations or real OS expose events.

The diagnostic scopes and report message are compiled only with `desktop-tests`; production builds contain no profiling timers or report output. A JSON report contains timing data, not typed text.

## Local comparison

In a Windows x64 release-build comparison on the same machine, the previous synchronous Open path rebuilt tables and lists on each request: 21 keyboard-table preparations and a median Open command round trip of 16.93 ms. The retained-frame implementation prepared the tables once and produced a median repeated Open round trip of 0.066 ms across the same 20-command scenario.

The CPU background was rasterized once for 21 paint callbacks. Both implementations produced zero additional background paint callbacks during the one-second visible and hidden idle samples. The new CPU/DIB paint callback measured about 0.41 ms at the median, versus 0.23 ms for the previous direct GDI background drawing; the renderer change is not evidence of a universal painting-speed improvement.

These samples measure application callbacks and explicit repaint requests. They do not measure physical mouse-to-photon latency, all native control paints, desktop composition, or driver/presentation stalls. Verify the packaged executable with actual button clicks, typing, hide/restore, and monitor DPI changes. A perceptual lag report remains unconfirmed until that interaction is checked with the user.
