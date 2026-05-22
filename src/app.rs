mod app_state;
pub mod api_client;
mod autocomplete;
pub mod events;
pub mod file_icons;
pub mod file_tree;
pub mod git_diff;
pub mod git_panel;
pub mod keyboard;
pub mod lsp_actions;
pub mod mouse;
mod python_completion;
pub mod terminal;
pub mod ui_handlers;
use crate::editor::Editor;
use crate::highlighter::{CompletionItem, SymbolKind, TREE_SITTER_HIGHLIGHT_MAX_BYTES};
use crate::renderer::Renderer;
use app_state::fuzzy_match;
pub use app_state::*;
use glutin::display::GetGlDisplay;
use python_completion::*;
use rustc_hash::{FxHashMap, FxHashSet};
use std::path::{Path, PathBuf};
use winit::event_loop::ActiveEventLoop;
use winit::platform::wayland::WindowAttributesExtWayland;
use winit::window::Window;

const FILE_OPEN_HIGHLIGHT_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(150);
const FILE_OPEN_LARGE_PRIORITY_HIGHLIGHT_TIMEOUT: std::time::Duration =
    std::time::Duration::from_millis(1200);
const FILE_OPEN_BLOCKING_HIGHLIGHT_MAX_BYTES: usize = TREE_SITTER_HIGHLIGHT_MAX_BYTES;

fn apply_initial_import_folds(editor: &mut Editor, ext: &str, text: &str) {
    let mut add_fold = |start_b: usize, end_b: usize| {
        if editor
            .foldable_ranges_bytes
            .iter()
            .any(|&(start, end, _)| start == start_b && end == end_b)
        {
            return;
        }
        editor.foldable_ranges_bytes.push((start_b, end_b, false));
        let sl = editor
            .line_offsets
            .partition_point(|&x| x <= start_b)
            .saturating_sub(1);
        let el = editor
            .line_offsets
            .partition_point(|&x| x <= end_b)
            .saturating_sub(1);
        if el > sl {
            editor.foldable_lines.insert(sl, el);
            editor.folded_lines.insert(sl);
            editor.folded_start_bytes.insert(editor.line_offsets[sl]);
        }
    };

    match ext {
        "py" | "pyi" => {
            for block in crate::languages::python::import_blocks(text) {
                add_fold(block.start, block.end);
            }
            for (start, end) in initial_python_bracket_folds(text) {
                add_fold(start, end);
            }
        }
        "rs" => {
            for block in crate::languages::rust::import_blocks(text) {
                add_fold(block.start, block.end);
            }
        }
        "dart" => {
            for block in crate::languages::dart::import_blocks(text) {
                add_fold(block.start, block.end);
            }
        }
        _ => {}
    }
}

pub(crate) fn tab_display_titles_for(
    tabs: &[EditorTab],
    active_tab: usize,
    active_path: Option<&PathBuf>,
    active_title: &str,
) -> Vec<String> {
    let mut paths: Vec<Option<&PathBuf>> = tabs.iter().map(|t| t.file_path.as_ref()).collect();
    if active_tab < paths.len() {
        paths[active_tab] = active_path;
    }

    let mut display_titles = vec![String::new(); tabs.len()];
    for i in 0..tabs.len() {
        if let Some(p1) = paths[i] {
            let mut diff_level = 0;
            let mut collision = false;
            for j in 0..tabs.len() {
                if i == j {
                    continue;
                }
                if let Some(p2) = paths[j]
                    && p1.file_name() == p2.file_name()
                {
                    collision = true;
                    let mut it1 = p1.components().rev();
                    let mut it2 = p2.components().rev();
                    let mut level = 0;
                    loop {
                        let c1 = it1.next();
                        let c2 = it2.next();
                        if c1 != c2 {
                            diff_level = diff_level.max(level);
                            break;
                        }
                        if c1.is_none() && c2.is_none() {
                            break;
                        }
                        level += 1;
                    }
                }
            }
            if collision && diff_level > 0 {
                let comps: Vec<_> = p1.components().rev().collect();
                if diff_level < comps.len() {
                    let diff_dir = comps[diff_level].as_os_str().to_string_lossy();
                    let file_name = comps[0].as_os_str().to_string_lossy();
                    display_titles[i] = if diff_level == 1 {
                        format!("{diff_dir}/{file_name}")
                    } else {
                        format!("{diff_dir}/.../{file_name}")
                    };
                } else {
                    display_titles[i] = p1.to_string_lossy().into_owned();
                }
            } else {
                display_titles[i] = p1
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned();
            }
        } else {
            let title = if i == active_tab {
                active_title
            } else {
                &tabs[i].base_title
            };
            display_titles[i] = if title.is_empty() {
                "Безымянный".to_string()
            } else {
                title.to_string()
            };
        }
    }
    display_titles
}

include!("app/app_ide_tab_methods.rs");
include!("app/app_file_tab_methods.rs");
include!("app/app_window_external_methods.rs");

#[cfg(test)]
mod app_behavior_tests;
