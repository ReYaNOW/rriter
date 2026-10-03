mod dart_workspace;
mod rooted_language;
mod lsp_manager_rust;
mod rust_workspace;
pub use lsp_manager_rust::RustRowInfo;
pub use rust_workspace::initialization_options as rust_initialization_options;
pub(crate) use rust_workspace::{
    RUST_ANALYZER_ARCHIVES, RUST_ANALYZER_RELEASE_TAG, RustAnalyzerArchive,
    rust_analyzer_archive_for_platform,
};
mod ruff_workspace;

include!("lsp/lsp_process.rs");
include!("lsp/lsp_manager.rs");
include!("lsp/lsp_diagnostics_store.rs");
include!("lsp/lsp_manager_support.rs");
