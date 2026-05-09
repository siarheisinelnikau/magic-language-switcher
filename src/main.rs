#![cfg_attr(windows, windows_subsystem = "windows")]

fn main() {
    #[cfg(windows)]
    if let Err(error) = magic_language_switcher::platform::run() {
        magic_language_switcher::platform::show_error(&error);
    }
    #[cfg(not(windows))]
    eprintln!("Magic Language Switcher is a Windows desktop application.");
}
