pub mod core_text;
mod editor_text_layer;
mod hover_overlays;
mod ide_panels;
pub mod lsp_ui;
pub mod minimap_ui;
pub mod search;
pub mod settings_ui;
pub mod sticky;
pub mod tabs_ui;
pub mod terminal_ui;
pub(crate) mod tree_ui;
pub mod ui;

use crate::editor::Editor;
use crate::highlighter::ColorSpan;
use crate::renderer::Renderer;
use std::cell::RefCell;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

pub static TELEMETRY_ENABLED: AtomicBool = AtomicBool::new(false);
pub(crate) const EDITOR_BOTTOM_MIN_VISIBLE_LINES: f32 = 5.0;
pub(crate) const IDE_STATUS_BAR_HEIGHT: f32 = 30.0;

fn decimal_usize_buf(buf: &mut [u8; 20], mut n: usize) -> &str {
    let mut idx = buf.len();
    if n == 0 {
        idx -= 1;
        buf[idx] = b'0';
    } else {
        while n > 0 {
            idx -= 1;
            buf[idx] = b'0' + (n % 10) as u8;
            n /= 10;
        }
    }
    std::str::from_utf8(&buf[idx..]).unwrap_or("0")
}

#[inline(always)]
pub(crate) fn hover_trace_enabled() -> bool {
    false && TELEMETRY_ENABLED.load(Ordering::Relaxed)
}

#[inline(always)]
pub(crate) fn editor_bottom_blank_lines(viewport_height: f32, line_height: f32) -> f32 {
    if line_height <= 0.0 {
        return 0.0;
    }
    (viewport_height.max(0.0) / line_height - EDITOR_BOTTOM_MIN_VISIBLE_LINES).max(0.0)
}

#[inline(always)]
pub(crate) fn editor_scroll_content_height(
    lines_count: usize,
    line_height: f32,
    viewport_height: f32,
) -> f32 {
    if line_height <= 0.0 {
        return viewport_height.max(0.0);
    }
    (lines_count.max(1) as f32 + editor_bottom_blank_lines(viewport_height, line_height))
        * line_height
}

#[inline(always)]
pub(crate) fn editor_max_scroll_for_lines(
    lines_count: usize,
    line_height: f32,
    viewport_height: f32,
) -> f32 {
    if line_height <= 0.0 {
        return 0.0;
    }
    let raw_max = (editor_scroll_content_height(lines_count, line_height, viewport_height)
        - viewport_height.max(0.0))
    .max(0.0);
    (raw_max / line_height).ceil() * line_height
}

#[inline(always)]
pub(crate) fn ide_status_bar_height(scale: f32) -> f32 {
    IDE_STATUS_BAR_HEIGHT * scale
}

#[inline(always)]
pub(crate) fn ide_status_bar_y(window_height: f32, _panel_bottom_h: f32, scale: f32) -> f32 {
    (window_height - ide_status_bar_height(scale)).max(0.0)
}

#[inline(always)]
pub(crate) fn ide_bottom_panel_y(window_height: f32, panel_bottom_h: f32, scale: f32) -> f32 {
    (window_height - ide_status_bar_height(scale) - panel_bottom_h).max(0.0)
}

#[inline(always)]
pub(crate) fn editor_view_height(
    window_height: f32,
    tab_bar_h: f32,
    panel_bottom_h: f32,
    is_ide_mode: bool,
    scale: f32,
) -> f32 {
    let status_bar_h = if is_ide_mode {
        ide_status_bar_height(scale)
    } else {
        0.0
    };
    (window_height - tab_bar_h - panel_bottom_h - status_bar_h).max(0.0)
}

#[inline(always)]
fn utf8_char_width(first_byte: u8) -> usize {
    if first_byte < 0x80 {
        1
    } else if first_byte < 0xE0 {
        2
    } else if first_byte < 0xF0 {
        3
    } else {
        4
    }
}

pub(crate) fn cursor_line_and_character(editor: &Editor) -> (usize, usize) {
    let cursor = editor.cursor.min(editor.len());
    let line_idx = editor
        .line_offsets
        .partition_point(|&offset| offset <= cursor)
        .saturating_sub(1);
    let line_start = editor.line_offsets.get(line_idx).copied().unwrap_or(0);

    let mut byte_idx = line_start.min(cursor);
    let mut character = 1usize;
    while byte_idx < cursor {
        let step = utf8_char_width(editor.byte_at(byte_idx));
        if byte_idx.saturating_add(step) > cursor {
            break;
        }
        byte_idx += step;
        character += 1;
    }

    (line_idx + 1, character)
}

pub(crate) fn selected_char_count(editor: &Editor) -> Option<usize> {
    let anchor = editor.selection_anchor?;
    let cursor = editor.cursor.min(editor.len());
    let start = anchor.min(cursor).min(editor.len());
    let end = anchor.max(cursor).min(editor.len());
    if start == end {
        return None;
    }

    let mut byte_idx = start;
    let mut count = 0usize;
    while byte_idx < end {
        let step = utf8_char_width(editor.byte_at(byte_idx)).max(1);
        byte_idx = byte_idx.saturating_add(step).min(end);
        count += 1;
    }
    Some(count)
}

pub(crate) fn language_display_name_for_ext(ext: &str) -> &'static str {
    match crate::highlighter::tree_sitter_lang_name_for_ext(ext) {
        "bash" => "Shell",
        "rs" => "Rust",
        "py" => "Python",
        "toml" => "TOML",
        "go" => "Go",
        "js" => "JavaScript",
        "ts" => "TypeScript",
        "tsx" => "TypeScript JSX",
        "regex" => "Regex",
        "java" => "Java",
        "cs" => "C#",
        "dart" => "Dart",
        "html" => "HTML",
        "css" => "CSS",
        "json" => "JSON",
        "c" => "C",
        "cpp" => "C++",
        "make" => "Makefile",
        _ => "Text",
    }
}

pub(crate) fn diagnostic_error_warning_counts<'a>(
    diagnostic_sets: impl IntoIterator<Item = &'a [crate::lsp::Diagnostic]>,
) -> (usize, usize) {
    let mut errors = 0usize;
    let mut warnings = 0usize;
    for diagnostics in diagnostic_sets {
        for diagnostic in diagnostics {
            match diagnostic.severity {
                crate::lsp::DiagSeverity::Error => errors += 1,
                crate::lsp::DiagSeverity::Warning => warnings += 1,
                _ => {}
            }
        }
    }
    (errors, warnings)
}

thread_local! {
    static TELEMETRY: RefCell<Telemetry> = RefCell::new(Telemetry::default());
}

struct Telemetry {
    render_time: f32,
    render_count: u32,
    scroll_time: f32,
    scroll_count: u32,
    type_time: f32,
    type_count: u32,
    last_print: Instant,
}

impl Default for Telemetry {
    fn default() -> Self {
        Self {
            render_time: 0.0,
            render_count: 0,
            scroll_time: 0.0,
            scroll_count: 0,
            type_time: 0.0,
            type_count: 0,
            last_print: Instant::now(),
        }
    }
}

fn transient_python_member_dot_byte(editor: &Editor) -> Option<usize> {
    let cursor = editor.cursor.min(editor.len());
    if cursor < 2 || editor.byte_at(cursor - 1) != b'.' || editor.byte_at(cursor - 2) == b'.' {
        return None;
    }
    let prev = editor.byte_at(cursor - 2);
    (prev.is_ascii_alphanumeric() || prev == b'_').then_some(cursor - 1)
}

fn diagnostic_overlaps_transient_member_dot(
    dot_byte: Option<usize>,
    cursor: usize,
    diag_start: usize,
    diag_end: usize,
) -> bool {
    dot_byte.is_some_and(|dot_byte| diag_start <= cursor && diag_end.saturating_add(1) >= dot_byte)
}

pub(crate) fn should_suppress_active_line_useless_expression(
    diagnostic: &crate::lsp::Diagnostic,
    cursor_phys_line: usize,
) -> bool {
    if diagnostic.start_line as usize != cursor_phys_line {
        return false;
    }
    if diagnostic
        .code
        .as_deref()
        .is_some_and(|code| code.eq_ignore_ascii_case("B018") || code == "useless-expression")
    {
        return true;
    }
    diagnostic.message.contains("Found useless expression")
        || diagnostic.message.contains("useless-expression")
}

#[inline(always)]
fn should_draw_empty_ide_file_tree_overlay(
    is_ide_mode: bool,
    tabs_empty: bool,
    file_tree_overlay_open: bool,
) -> bool {
    is_ide_mode && tabs_empty && file_tree_overlay_open
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn telemetry_default_starts_with_empty_counters_and_fresh_print_time() {
        let before = Instant::now();
        let telemetry = Telemetry::default();
        let after = Instant::now();

        assert_eq!(telemetry.render_time, 0.0);
        assert_eq!(telemetry.render_count, 0);
        assert_eq!(telemetry.scroll_time, 0.0);
        assert_eq!(telemetry.scroll_count, 0);
        assert_eq!(telemetry.type_time, 0.0);
        assert_eq!(telemetry.type_count, 0);
        assert!(telemetry.last_print >= before);
        assert!(telemetry.last_print <= after);
        assert!(!TELEMETRY_ENABLED.load(Ordering::Relaxed));
    }

    #[test]
    fn editor_max_scroll_keeps_five_text_lines_at_bottom() {
        assert_eq!(editor_bottom_blank_lines(400.0, 10.0), 35.0);
        assert_eq!(editor_max_scroll_for_lines(100, 10.0, 400.0), 950.0);
        assert_eq!(editor_scroll_content_height(100, 10.0, 400.0), 1350.0);
    }

    #[test]
    fn editor_max_scroll_handles_short_files_and_tiny_viewports() {
        assert_eq!(editor_max_scroll_for_lines(5, 10.0, 400.0), 0.0);
        assert_eq!(editor_max_scroll_for_lines(4, 10.0, 400.0), 0.0);
        assert_eq!(editor_bottom_blank_lines(30.0, 10.0), 0.0);
        assert_eq!(editor_max_scroll_for_lines(100, 10.0, 30.0), 970.0);
        assert_eq!(editor_max_scroll_for_lines(100, 0.0, 400.0), 0.0);
    }

    #[test]
    fn status_bar_y_sits_above_bottom_panel() {
        assert_eq!(ide_status_bar_height(1.0), 30.0);
        assert_eq!(ide_status_bar_y(900.0, 0.0, 1.0), 870.0);
        assert_eq!(ide_status_bar_y(900.0, 180.0, 1.0), 870.0);
        assert_eq!(ide_status_bar_y(10.0, 20.0, 1.0), 0.0);
        assert_eq!(ide_bottom_panel_y(900.0, 180.0, 1.0), 690.0);
        assert_eq!(ide_bottom_panel_y(100.0, 180.0, 1.0), 0.0);
    }

    #[test]
    fn editor_view_height_excludes_status_and_bottom_panel() {
        assert_eq!(editor_view_height(900.0, 44.0, 0.0, true, 1.0), 826.0);
        assert_eq!(editor_view_height(900.0, 44.0, 240.0, true, 1.0), 586.0);
        assert_eq!(editor_view_height(900.0, 0.0, 240.0, false, 1.0), 660.0);
        assert_eq!(editor_view_height(20.0, 44.0, 240.0, true, 1.0), 0.0);
    }

    #[test]
    fn cursor_line_and_character_counts_unicode_scalars() {
        let mut editor = Editor::new(64);
        editor.insert_str("ab\nжz\n");
        editor.cursor = 0;
        assert_eq!(cursor_line_and_character(&editor), (1, 1));
        editor.cursor = 2;
        assert_eq!(cursor_line_and_character(&editor), (1, 3));
        editor.cursor = 5;
        assert_eq!(cursor_line_and_character(&editor), (2, 2));
        editor.cursor = editor.len();
        assert_eq!(cursor_line_and_character(&editor), (3, 1));
    }

    #[test]
    fn selected_char_count_counts_unicode_scalars() {
        let mut editor = Editor::new(64);
        editor.insert_str("aж😊z");
        editor.selection_anchor = Some(1);
        editor.cursor = editor.len() - 1;
        assert_eq!(selected_char_count(&editor), Some(2));
        editor.cursor = 1;
        assert_eq!(selected_char_count(&editor), None);
    }

    #[test]
    fn language_display_names_are_user_facing_not_short_codes() {
        assert_eq!(language_display_name_for_ext("py"), "Python");
        assert_eq!(language_display_name_for_ext("pyi"), "Python");
        assert_eq!(language_display_name_for_ext("rs"), "Rust");
        assert_eq!(language_display_name_for_ext("ts"), "TypeScript");
        assert_eq!(language_display_name_for_ext("txt"), "Text");
    }

    fn test_diagnostic(severity: crate::lsp::DiagSeverity) -> crate::lsp::Diagnostic {
        crate::lsp::Diagnostic {
            start_line: 0,
            start_col: 0,
            end_line: 0,
            end_col: 1,
            severity,
            code: None,
            code_href: None,
            message: String::new(),
            source: None,
            quickfixes: Vec::new(),
            tags: Vec::new(),
            spans: Vec::new(),
        }
    }

    #[test]
    fn diagnostic_status_counts_errors_and_warnings_only() {
        let first = vec![
            test_diagnostic(crate::lsp::DiagSeverity::Error),
            test_diagnostic(crate::lsp::DiagSeverity::Warning),
            test_diagnostic(crate::lsp::DiagSeverity::Hint),
        ];
        let second = vec![
            test_diagnostic(crate::lsp::DiagSeverity::Warning),
            test_diagnostic(crate::lsp::DiagSeverity::Info),
        ];

        assert_eq!(
            diagnostic_error_warning_counts([first.as_slice(), second.as_slice()]),
            (1, 2)
        );
    }

    #[test]
    fn active_line_useless_expression_suppresses_only_current_b018() {
        let mut diag = test_diagnostic(crate::lsp::DiagSeverity::Warning);
        diag.code = Some("B018".to_string());
        assert!(should_suppress_active_line_useless_expression(&diag, 0));
        assert!(!should_suppress_active_line_useless_expression(&diag, 1));

        diag.code = Some("useless-expression".to_string());
        assert!(should_suppress_active_line_useless_expression(&diag, 0));

        diag.code = None;
        diag.message = "Found useless expression. Either assign it to a variable or remove it."
            .to_string();
        assert!(should_suppress_active_line_useless_expression(&diag, 0));

        diag.code = Some("F401".to_string());
        diag.message = "imported but unused".to_string();
        assert!(!should_suppress_active_line_useless_expression(&diag, 0));
    }

    #[test]
    fn empty_ide_still_draws_file_tree_overlay() {
        assert!(should_draw_empty_ide_file_tree_overlay(true, true, true));
        assert!(!should_draw_empty_ide_file_tree_overlay(false, true, true));
        assert!(!should_draw_empty_ide_file_tree_overlay(true, false, true));
        assert!(!should_draw_empty_ide_file_tree_overlay(true, true, false));
    }

    #[test]
    fn transient_python_member_dot_byte_accepts_single_dot_only() {
        let mut editor = Editor::new(32);
        editor.insert_str("box.");
        assert_eq!(transient_python_member_dot_byte(&editor), Some(3));

        let mut double_dot = Editor::new(32);
        double_dot.insert_str("box..");
        assert_eq!(transient_python_member_dot_byte(&double_dot), None);

        let mut bare_dot = Editor::new(32);
        bare_dot.insert_str(".");
        assert_eq!(transient_python_member_dot_byte(&bare_dot), None);
    }

    #[test]
    fn diagnostic_overlap_suppresses_single_dot_not_double_dot() {
        let mut editor = Editor::new(32);
        editor.insert_str("box.");
        let dot = transient_python_member_dot_byte(&editor);
        assert!(diagnostic_overlaps_transient_member_dot(
            dot,
            editor.cursor,
            0,
            4
        ));
        assert!(!diagnostic_overlaps_transient_member_dot(
            dot,
            editor.cursor,
            0,
            1
        ));

        let mut double_dot = Editor::new(32);
        double_dot.insert_str("box..");
        assert!(!diagnostic_overlaps_transient_member_dot(
            transient_python_member_dot_byte(&double_dot),
            double_dot.cursor,
            0,
            5
        ));
    }
}
use glow::HasContext;

#[derive(Clone, Copy)]
pub struct ModInterval {
    pub top: f32,
    pub bottom: f32,
    pub state: crate::editor::LineModState,
}
