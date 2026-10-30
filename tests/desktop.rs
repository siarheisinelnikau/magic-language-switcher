#[cfg(all(windows, feature = "desktop-tests"))]
#[test]
#[ignore = "requires an unlocked Windows desktop and no running Magic Language Switcher"]
fn native_desktop_workflow() {
    magic_language_switcher::platform::smoke::run(std::path::Path::new(env!(
        "CARGO_BIN_EXE_magic-language-switcher"
    )))
    .expect("Windows desktop smoke checks");
}

#[cfg(all(windows, feature = "desktop-tests"))]
#[test]
fn update_replaces_exe_and_restarts_without_installation() {
    magic_language_switcher::platform::update_smoke(std::path::Path::new(env!(
        "CARGO_BIN_EXE_magic-language-switcher"
    )))
    .expect("isolated updater handoff, replacement, restart, and cleanup");
}

#[cfg(all(windows, feature = "desktop-tests"))]
#[test]
#[ignore = "profiles a real native window; requires no running app instance"]
fn native_ui_responsiveness_profile() {
    let output = std::env::var_os("MAGIC_LANGUAGE_SWITCHER_PROFILE_OUTPUT")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("MagicLanguageSwitcher-ui-profile.json"));
    magic_language_switcher::platform::ui_probe::run(
        std::path::Path::new(env!("CARGO_BIN_EXE_magic-language-switcher")),
        &output,
    )
    .expect("native UI timings");
}

#[cfg(all(windows, feature = "desktop-tests"))]
#[test]
#[ignore = "starts a real app instance with isolated settings and a local release fixture"]
fn startup_update_opt_out_and_quiet_results() {
    magic_language_switcher::platform::ui_probe::startup_checks(std::path::Path::new(env!(
        "CARGO_BIN_EXE_magic-language-switcher"
    )))
    .expect("startup update behavior");
}
