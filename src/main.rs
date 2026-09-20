#![cfg_attr(windows, windows_subsystem = "windows")]

fn main() {
    #[cfg(windows)]
    if let Err(error) = magic_language_switcher::platform::run() {
        // Internal helpers must fail through their exit status without a dialog;
        // otherwise a failed startup can block the update helper's rollback.
        if std::env::args().any(|argument| {
            argument == "--apply-update"
                || argument == "--test-updater"
                || argument.starts_with("--updater-smoke-")
        }) {
            eprintln!("Update failed: {error}");
            std::process::exit(1);
        }
        magic_language_switcher::platform::show_error(&error);
    }
    #[cfg(not(windows))]
    eprintln!("Magic Language Switcher is a Windows desktop application.");
}
