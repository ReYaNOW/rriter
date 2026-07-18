pub(crate) fn byte_offset_for_char_col(line: &str, col: usize) -> usize {
    line.char_indices()
        .nth(col)
        .map(|(index, _)| index)
        .unwrap_or(line.len())
}

include!("editor/editor_core.rs");
include!("editor/editor_behavior_tests.rs");
