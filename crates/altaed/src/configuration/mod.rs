//! Application configuration boundary: data model, declarative settings and
//! command catalog, plus versioned file I/O. GPUI bindings live in keymap.rs.
mod color;
pub(crate) mod commands;
pub use color::RgbColor;
mod compiler;
mod preferences;
pub(crate) mod schema;
mod storage;

pub(crate) use preferences::WorkbenchPreferences;
pub use preferences::{AppPreferences, EditorPreferences, ToolchainPreferences};
pub use storage::{FILE_NAME, load, load_or_create, save};
