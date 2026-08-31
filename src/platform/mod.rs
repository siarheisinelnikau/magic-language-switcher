mod about;
mod clipboard;
mod correction;
mod hotkeys;
mod http;
mod keyboard;
mod native;
mod selection;
mod settings;
mod surface;
mod ui;
mod updater;

#[cfg(feature = "desktop-tests")]
mod diagnostics;
#[cfg(feature = "desktop-tests")]
pub mod ui_probe;

#[cfg(feature = "desktop-tests")]
pub use updater::smoke as update_smoke;

#[cfg(feature = "desktop-tests")]
pub mod smoke;

pub use ui::{run, show_error};
