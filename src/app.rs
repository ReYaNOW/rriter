pub mod api_client;
pub mod api_mock;
mod app_state;
mod autocomplete;
pub mod events;
pub mod file_icons;
pub mod file_tree;
pub mod git_diff;
pub mod git_panel;
pub mod keyboard;
pub mod lsp_actions;
pub mod mouse;
pub mod project_search;
mod python_completion;
pub mod terminal;
pub mod ui_handlers;
use crate::editor::Editor;
use crate::highlighter::{CompletionItem, SymbolKind, TREE_SITTER_HIGHLIGHT_MAX_BYTES};
use crate::renderer::Renderer;
use app_state::fuzzy_match;
pub use app_state::*;
#[cfg(test)]
pub(crate) use autocomplete::{
    AutocompleteKeyAction, autocomplete_key_action, autocomplete_next_index,
};
pub(crate) use autocomplete::{
    AutocompletePopupKeyResult, CompletionApplyPlan, CompletionTextEditOp,
    apply_completion_plan_to_editor,
};
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

pub(crate) fn one_line_input_max_scroll_x(
    renderer: &mut Renderer,
    text: &str,
    visible_w: f32,
    text_scale: f32,
    trailing_pad: f32,
) -> f32 {
    let text_w = renderer.measure_ui_width(text, text_scale);
    (text_w - visible_w + trailing_pad).max(0.0)
}

pub(crate) fn sync_one_line_input_scroll_target(
    renderer: &mut Renderer,
    editor: &Editor,
    scroll: &mut crate::scroll::ScrollState,
    visible_w: f32,
    text_scale: f32,
    edge_pad: f32,
    immediate: bool,
) {
    let text = editor.get_full_text();
    let cursor = editor.cursor.min(text.len());
    let cursor_x = renderer.measure_ui_width(&text[..cursor], text_scale);
    let max_scroll =
        one_line_input_max_scroll_x(renderer, &text, visible_w, text_scale, edge_pad * 2.0);
    let mut target = scroll.target;
    if cursor_x - target > visible_w {
        target = cursor_x - visible_w + edge_pad;
    } else if cursor_x < target {
        target = cursor_x;
    }
    scroll.target = target.clamp(0.0, max_scroll);
    if immediate {
        scroll.current = scroll.target;
        scroll.velocity = 0.0;
    }
}

fn write_api_client_tab_display_title(
    meta: &crate::app::api_client::ApiClientTabMeta,
    fallback: &str,
    out: &mut String,
) {
    out.clear();
    let title = if meta.title.is_empty() {
        fallback
    } else {
        &meta.title
    };
    let Some(method) = meta.route_method else {
        if title.is_empty() {
            out.push_str("API");
        } else {
            out.push_str(title);
        }
        return;
    };
    out.push_str(title);
    out.push(' ');
    out.push_str(method.chip_str());
    out.push(' ');
    crate::app::api_client::append_api_path_display(&meta.route_path, out);
}

fn tab_effective_path<'a>(
    tabs: &'a [EditorTab],
    idx: usize,
    active_tab: usize,
    active_path: Option<&'a PathBuf>,
) -> Option<&'a PathBuf> {
    if idx == active_tab {
        active_path
    } else {
        tabs[idx].file_path.as_ref()
    }
}

pub(crate) fn tab_display_titles_for(
    tabs: &[EditorTab],
    active_tab: usize,
    active_path: Option<&PathBuf>,
    active_title: &str,
) -> Vec<String> {
    let mut display_titles = Vec::with_capacity(tabs.len());
    write_tab_display_titles_for(
        tabs,
        active_tab,
        active_path,
        active_title,
        &mut display_titles,
    );
    display_titles
}

pub(crate) fn write_tab_display_titles_for(
    tabs: &[EditorTab],
    active_tab: usize,
    active_path: Option<&PathBuf>,
    active_title: &str,
    display_titles: &mut Vec<String>,
) {
    display_titles.clear();
    display_titles.resize_with(tabs.len(), String::new);
    for i in 0..tabs.len() {
        let display_title = &mut display_titles[i];
        display_title.clear();
        if let Some(p1) = tab_effective_path(tabs, i, active_tab, active_path) {
            let mut diff_level = 0;
            let mut collision = false;
            for j in 0..tabs.len() {
                if i == j {
                    continue;
                }
                if let Some(p2) = tab_effective_path(tabs, j, active_tab, active_path)
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
                let mut rev_components = p1.components().rev();
                let file_name = rev_components.next();
                let diff_dir = p1.components().rev().nth(diff_level);
                if let (Some(diff_dir), Some(file_name)) = (diff_dir, file_name) {
                    display_title.push_str(&diff_dir.as_os_str().to_string_lossy());
                    if diff_level == 1 {
                        display_title.push('/');
                    } else {
                        display_title.push_str("/.../");
                    }
                    display_title.push_str(&file_name.as_os_str().to_string_lossy());
                } else {
                    display_title.push_str(&p1.to_string_lossy());
                }
            } else {
                display_title.push_str(&p1.file_name().unwrap_or_default().to_string_lossy());
            }
        } else {
            let title = if i == active_tab {
                active_title
            } else {
                &tabs[i].base_title
            };
            if let EditorTabKind::ApiClient(meta, _) = &tabs[i].kind {
                write_api_client_tab_display_title(meta, title, display_title);
            } else if title.is_empty() {
                display_title.push_str("Безымянный");
            } else {
                display_title.push_str(title);
            }
        }
    }
}

include!("app/app_ide_tab_methods.rs");
include!("app/app_file_tab_methods.rs");
include!("app/app_window_external_methods.rs");

#[cfg(test)]
mod app_behavior_tests;
