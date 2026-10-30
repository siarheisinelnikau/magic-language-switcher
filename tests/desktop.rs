#[cfg(all(windows, feature = "desktop-tests"))]
#[test]
#[ignore = "requires an unlocked Windows desktop and no running Magic Language Switcher"]
fn native_desktop_workflow() {
    magic_language_switcher::platform::smoke::run(std::path::Path::new(env!(
        "CARGO_BIN_EXE_magic-language-switcher"
    )))
    .expect("Windows desktop smoke checks");
}
