//! Application configuration boundary: data model, declarative settings and
//! command catalog, plus versioned file I/O. GPUI bindings live in keymap.rs.
pub(crate) mod commands;
mod preferences;
pub(crate) mod schema;
mod storage;

pub(crate) use preferences::WorkbenchPreferences;
pub use preferences::{AppPreferences, EditorPreferences, ToolchainPreferences};
pub use storage::{FILE_NAME, load, load_or_create, save};
