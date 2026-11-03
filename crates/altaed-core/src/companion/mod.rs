//! Competitive Companion protocol and durable reception, independent of GPUI.
mod inbox;
mod protocol;
mod server;
pub use inbox::PendingImport;
pub use inbox::{acknowledge, pending_documents};
pub use protocol::parse_problem;
pub use server::{CompanionServer, CompanionSettings};

/// Durably receive an import before acknowledging it to an external sender.
pub fn receive_problem(
    root: &std::path::Path,
    bytes: &[u8],
    settings: crate::document::DocumentSettings,
) -> anyhow::Result<crate::document::DocumentId> {
    let problem = parse_problem(bytes)?;
    store_problem(root, problem, settings)
}
pub(super) fn store_problem(
    root: &std::path::Path,
    problem: crate::problem::Problem,
    settings: crate::document::DocumentSettings,
) -> anyhow::Result<crate::document::DocumentId> {
    let document = crate::document::CodeDocument::from_problem(problem, settings);
    let id = document.id;
    inbox::store(root, document)?;
    Ok(id)
}
