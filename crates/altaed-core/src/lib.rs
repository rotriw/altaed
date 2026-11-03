//! Document identity, persistence and execution without a window or editor dependency.
//!
//! A document ID owns mutable local data; filenames and problem URLs never
//! replace that identity. Execution consumes content/configuration snapshots,
//! so a later edit cannot change an in-flight build or an existing terminal.
pub mod atomic_file;
pub mod checker;
pub mod companion;
pub mod document;
pub mod output_diff;
pub mod platform;
pub mod problem;
pub mod runner;
pub mod sample_files;
pub mod session;
pub mod storage;
pub mod toolchain;
