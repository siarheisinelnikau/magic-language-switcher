//! Physical-key layout conversion and the native Windows desktop application.
pub mod layout;
pub mod shortcuts;
pub mod updates;

#[cfg(any(windows, test))]
mod raster;

#[cfg(windows)]
pub mod platform;
