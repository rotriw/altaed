//! Document identity, persistence and execution without a window or editor dependency.
//!
//! A document ID owns mutable local data; filenames and problem URLs never
//! replace that identity. Execution consumes content/configuration snapshots,
//! so a later edit cannot change an in-flight build or an existing terminal.
pub mod atomic_file;
pub mod checker;
pub mod document;
pub mod platform;
pub mod runner;
pub mod session;
pub mod storage;
pub mod toolchain;
