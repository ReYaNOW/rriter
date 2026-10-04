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
include!("lsp/lsp_process_notifications.rs");
include!("lsp/lsp_manager.rs");
include!("lsp/lsp_diagnostics_store.rs");
include!("lsp/lsp_manager_support.rs");

pub(crate) fn server_names_for_extension(extension: &str) -> &'static [&'static str] {
    match extension {
        "py" | "pyi" => &[RUFF_SERVER.program, TY_SERVER.program],
        "rs" => &[RUST_ANALYZER_SERVER.program],
        "dart" => &[DART_SERVER.program],
        _ => &[],
    }
}

pub(crate) fn has_server_for_extension(extension: &str) -> bool {
    !server_names_for_extension(extension).is_empty()
}

#[cfg(test)]
mod restart_server_keymap_tests {
    use super::*;

    #[test]
    fn active_extension_maps_to_every_matching_server() {
        for (extension, expected) in [
            ("py", vec![RUFF_SERVER.program, TY_SERVER.program]),
            ("pyi", vec![RUFF_SERVER.program, TY_SERVER.program]),
            ("rs", vec![RUST_ANALYZER_SERVER.program]),
            ("dart", vec![DART_SERVER.program]),
            ("txt", vec![]),
        ] {
            let actual = server_names_for_extension(extension);
            assert_eq!(actual, expected, "extension {extension} maps to {actual:?}, expected {expected:?}");
        }
    }
}

#[cfg(test)]
mod rooted_language_tests;
