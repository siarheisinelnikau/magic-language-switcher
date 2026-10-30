mod about;
mod clipboard;
mod correction;
mod keyboard;
mod native;
mod settings;
mod ui;

#[cfg(feature = "desktop-tests")]
pub mod smoke;

pub use ui::{run, show_error};
