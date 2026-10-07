pub(crate) fn byte_offset_for_char_col(line: &str, col: usize) -> usize {
    line.char_indices()
        .nth(col)
        .map(|(index, _)| index)
        .unwrap_or(line.len())
}

mod git_blame_state;
pub(crate) use git_blame_state::{
    age_rank, blame_blocks, head_line_for, parse_porcelain, truncate_to_width, BlameCommit,
    GitBlame, GitBlameContextMenu, GitBlameState, GitHeadSnapshot,
};

include!("editor/editor_core.rs");
include!("editor/editor_multi_cursor.rs");
include!("editor/editor_behavior_tests.rs");
