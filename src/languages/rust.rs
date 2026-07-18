pub use super::ImportBlock;
use super::finish_import_block;

pub fn import_blocks(text: &str) -> Vec<ImportBlock> {
    let mut blocks = Vec::new();
    let mut current: Option<ImportBlock> = None;
    let mut pending_blank_lines = 0usize;
    let mut offset = 0usize;
    let mut continuing = false;
    let mut brace_depth = 0i32;

    for raw_line in text.split_inclusive('\n') {
        let line_start = offset;
        offset += raw_line.len();
        let line = raw_line.trim_end_matches('\n').trim_end_matches('\r');
        let line_end = line_start + line.len();
        let leading = line.len().saturating_sub(line.trim_start().len());
        let trimmed = line.trim_start();

        let import_start = rust_use_keyword_offset(trimmed).map(|rel| line_start + leading + rel);
        if trimmed.is_empty() && current.is_some() {
            pending_blank_lines += 1;
            continue;
        }

        if let Some(keyword_start) = import_start {
            if let Some(block) = &mut current {
                block.end = line_end;
                block.line_count += pending_blank_lines + 1;
            } else {
                current = Some(ImportBlock {
                    start: line_start,
                    end: line_end,
                    keyword_start,
                    keyword_end: keyword_start + "use".len(),
                    line_count: 1,
                });
            }
            pending_blank_lines = 0;
            update_use_continuation(trimmed, &mut brace_depth, &mut continuing);
            if !continuing {
                brace_depth = 0;
            }
            continue;
        }

        if continuing && !trimmed.is_empty() {
            if let Some(block) = &mut current {
                block.end = line_end;
                block.line_count += pending_blank_lines + 1;
            }
            pending_blank_lines = 0;
            update_use_continuation(trimmed, &mut brace_depth, &mut continuing);
            continue;
        }

        pending_blank_lines = 0;
        continuing = false;
        brace_depth = 0;
        finish_import_block(&mut current, &mut blocks);
    }

    finish_import_block(&mut current, &mut blocks);
    blocks
}

fn rust_use_keyword_offset(trimmed: &str) -> Option<usize> {
    if trimmed.starts_with("use ") {
        return Some(0);
    }
    if let Some(rest) = trimmed.strip_prefix("pub ") {
        return rest.starts_with("use ").then_some("pub ".len());
    }
    if trimmed.starts_with("pub(") {
        if let Some(close) = trimmed.find(')') {
            let after = trimmed[close + 1..].trim_start();
            if after.starts_with("use ") {
                return Some(close + 1 + trimmed[close + 1..].len() - after.len());
            }
        }
    }
    None
}

fn update_use_continuation(trimmed: &str, brace_depth: &mut i32, continuing: &mut bool) {
    for b in trimmed.bytes() {
        match b {
            b'{' | b'(' | b'[' => *brace_depth += 1,
            b'}' | b')' | b']' => *brace_depth -= 1,
            _ => {}
        }
    }
    *continuing = *brace_depth > 0 || !trimmed.ends_with(';');
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rust_import_blocks_cover_use_groups_and_pub_use_keyword() {
        let text = "use a::A;\npub use b::B;\n\nfn main() {}\n";
        let blocks = import_blocks(text);
        assert_eq!(blocks.len(), 1);
        assert_eq!(&text[blocks[0].keyword_start..blocks[0].keyword_end], "use");
        assert_eq!(blocks[0].line_count, 2);
    }

    #[test]
    fn rust_import_blocks_keep_multiline_use_inside_group() {
        let text = "use a::{\n    A,\n    B,\n};\nuse c::C;\n\nfn main() {}\n";
        let blocks = import_blocks(text);
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].line_count, 5);
    }

    #[test]
    fn rust_import_blocks_keep_blank_lines_between_use_groups_only() {
        let text = "use std::time::Instant;\n\nuse crate::app::App;\n\nfn main() {}\n";
        let blocks = import_blocks(text);
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].line_count, 3);
        assert_eq!(
            &text[blocks[0].start..blocks[0].end],
            "use std::time::Instant;\n\nuse crate::app::App;"
        );
    }
}
