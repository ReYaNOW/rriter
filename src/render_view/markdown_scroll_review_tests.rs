#[cfg(test)]
mod reviewer_stage1_regressions {
    use super::*;

    #[test]
    fn reviewer_stage1_half_open_last_character_stays_on_its_rendered_line() {
        let source = "x".repeat(120);
        let cache = build_test_markdown_read_layout(&source, 120.0);
        let text = cache
            .blocks
            .iter()
            .find_map(|block| match &block.kind {
                ReadBlockKind::Text(text) => Some(text),
                _ => None,
            })
            .expect("production text layout");
        let pair = text
            .lines
            .windows(2)
            .find(|pair| pair[0].range.end == pair[1].range.start && pair[0].range.len() > 2)
            .expect("contiguous hard wrap");
        let range = pair[0].range.end - 1..pair[0].range.end;
        let expected = text_line_top(text, &pair[0]);
        let actual = cache.source_target_y(&range).expect("search target");
        println!("HALF_OPEN range={range:?} expected_y={expected} actual_y={actual}");
        assert_eq!(
            actual, expected,
            "a nonempty half-open range must not match the adjacent next line"
        );
    }

    #[test]
    fn reviewer_stage1_empty_fence_does_not_point_to_document_start() {
        let source = "first paragraph\n\nsecond paragraph\n\n```rust\n```\n\nafter\n";
        let cache = build_test_markdown_read_layout(source, 360.0);
        let (block, code) = cache
            .blocks
            .iter()
            .find_map(|block| match &block.kind {
                ReadBlockKind::Code(code) => Some((block, code)),
                _ => None,
            })
            .expect("production code layout");
        let viewport = code_line_top(code, &code.lines[0]) + 3.5;
        let anchor = cache
            .viewport_source_anchor(viewport)
            .expect("empty code anchor");
        let y = cache
            .source_anchor_y(&anchor.source_range)
            .expect("anchor projection");
        println!(
            "EMPTY_FENCE block={:?} viewport={viewport} anchor={anchor:?} projected_scroll={}",
            block.source_range,
            anchor.projected_scroll_y(y)
        );
        assert_ne!(
            anchor.source_range.start, 0,
            "a local empty fence must not fabricate a source position at document start"
        );
    }

    #[test]
    fn reviewer_stage1_blank_raw_line_does_not_jump_to_block_start() {
        let mut source = String::from("intro\n\n<pre>\n");
        for _ in 0..80 {
            source.push_str("value\n");
        }
        source.push_str("\nlast\n</pre>\n\nafter\n");
        let cache = build_test_markdown_read_layout(&source, 360.0);
        let text = cache
            .blocks
            .iter()
            .find_map(|block| match &block.kind {
                ReadBlockKind::Text(text) if text.mono => Some(text),
                _ => None,
            })
            .expect("production raw text layout");
        let line = text
            .lines
            .iter()
            .enumerate()
            .find(|(i, line)| *i > 60 && line.range.is_empty())
            .map(|(_, line)| line)
            .expect("deep empty raw line");
        let viewport = text_line_top(text, line) + 3.5;
        let anchor = cache
            .viewport_source_anchor(viewport)
            .expect("empty raw anchor");
        let y = cache
            .source_anchor_y(&anchor.source_range)
            .expect("anchor projection");
        let actual = anchor.projected_scroll_y(y);
        println!("RAW_EMPTY viewport={viewport} anchor={anchor:?} projected_scroll={actual}");
        assert!(
            (actual - viewport).abs() <= 1.0,
            "fallback must preserve local viewport offset, not reset a large raw block"
        );
    }

    #[test]
    fn reviewer_stage1_wrapped_synthetic_image_prefix_has_consistent_anchor() {
        let source = "intro\n\n![alt](destination)\n\nsuffix";
        let cache = build_test_markdown_read_layout(source, 80.0);
        let (block, text) = cache
            .blocks
            .iter()
            .find_map(|block| match &block.kind {
                ReadBlockKind::Text(text) if text.styled.text.contains("Image: ") => {
                    Some((block, text))
                }
                _ => None,
            })
            .expect("production image layout");
        let line = text.lines.first().expect("synthetic prefix line");
        assert!(
            text.styled
                .runs
                .iter()
                .all(|run| { run.source_range.is_none() || run.range.start >= line.range.end }),
            "fixture first line should consist only of the synthetic Image prefix"
        );
        let viewport = text_line_top(text, line) + 2.5;
        let anchor = cache
            .viewport_source_anchor(viewport)
            .expect("image prefix anchor");
        let y = cache
            .source_anchor_y(&anchor.source_range)
            .expect("anchor projection");
        let actual = anchor.projected_scroll_y(y);
        println!(
            "SYNTHETIC_IMAGE block={:?} viewport={viewport} anchor={anchor:?} projected_scroll={actual}",
            block.source_range
        );
        assert!(
            (actual - viewport).abs() <= 1.0,
            "synthetic fallback must use an offset from the same source geometry that resolves it"
        );
    }
}

#[cfg(test)]
mod reviewer_stage1_v2_regressions {
    use super::*;

    #[test]
    fn reviewer_v2_multiline_hidden_destination_keeps_owning_paragraph() {
        let source =
            "before\n\n[label](\nhttps://example.test/very/long/path/to/target\n)\n\nafter\n";
        let cache = build_test_markdown_read_layout(source, 360.0);
        let label = source.find("label").expect("label");
        let destination = source.find("https://").expect("url");
        let destination_end = destination + source[destination..].find('\n').expect("url line end");
        let label_y = cache.source_anchor_y(&(label..label + 5)).expect("label y");
        let owner = cache
            .blocks
            .iter()
            .find(|block| block.source_range.contains(&destination))
            .expect("owning block");
        let ReadBlockKind::Text(text) = &owner.kind else {
            panic!("expected a link paragraph")
        };
        assert!(
            !text.styled.text.contains("https://"),
            "fixture must hide destination"
        );
        let actual = cache
            .source_target_y(&(destination..destination_end))
            .expect("destination y");
        println!(
            "MULTILINE_URL query={:?} owner={:?} expected_y={label_y} actual_y={actual} visual={:?}",
            destination..destination_end,
            owner.source_range,
            text.styled.text
        );
        assert_eq!(
            actual, label_y,
            "an Edit viewport on the hidden destination line must project to its link paragraph, not a different block"
        );
    }

    #[test]
    fn reviewer_v2_hidden_fence_marker_stays_inside_owning_code_block() {
        let source = "before\n\n```rust\ncode\n```\n\nafter\n";
        let cache = build_test_markdown_read_layout(source, 360.0);
        let start = source.find("```rust").expect("opening fence");
        let block = cache
            .blocks
            .iter()
            .find(|block| matches!(&block.kind, ReadBlockKind::Code(_)))
            .expect("code block");
        let actual = cache.source_target_y(&(start..start + 3)).expect("fence y");
        println!(
            "FENCE query={:?} owner={:?} owner_top={} owner_bottom={} actual_y={actual}",
            start..start + 3,
            block.source_range,
            block.top,
            block.bottom
        );
        assert!(
            actual >= block.top && actual <= block.bottom,
            "the opening code marker must not target a preceding paragraph"
        );
    }

    #[test]
    fn reviewer_v2_empty_synthetic_interval_is_not_a_real_half_open_hit() {
        let source = "intro\n\n![alt](destination)\n\nsuffix";
        let cache = build_test_markdown_read_layout(source, 80.0);
        let marker = source.find("![alt]").expect("image marker");
        let alt = source.find("alt").expect("alt");
        let expected = cache
            .source_anchor_y(&(alt..alt + 1))
            .expect("real source line");
        let range = marker..alt + 1;
        let actual = cache
            .source_target_y(&range)
            .expect("marker plus first real char");
        let empty_count = cache
            .source_lines
            .iter()
            .filter(|entry| entry.source_range.is_empty() && entry.source_range.start == alt)
            .count();
        assert!(
            empty_count > 0,
            "fixture must have empty synthetic prefix intervals"
        );
        println!(
            "SYNTHETIC_HALF_OPEN query={range:?} real_char={:?} synthetic_intervals={empty_count} expected_y={expected} actual_y={actual}",
            alt..alt + 1
        );
        assert_eq!(
            actual, expected,
            "a zero-length synthetic entry cannot beat the first actual intersection with a nonempty source range"
        );
    }

    #[test]
    fn reviewer_v2_production_geometry_round_trips_many_viewports() {
        let source = "# Heading\n\nalpha **bold** beta [label](url) gamma delta epsilon zeta eta theta\n\n> - [x] quote item\n>   continued\n\n```rust\nlet x = 1;\n\nlet y = 2;\n```\n\n| left | center | right |\n| --- | --- | --- |\n|  | alpha beta gamma delta epsilon zeta eta theta lambda |  |\n| a | b | c |\n\n![alt](destination)\n\n<div>\nraw\n\nend\n</div>\n\nafter\n";
        for width in [80.0, 180.0, 640.0] {
            let cache = build_test_markdown_read_layout(source, width);
            let rebuilds = cache.rebuild_count();
            for step in 0..((cache.content_height * 2.0) as usize) {
                let viewport = step as f32 * 0.5 + 0.125;
                let anchor = cache.viewport_source_anchor(viewport).expect("anchor");
                assert!(source.is_char_boundary(anchor.source_range.start));
                assert!(source.is_char_boundary(anchor.source_range.end));
                let y = cache
                    .source_anchor_y(&anchor.source_range)
                    .expect("source y");
                let round_trip = anchor.projected_scroll_y(y);
                assert!(
                    (round_trip - viewport).abs() <= 0.001,
                    "width={width} viewport={viewport} anchor={anchor:?} y={y} actual={round_trip}"
                );
            }
            assert_eq!(cache.rebuild_count(), rebuilds);
        }
    }
    #[test]
    fn reviewer_v2_edit_viewport_on_multiline_link_title_keeps_owning_paragraph() {
        let source = "before\n\n[label](https://example.test/very/long/path/to/target\n\"title on a separate source line\"\n)\n\nafter\n";
        let cache = build_test_markdown_read_layout(source, 360.0);
        let label = source.find("label").expect("label");
        let title = source.find("\"title").expect("title");
        let title_end = title + source[title..].find('\n').expect("title line end");
        let label_y = cache.source_anchor_y(&(label..label + 5)).expect("label y");
        let owner = cache
            .blocks
            .iter()
            .find(|block| block.source_range.contains(&title))
            .expect("owning block");
        let ReadBlockKind::Text(text) = &owner.kind else {
            panic!("expected a link paragraph")
        };
        assert_eq!(
            text.styled.text, "label",
            "fixture must hide destination and title"
        );
        let mut editor = Editor::new(source.len() + 32);
        let _ = editor.insert_str(source);
        let mut map = Vec::new();
        crate::render_view::rebuild_editor_visual_line_map(&editor, &mut map);
        let line = editor
            .line_offsets
            .partition_point(|&offset| offset <= title)
            .saturating_sub(1);
        let anchor = editor_viewport_anchor_from_map(&editor, &map, 24.0, line as f32 * 24.0 + 3.5)
            .expect("Edit anchor");
        assert_eq!(anchor.source_range, title..title_end);
        let actual = cache
            .source_anchor_y(&anchor.source_range)
            .expect("title projection");
        println!(
            "MULTILINE_TITLE edit_anchor={anchor:?} owner={:?} expected_y={label_y} actual_y={actual} visual={:?}",
            owner.source_range, text.styled.text
        );
        assert_eq!(
            actual, label_y,
            "an Edit viewport on the hidden title line must project to its link paragraph, not the next block"
        );
    }

    #[test]
    fn reviewer_v2_search_in_hidden_destination_keeps_owning_paragraph() {
        let source = "before\n\n[label](https://example.test/very/long/path/to/target)\n\nafter\n";
        let cache = build_test_markdown_read_layout(source, 360.0);
        let label = source.find("label").expect("label");
        let target = source.find("target").expect("url suffix");
        let expected = cache.source_anchor_y(&(label..label + 5)).expect("label y");
        let actual = cache
            .source_target_y(&(target..target + 6))
            .expect("search y");
        println!(
            "HIDDEN_URL_SEARCH query={:?} expected_y={expected} actual_y={actual}",
            target..target + 6
        );
        assert_eq!(
            actual, expected,
            "search in a hidden URL must remain attached to the visible link, not jump to the following block"
        );
    }
}

#[cfg(test)]
mod reviewer_stage1_v3_regressions {
    use super::*;

    fn assert_marker_target(source: &str, marker: &str, target: &str, width: f32) {
        let cache = build_test_markdown_read_layout(source, width);
        let marker_byte = source.find(marker).expect("marker");
        let target_byte = source.find(target).expect("visible target");
        let marker_len = if marker.starts_with("|\n") {
            1
        } else {
            marker.len()
        };
        let query = marker_byte..marker_byte + marker_len;
        let expected = cache
            .source_anchor_y(&(target_byte..target_byte + target.len()))
            .expect("target Y");
        assert!(
            exact_read_source_line(&cache.source_lines, &cache.source_prefix_max_end, &query)
                .is_none(),
            "fixture marker must be hidden"
        );
        let actual = cache.source_target_y(&query).expect("marker Y");
        println!(
            "LOCAL_MARKER marker={marker:?} query={query:?} expected_y={expected} actual_y={actual} scopes={:?}",
            cache.source_lines
        );
        assert_eq!(
            actual, expected,
            "hidden container syntax must stay attached to its own visible content"
        );
    }

    #[test]
    fn reviewer_v3_task_list_prefix_keeps_its_own_item() {
        assert_marker_target("before\n\n- [x] target\n\nafter\n", "-", "target", 360.0);
    }

    #[test]
    fn reviewer_v3_long_ordered_list_prefix_keeps_its_own_item() {
        assert_marker_target(
            "before\n\n123456789. target\n\nafter\n",
            "123",
            "target",
            360.0,
        );
    }

    #[test]
    fn reviewer_v3_nested_quote_prefix_keeps_its_own_paragraph() {
        assert_marker_target("before\n\n> > > target\n\nafter\n", ">", "target", 360.0);
    }

    #[test]
    fn reviewer_v3_leading_table_pipe_keeps_its_own_header() {
        assert_marker_target(
            "before\n\n|                 left | right |\n| --- | --- |\n| a | b |\n\nafter\n",
            "|",
            "left",
            360.0,
        );
    }

    #[test]
    fn reviewer_v3_trailing_table_pipe_keeps_its_own_row() {
        assert_marker_target(
            "before\n\n| left | right |\n| --- | --- |\n| a | lastcell                   |\n\nafter\n",
            "|\n\nafter",
            "lastcell",
            360.0,
        );
    }

    #[test]
    fn reviewer_v3_edit_anchor_on_quote_only_line_keeps_its_container() {
        let source = "before\n\n> >\n> > target\n\nafter\n";
        let cache = build_test_markdown_read_layout(source, 360.0);
        let mut editor = Editor::new(source.len() + 32);
        let _ = editor.insert_str(source);
        let mut map = Vec::new();
        crate::render_view::rebuild_editor_visual_line_map(&editor, &mut map);
        let anchor = editor_viewport_anchor_from_map(&editor, &map, 24.0, 2.0 * 24.0 + 3.5)
            .expect("Edit anchor");
        assert_eq!(anchor.source_range, 8..11);
        let target = source.find("target").expect("target");
        let expected = cache
            .source_anchor_y(&(target..target + 6))
            .expect("target Y");
        let actual = cache
            .source_anchor_y(&anchor.source_range)
            .expect("Read projection");
        println!(
            "QUOTE_ONLY_EDIT anchor={anchor:?} expected_y={expected} actual_y={actual} scopes={:?}",
            cache.source_lines
        );
        assert_eq!(
            actual, expected,
            "an Edit viewport on container-only syntax must not leave that container"
        );
    }

    #[test]
    fn reviewer_v3_utf8_fractional_layout_round_trips_every_half_pixel() {
        let source = "# \u{0417}\u{0430}\u{0433}\u{043e}\u{043b}\u{043e}\u{0432}\u{043e}\u{043a}\n\n\u{03b1}\u{03b2}\u{03b3} **bold** [label](https://example.test \"title\") `\u{043a}\u{043e}\u{0434}` \u{1f600}\n\n> - [x] \u{0442}\u{0435}\u{043a}\u{0441}\u{0442}\n\n| left | middle | right |\n| --- | --- | --- |\n|  | \u{03b1}\u{03b2} gamma delta epsilon zeta eta theta iota |  |\n| x | y | z |\n\n![alt](destination)\n\n```rust\nlet a = 1;\n\nlet b = 2;\n```\n\n<pre>\nraw\n\nend\n</pre>\n\nlast\n";
        for scale in [1.0, 1.25, 1.5] {
            for width in [80.0, 180.0, 640.0] {
                let document = crate::languages::markdown::MarkdownParseState::default()
                    .parse(source)
                    .expect("parse");
                let mut builder = LayoutBuilder::new(source, width, scale, |_, _| 8.0 * scale);
                builder.append_blocks(&document.blocks, 0.0, 0, None);
                let (blocks, height) = builder.finish();
                let mut cache = MarkdownReadLayoutCache::default();
                cache.replace_layout(
                    LayoutKey::new(1, width, scale, 16.0),
                    blocks,
                    height,
                    source.len(),
                );
                let rebuilds = cache.rebuild_count();
                for step in 0..(height * 2.0) as usize {
                    let viewport = step as f32 * 0.5 + 0.125;
                    let anchor = cache.viewport_source_anchor(viewport).expect("anchor");
                    assert!(source.is_char_boundary(anchor.source_range.start));
                    assert!(source.is_char_boundary(anchor.source_range.end));
                    let y = cache
                        .source_anchor_y(&anchor.source_range)
                        .expect("projection");
                    assert!(
                        (anchor.projected_scroll_y(y) - viewport).abs() <= 0.001,
                        "scale={scale} width={width} viewport={viewport} anchor={anchor:?}"
                    );
                }
                assert_eq!(cache.rebuild_count(), rebuilds);
            }
        }
    }

    #[test]
    fn reviewer_v3_neighboring_list_items_keep_distinct_marker_ownership() {
        let source = "before\n\n- first\n- second\n\nafter\n";
        let cache = super::markdown_scroll_tests::scaled_layout(source, 360.0, 1.25);
        let markers = source
            .match_indices("- ")
            .map(|(byte, _)| byte)
            .collect::<Vec<_>>();
        let first = source.find("first").unwrap();
        let second = source.find("second").unwrap();
        let first_y = cache.source_anchor_y(&(first..first + 5)).unwrap();
        let second_y = cache.source_anchor_y(&(second..second + 6)).unwrap();
        assert_ne!(first_y, second_y);
        assert_eq!(
            cache.source_target_y(&(markers[0]..markers[0] + 1)),
            Some(first_y)
        );
        assert_eq!(
            cache.source_target_y(&(markers[1]..markers[1] + 1)),
            Some(second_y)
        );
    }

    #[test]
    fn reviewer_v3_nested_quote_gap_prefers_nearest_child_in_same_container() {
        let source = "before\n\n> > first\n> >\n> > second\n\nafter\n";
        let cache = build_test_markdown_read_layout(source, 360.0);
        let gap = source.find("\n> >\n").unwrap() + 1;
        let first = source.find("first").unwrap();
        let second = source.find("second").unwrap();
        let first_y = cache.source_anchor_y(&(first..first + 5)).unwrap();
        let second_y = cache.source_anchor_y(&(second..second + 6)).unwrap();
        assert_ne!(first_y, second_y);
        assert_eq!(cache.source_target_y(&(gap..gap + 3)), Some(first_y));
    }

    #[test]
    fn reviewer_v3_deep_wrapped_table_trailing_pipe_stays_on_its_row_tail() {
        let source = "| left | right |\n| --- | --- |\n| a | alpha beta gamma delta epsilon zeta eta theta iota kappa lambda tailword |\n";
        let cache = super::markdown_scroll_tests::scaled_layout(source, 180.0, 1.25);
        let tail = source.find("tailword").unwrap();
        let pipe = source.rfind('|').unwrap();
        let tail_y = cache.source_anchor_y(&(tail..tail + 8)).unwrap();
        let first = source.find("alpha").unwrap();
        let first_y = cache.source_anchor_y(&(first..first + 5)).unwrap();
        let pipe_y = cache.source_target_y(&(pipe..pipe + 1)).unwrap();
        assert!(tail_y > first_y);
        assert!(pipe_y > tail_y);
        assert_eq!(pipe_y, 641.0);
    }

    #[test]
    fn reviewer_v3_empty_container_uses_global_fallback_only_without_local_geometry() {
        let source = "before\n\n> >\n\nafter\n";
        let cache = build_test_markdown_read_layout(source, 360.0);
        let marker = source.find('>').unwrap();
        let after = source.find("after").unwrap();
        let before = source.find("before").unwrap();
        let expected = cache.source_anchor_y(&(after..after + 5)).unwrap();
        assert_ne!(
            expected,
            cache.source_anchor_y(&(before..before + 6)).unwrap()
        );
        assert_eq!(cache.source_target_y(&(marker..marker + 3)), Some(expected));
    }
}

#[cfg(test)]
mod reviewer_stage1_v4_regressions {
    use super::*;

    fn scaled(source: &str, width: f32, scale: f32) -> MarkdownReadLayoutCache {
        super::markdown_scroll_tests::scaled_layout(source, width, scale)
    }

    fn table(cache: &MarkdownReadLayoutCache) -> &TableBlock {
        cache
            .blocks
            .iter()
            .find_map(|block| match &block.kind {
                ReadBlockKind::Table(table) => Some(table),
                _ => None,
            })
            .expect("production table")
    }

    #[test]
    fn reviewer_v4_ragged_table_anchor_stays_on_the_displayed_source_row() {
        let source = "before\n\n| left | middle | right |\n| --- | --- | --- |\n| one |\n| target | b | c |\n\nafter\n";
        let cache = scaled(source, 360.0, 1.25);
        let row = &table(&cache).rows[1];
        let top = row.y + table(&cache).cell_padding;
        let anchor = cache
            .viewport_source_anchor(top + 3.5)
            .expect("Read row anchor");
        let mut editor = Editor::new(source.len() + 32);
        let _ = editor.insert_str(source);
        let mut map = Vec::new();
        crate::render_view::rebuild_editor_visual_line_map(&editor, &mut map);
        let row_byte = source.find("| one |").unwrap();
        let row_line = editor
            .line_offsets
            .partition_point(|&byte| byte <= row_byte)
            - 1;
        let expected = row_line as f32 * 24.0;
        let actual = editor_source_y_from_map(&editor, &map, 24.0, &anchor.source_range).unwrap();
        println!(
            "RAGGED_READ_EDIT row={:?} viewport={} anchor={anchor:?} expected_edit_y={expected} actual_edit_y={actual}",
            row.source_range,
            top + 3.5
        );
        assert_eq!(
            actual, expected,
            "a Read viewport on a short row must not project into the following source row because of padded cells"
        );
    }

    #[test]
    fn reviewer_v4_marker_after_ragged_row_uses_its_own_row() {
        let source = "before\n\n| left | middle | right |\n| --- | --- | --- |\n| one |\n| target | b | c |\n\nafter\n";
        let cache = scaled(source, 360.0, 1.25);
        let t = table(&cache);
        let expected = t.rows[2].y + t.cell_padding;
        let marker = source.find("| target").unwrap();
        let query = marker..marker + 1;
        assert!(
            exact_read_source_line(&cache.source_lines, &cache.source_prefix_max_end, &query)
                .is_none()
        );
        let actual = cache.source_target_y(&query).unwrap();
        println!(
            "RAGGED_NEXT_MARKER query={query:?} expected_y={expected} actual_y={actual} entries={:?}",
            cache.source_lines
        );
        assert_eq!(
            actual, expected,
            "synthetic entries from a preceding row must not hide the next row's own fallback candidate"
        );
    }

    #[test]
    fn reviewer_v4_point_after_ragged_row_has_downstream_affinity() {
        let source =
            "| left | middle | right |\n| --- | --- | --- |\n| one |\n| target | b | c |\n";
        let cache = scaled(source, 360.0, 1.0);
        let t = table(&cache);
        let expected = t.rows[2].y + t.cell_padding;
        let byte = source.find("| target").unwrap();
        let actual = cache.source_anchor_y(&(byte..byte)).unwrap();
        println!("RAGGED_POINT byte={byte} expected_y={expected} actual_y={actual}");
        assert_eq!(
            actual, expected,
            "a point at the next physical row start must not inherit the previous row's synthetic cells"
        );
    }

    #[test]
    fn reviewer_v4_fallback_does_not_skip_a_more_specific_existing_owner() {
        let fixtures = [
            "before\n\n> - first\n>\n>   second\n>   > quoted\n>\n> - last\n\nafter\n",
            "before\n\n- ```rust\n  let x = 1;\n\n  let y = 2;\n  ```\n- end\n\nafter\n",
            "before\n\n| left | middle | right |\n| --- | --- | --- |\n| one |\n| target | b | c |\n\nafter\n",
            "before\n\n> | left | right |\n> | --- | --- |\n> | a | b |\n> | c | d |\n\nafter\n",
            "before\n\n1. > > [label](long-hidden-destination)\n   > >\n   > > next\n2. last\n\nafter\n",
        ];
        let mut checked = 0;
        for source in fixtures {
            let cache = scaled(source, 180.0, 1.25);
            for (byte, ch) in source.char_indices() {
                let query = byte..byte + ch.len_utf8();
                if exact_read_source_line(&cache.source_lines, &cache.source_prefix_max_end, &query)
                    .is_some()
                {
                    continue;
                }
                // Exhaustive test oracle only: every layout owner with geometry is eligible.
                let own_span = cache
                    .source_scopes
                    .iter()
                    .flatten()
                    .filter(|scope| scope.start <= byte && query.end <= scope.end)
                    .map(|scope| scope.end - scope.start)
                    .min();
                let Some(own_span) = own_span else {
                    continue;
                };
                let actual = read_source_line(
                    &cache.source_lines,
                    &cache.source_prefix_max_end,
                    &cache.source_scopes,
                    &query,
                )
                .unwrap();
                let actual_span = cache.source_scopes[actual.scope_index]
                    .iter()
                    .filter(|scope| scope.start <= byte && query.end <= scope.end)
                    .map(|scope| scope.end - scope.start)
                    .min();
                assert_eq!(
                    actual_span,
                    Some(own_span),
                    "query={query:?} char={ch:?} chosen={actual:?} source={source:?}"
                );
                checked += 1;
            }
        }
        println!("OWNERSHIP checked={checked}");
        assert!(checked > 50);
    }

    fn check_styled(
        styled: &StyledText,
        range: &Range<usize>,
        y: f32,
        cache: &MarkdownReadLayoutCache,
    ) -> usize {
        let mut count = 0;
        for run in &styled.runs {
            let Some(source) = &run.source_range else {
                continue;
            };
            let a = range.start.max(run.range.start);
            let b = range.end.min(run.range.end);
            if a >= b {
                continue;
            }
            for (offset, ch) in styled.text[a..b].char_indices() {
                let start = source.start + a + offset - run.range.start;
                let query = start..start + ch.len_utf8();
                assert_eq!(
                    cache.source_anchor_y(&query),
                    Some(y),
                    "draw fragment query={query:?} expected_y={y}"
                );
                count += 1;
            }
        }
        count
    }

    #[test]
    fn reviewer_v4_exact_fragments_match_production_draw_geometry() {
        let source = "# Heading\n\n\u{03b1}\u{03b2}\u{03b3} **strong** _em_ [label](hidden) `code` \\* alpha beta gamma delta epsilon\n\n> - first\n>   continuation\n> - \u{0442}\u{0435}\u{043a}\u{0441}\u{0442}\n\n| left | right |\n| --- | --- |\n| alpha beta gamma delta epsilon | \u{043a}\u{043e}\u{0434} **bold** omega |\n\n```rust\nlet one = 1;\n\nlet two = 2;\n```\n\n![alt](destination)\n\n<pre>\nraw\n\nend\n</pre>\n";
        let mut checked = 0;
        for scale in [1.0, 1.25, 1.5, 1.75, 2.0] {
            for width in [80.0, 180.0, 640.0] {
                let cache = scaled(source, width, scale);
                let rebuilds = cache.rebuild_count();
                for block in &cache.blocks {
                    match &block.kind {
                        ReadBlockKind::Text(text) => {
                            for line in &text.lines {
                                checked += check_styled(
                                    &text.styled,
                                    &line.range,
                                    text_line_top(text, line),
                                    &cache,
                                );
                            }
                        }
                        ReadBlockKind::Code(code) => {
                            for line in &code.lines {
                                if !line.source_range.is_empty() {
                                    assert_eq!(
                                        cache.source_anchor_y(&line.source_range),
                                        Some(code_line_top(code, line))
                                    );
                                    checked += 1;
                                }
                            }
                        }
                        ReadBlockKind::Table(t) => {
                            for row in &t.rows {
                                for cell in &row.cells {
                                    for (idx, range) in cell.lines.iter().enumerate() {
                                        let y =
                                            (row.y + t.cell_padding + idx as f32 * t.line_height)
                                                .round();
                                        checked += check_styled(&cell.styled, range, y, &cache);
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                }
                assert_eq!(cache.rebuild_count(), rebuilds);
            }
        }
        println!("EXACT_DRAW checked={checked}");
        assert!(checked > 2000);
    }

    #[test]
    fn reviewer_v4_deep_table_tail_matches_real_cached_geometry() {
        let source = "| left | right |\n| --- | --- |\n| a | alpha beta gamma delta epsilon zeta eta theta iota kappa lambda tailword |\n";
        let cache = scaled(source, 180.0, 1.25);
        let t = table(&cache);
        let row = &t.rows[1];
        let cell = &row.cells[1];
        let expected = row.y + t.cell_padding + (cell.lines.len() - 1) as f32 * t.line_height;
        let pipe = source.rfind('|').unwrap();
        let actual = cache.source_target_y(&(pipe..pipe + 1)).unwrap();
        println!(
            "TAIL_LAST_LINE visual={:?} expected_y={expected} actual_y={actual}",
            &cell.styled.text[cell.lines.last().unwrap().clone()]
        );
        assert_eq!(
            actual, expected,
            "tail fallback follows the actual final wrapped line, not a hard-coded coordinate"
        );
    }
}
