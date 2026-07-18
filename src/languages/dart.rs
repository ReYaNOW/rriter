pub use super::ImportBlock;
use super::finish_import_block;

pub fn import_blocks(text: &str) -> Vec<ImportBlock> {
    let mut blocks = Vec::new();
    let mut current: Option<ImportBlock> = None;
    let mut pending_blank_lines = 0usize;
    let mut offset = 0usize;

    for raw_line in text.split_inclusive('\n') {
        let line_start = offset;
        offset += raw_line.len();
        let line = raw_line.trim_end_matches('\n').trim_end_matches('\r');
        let line_end = line_start + line.len();
        let leading = line.len().saturating_sub(line.trim_start().len());
        let trimmed = line.trim_start();

        if trimmed.is_empty() && current.is_some() {
            pending_blank_lines += 1;
            continue;
        }

        if let Some(keyword_len) = dart_import_keyword_len(trimmed) {
            let keyword_start = line_start + leading;
            if let Some(block) = &mut current {
                block.end = line_end;
                block.line_count += pending_blank_lines + 1;
            } else {
                current = Some(ImportBlock {
                    start: line_start,
                    end: line_end,
                    keyword_start,
                    keyword_end: keyword_start + keyword_len,
                    line_count: 1,
                });
            }
            pending_blank_lines = 0;
            continue;
        }

        pending_blank_lines = 0;
        finish_import_block(&mut current, &mut blocks);
    }

    finish_import_block(&mut current, &mut blocks);
    blocks
}

fn dart_import_keyword_len(trimmed: &str) -> Option<usize> {
    if trimmed.starts_with("import ") {
        Some("import".len())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dart_import_blocks_cover_contiguous_imports() {
        let text = "import 'a.dart';\nimport 'b.dart';\n\nvoid main() {}\n";
        let blocks = import_blocks(text);
        assert_eq!(blocks.len(), 1);
        assert_eq!(
            &text[blocks[0].keyword_start..blocks[0].keyword_end],
            "import"
        );
        assert_eq!(
            &text[blocks[0].start..blocks[0].end],
            "import 'a.dart';\nimport 'b.dart';"
        );
    }

    #[test]
    fn dart_import_blocks_keep_blank_lines_between_import_groups_only() {
        let text = "import 'dart:async';\n\nimport 'package:a/a.dart';\n\nvoid main() {}\n";
        let blocks = import_blocks(text);
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].line_count, 3);
        assert_eq!(
            &text[blocks[0].start..blocks[0].end],
            "import 'dart:async';\n\nimport 'package:a/a.dart';"
        );
    }
}
