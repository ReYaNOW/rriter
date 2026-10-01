#[cfg(test)]
mod interaction_tests {
    use super::*;

    fn layout(source: &str, width: f32) -> MarkdownReadLayoutCache {
        build_test_markdown_read_layout(source, width)
    }

    fn mapped_styled_source_byte(
        styled: &StyledText,
        range: &Range<usize>,
        target_x: f32,
        text_scale: f32,
        layout_scale: f32,
        raw_advance: f32,
    ) -> usize {
        let text = styled.text.get(range.clone()).expect("styled range");
        let visual = visual_byte_at_x(text, range.start, target_x, |byte, ch| {
            let mut advance = |_ch: char, _mono: bool| raw_advance;
            styled_char_metrics(
                styled,
                byte,
                ch,
                text_scale,
                layout_scale,
                false,
                &mut advance,
            )
        });
        styled_source_boundary(styled, visual).expect("source boundary")
    }

    fn paint_layers(style: TextStyle, highlights: ReadHighlights<'_>) -> Vec<StyledRunPaintLayer> {
        STYLED_RUN_PAINT_ORDER
            .into_iter()
            .filter(|&layer| styled_run_layer_enabled(layer, style, !highlights.is_empty()))
            .collect()
    }

    fn paragraph_inline_code_style(cache: &MarkdownReadLayoutCache) -> TextStyle {
        cache
            .blocks
            .iter()
            .find_map(|block| match &block.kind {
                ReadBlockKind::Text(text) => text
                    .styled
                    .runs
                    .iter()
                    .find(|run| run.style.contains(TextStyle::CODE))
                    .map(|run| run.style),
                _ => None,
            })
            .expect("paragraph inline-code run")
    }

    #[test]
    fn inline_code_selection_paints_background_then_highlight_then_glyphs() {
        let source = "text `inline-code` tail\n";
        let cache = layout(source, 420.0);
        let start = source.find("inline-code").expect("inline source");
        let selection = start..start + "inline-code".len();
        let highlights = ReadHighlights {
            selection: Some(&selection),
            search_results: &[],
            search_current_idx: None,
        };

        assert_eq!(
            paint_layers(paragraph_inline_code_style(&cache), highlights),
            vec![
                StyledRunPaintLayer::InlineCodeBackground,
                StyledRunPaintLayer::SourceHighlights,
                StyledRunPaintLayer::Glyphs,
            ]
        );
    }

    #[test]
    fn inline_code_search_paints_above_background_and_keeps_active_color() {
        let source = "text `inline-code` tail\n";
        let cache = layout(source, 420.0);
        let start = source.find("inline-code").expect("inline source");
        let search = [(start, start + "inline-code".len())];
        let highlights = ReadHighlights {
            selection: None,
            search_results: &search,
            search_current_idx: Some(0),
        };

        assert_eq!(
            paint_layers(paragraph_inline_code_style(&cache), highlights),
            vec![
                StyledRunPaintLayer::InlineCodeBackground,
                StyledRunPaintLayer::SourceHighlights,
                StyledRunPaintLayer::Glyphs,
            ]
        );
        assert_eq!(
            search_highlight_color(highlights, 0),
            crate::render_view::SEARCH_ACTIVE_HIGHLIGHT_COLOR
        );
    }

    #[test]
    fn table_inline_code_uses_same_background_highlight_glyph_order() {
        let source = "| cell |\n| --- |\n| `inline-code` |\n";
        let cache = layout(source, 420.0);
        let style = cache
            .blocks
            .iter()
            .find_map(|block| match &block.kind {
                ReadBlockKind::Table(table) => table
                    .rows
                    .iter()
                    .flat_map(|row| row.cells.iter())
                    .flat_map(|cell| cell.styled.runs.iter())
                    .find(|run| run.style.contains(TextStyle::CODE))
                    .map(|run| run.style),
                _ => None,
            })
            .expect("table inline-code run");
        let start = source.find("inline-code").expect("inline source");
        let search = [(start, start + "inline-code".len())];
        let highlights = ReadHighlights {
            selection: None,
            search_results: &search,
            search_current_idx: None,
        };

        assert_eq!(
            paint_layers(style, highlights),
            vec![
                StyledRunPaintLayer::InlineCodeBackground,
                StyledRunPaintLayer::SourceHighlights,
                StyledRunPaintLayer::Glyphs,
            ]
        );
    }

    #[test]
    fn normal_text_and_no_highlight_fast_path_skip_unneeded_layers() {
        let selection = 0..1;
        let selection_highlights = ReadHighlights {
            selection: Some(&selection),
            search_results: &[],
            search_current_idx: None,
        };
        let none = ReadHighlights {
            selection: None,
            search_results: &[],
            search_current_idx: None,
        };
        let code = TextStyle::default().with(TextStyle::CODE);

        assert_eq!(
            paint_layers(TextStyle::default(), selection_highlights),
            vec![
                StyledRunPaintLayer::SourceHighlights,
                StyledRunPaintLayer::Glyphs,
            ]
        );
        assert_eq!(
            paint_layers(code, none),
            vec![
                StyledRunPaintLayer::InlineCodeBackground,
                StyledRunPaintLayer::Glyphs,
            ]
        );
    }

    #[test]
    fn reader_selection_mapping_copies_visible_text_without_markdown_punctuation() {
        let source = "# Заголовок 😀\n\nТекст **strong** и *emphasis* с `code λ`.\n\n```rust\nlet x = 1;\nprintln!(\"λ\");\n```\n";
        let cache = layout(source, 520.0);
        let copied = cache.copy_source_selection(source, &(0..source.len()));

        assert!(copied.contains("Заголовок 😀"));
        assert!(copied.contains("Текст strong и emphasis с code λ."));
        assert!(copied.contains("let x = 1;\nprintln!(\"λ\");"));
        assert!(!copied.contains("# "));
        assert!(!copied.contains("**"));
        assert!(!copied.contains('`'));
    }

    #[test]
    fn reader_visual_mapping_skips_hidden_markdown_syntax_and_link_destination() {
        let source = "**bold** **жир** _курсив_ [label](https://example.com) and `code`\n";
        let cache = layout(source, 520.0);
        let styled = cache
            .blocks
            .iter()
            .find_map(|block| match &block.kind {
                ReadBlockKind::Text(text) => Some(&text.styled),
                _ => None,
            })
            .expect("paragraph");

        assert!(!styled.text.contains("https://"));
        assert!(!styled.text.contains("**"));
        assert!(!styled.text.contains('`'));
        for needle in ["bold", "жир", "курсив", "label", "code"] {
            let visual = styled.text.find(needle).expect("visual needle");
            let source_byte = source.find(needle).expect("source needle");
            for (relative, _) in needle.char_indices() {
                assert_eq!(
                    styled_source_boundary(styled, visual + relative),
                    Some(source_byte + relative)
                );
            }
        }
    }

    #[test]
    fn inline_code_hit_test_uses_drawn_glyph_midpoint_not_padded_cell_midpoint() {
        let mut styled = StyledText::default();
        styled.push(
            "ab",
            TextStyle::default().with(TextStyle::CODE),
            Some(40..42),
        );
        let range = 0..styled.text.len();
        let mut advance = |_ch: char, _mono: bool| 8.0;
        let first = styled_char_metrics(&styled, 0, 'a', 1.0, 1.0, false, &mut advance);
        let second = styled_char_metrics(&styled, 1, 'b', 1.0, 1.0, false, &mut advance);

        assert_eq!(first, VisualCharMetrics { leading: 4.0, advance: 8.0, trailing: 0.0 });
        assert_eq!(second, VisualCharMetrics { leading: 0.0, advance: 8.0, trailing: 4.0 });
        let first_midpoint = first.leading + first.advance * 0.5;
        assert_eq!(first_midpoint, 8.0);
        assert_eq!(mapped_styled_source_byte(&styled, &range, 7.0, 1.0, 1.0, 8.0), 40);
        assert_eq!(mapped_styled_source_byte(&styled, &range, 9.0, 1.0, 1.0, 8.0), 41);

        let second_left = first.width() + second.leading;
        let second_midpoint = second_left + second.advance * 0.5;
        assert_eq!(second_midpoint, 16.0);
        assert_eq!(mapped_styled_source_byte(&styled, &range, 15.0, 1.0, 1.0, 8.0), 41);
        assert_eq!(mapped_styled_source_byte(&styled, &range, 17.0, 1.0, 1.0, 8.0), 42);
        assert_eq!(mapped_styled_source_byte(&styled, &range, -5.0, 1.0, 1.0, 8.0), 40);
        assert_eq!(mapped_styled_source_byte(&styled, &range, 30.0, 1.0, 1.0, 8.0), 42);
    }

    #[test]
    fn inline_code_hit_test_matches_wrapped_run_geometry_at_fractional_scales() {
        let mut styled = StyledText::default();
        styled.push(
            "abcd",
            TextStyle::default().with(TextStyle::CODE),
            Some(100..104),
        );
        let first_line = 0..2;
        let second_line = 2..4;

        for layout_scale in [1.25, 1.5, 1.75] {
            let raw_advance = 8.0 * layout_scale;
            let mut advance = |_ch: char, _mono: bool| raw_advance;
            let b = styled_char_metrics(
                &styled,
                1,
                'b',
                BODY_SCALE,
                layout_scale,
                false,
                &mut advance,
            );
            let c = styled_char_metrics(
                &styled,
                2,
                'c',
                BODY_SCALE,
                layout_scale,
                false,
                &mut advance,
            );
            let d = styled_char_metrics(
                &styled,
                3,
                'd',
                BODY_SCALE,
                layout_scale,
                false,
                &mut advance,
            );
            let pad = inline_code_padding_x(layout_scale);
            assert_eq!(b.trailing, 0.0, "soft wrap must not invent right padding");
            assert_eq!(c.leading, 0.0, "continuation line must not invent left padding");
            assert_eq!(d.trailing, pad);

            let c_midpoint = c.advance * 0.5;
            assert_eq!(
                mapped_styled_source_byte(
                    &styled,
                    &second_line,
                    c_midpoint - 0.25,
                    BODY_SCALE,
                    layout_scale,
                    raw_advance,
                ),
                102,
            );
            assert_eq!(
                mapped_styled_source_byte(
                    &styled,
                    &second_line,
                    c_midpoint + 0.25,
                    BODY_SCALE,
                    layout_scale,
                    raw_advance,
                ),
                103,
            );

            let first_a = styled_char_metrics(
                &styled,
                0,
                'a',
                BODY_SCALE,
                layout_scale,
                false,
                &mut advance,
            );
            assert_eq!(first_a.leading, pad);
            let a_midpoint = pad + first_a.advance * 0.5;
            assert_eq!(
                mapped_styled_source_byte(
                    &styled,
                    &first_line,
                    a_midpoint - 0.25,
                    BODY_SCALE,
                    layout_scale,
                    raw_advance,
                ),
                100,
            );
            assert_eq!(
                mapped_styled_source_byte(
                    &styled,
                    &first_line,
                    a_midpoint + 0.25,
                    BODY_SCALE,
                    layout_scale,
                    raw_advance,
                ),
                101,
            );
        }
    }

    #[test]
    fn table_inline_code_hit_test_keeps_midpoint_after_alignment_translation() {
        let mut styled = StyledText::default();
        styled.push(
            "ab",
            TextStyle::default().with(TextStyle::CODE),
            Some(200..202),
        );
        let range = 0..2;
        let text_scale = 0.82;

        for layout_scale in [1.25, 1.5, 1.75] {
            let raw_advance = 8.0 * layout_scale;
            let mut advance = |_ch: char, _mono: bool| raw_advance;
            let first = styled_char_metrics(
                &styled,
                0,
                'a',
                text_scale,
                layout_scale,
                false,
                &mut advance,
            );
            let second = styled_char_metrics(
                &styled,
                1,
                'b',
                text_scale,
                layout_scale,
                false,
                &mut advance,
            );
            let measured = first.width() + second.width();
            let cell_x = 100.0;
            let cell_w = 90.0 * layout_scale;
            let cell_pad = (6.0 * layout_scale).round();
            let centers = [
                cell_x + (cell_w - measured) * 0.5,
                cell_x + cell_w - cell_pad - measured,
            ];

            for tx in centers {
                let midpoint_world = tx + first.leading + first.advance * 0.5;
                for (world_x, expected) in [
                    (midpoint_world - 0.25, 200usize),
                    (midpoint_world + 0.25, 201usize),
                ] {
                    assert_eq!(
                        mapped_styled_source_byte(
                            &styled,
                            &range,
                            world_x - tx,
                            text_scale,
                            layout_scale,
                            raw_advance,
                        ),
                        expected,
                    );
                }
            }
        }
    }

    #[test]
    fn reader_visual_byte_mapping_skips_zero_width_unicode_and_keeps_tabs() {
        let text = "a\u{200D}\u{FE0F}\u{0301}\tb";
        let tab_byte = text.find('\t').expect("tab");
        let b_byte = text.find('b').expect("b");
        let mapped = |target_x: f32| {
            visual_byte_at_x(text, 100, target_x, |byte, ch| {
                let local = byte - 100;
                let advance = if text_char_is_non_rendering_control(ch) || ch == '\u{0301}' {
                    0.0
                } else if local == tab_byte {
                    16.0
                } else {
                    8.0
                };
                VisualCharMetrics::glyph(advance)
            })
        };

        assert_eq!(mapped(3.0), 100);
        assert_eq!(mapped(9.0), 100 + tab_byte);
        assert_eq!(mapped(17.0), 100 + b_byte);
        assert_eq!(mapped(40.0), 100 + text.len());
        for byte in [mapped(3.0), mapped(9.0), mapped(17.0), mapped(40.0)] {
            assert!(text.is_char_boundary(byte - 100));
        }
    }

    #[test]
    fn code_hit_test_padding_matches_draw_at_fractional_scales() {
        for scale in [1.0, 1.25, 1.3, 1.5, 1.75, 2.0] {
            assert_eq!(code_block_padding(scale), (CODE_PAD * scale).round());
        }
    }

    #[test]
    fn reader_selection_across_wrapped_blocks_does_not_copy_soft_wraps() {
        let source = "alpha beta gamma delta epsilon zeta eta theta\n\nsecond block\n";
        let cache = layout(source, 90.0);
        let first = cache
            .blocks
            .iter()
            .find_map(|block| match &block.kind {
                ReadBlockKind::Text(text) => Some(text),
                _ => None,
            })
            .expect("first paragraph");
        assert!(first.lines.len() > 1, "fixture must wrap visually");

        let copied = cache.copy_source_selection(source, &(0..source.len()));
        assert_eq!(
            copied,
            "alpha beta gamma delta epsilon zeta eta theta\nsecond block"
        );
    }
    #[test]
    fn reader_table_selection_serializes_cells_without_pipe_syntax() {
        let source = "| left | right |\n| --- | --- |\n| one | `two` |\n";
        let cache = layout(source, 420.0);
        let copied = cache.copy_source_selection(source, &(0..source.len()));

        assert!(copied.contains("left\tright"));
        assert!(copied.contains("one\ttwo"));
        assert!(!copied.contains('|'));
        assert!(!copied.contains('`'));
    }
    #[test]
    fn reader_search_source_targets_cover_paragraph_inline_code_and_fenced_code() {
        let source = "# title\n\nparagraph needle\n\ninline `code-needle`\n\n```rust\nlet fenced_needle = 1;\n```\n";
        let cache = layout(source, 500.0);
        let paragraph = source.find("needle").expect("paragraph match");
        let inline = source.find("code-needle").expect("inline match");
        let fenced = source.find("fenced_needle").expect("fenced match");

        let paragraph_y = cache
            .source_target_y(&(paragraph..paragraph + "needle".len()))
            .expect("paragraph target");
        let inline_y = cache
            .source_target_y(&(inline..inline + "code-needle".len()))
            .expect("inline target");
        let fenced_y = cache
            .source_target_y(&(fenced..fenced + "fenced_needle".len()))
            .expect("fenced target");

        assert!(paragraph_y < inline_y);
        assert!(inline_y < fenced_y);
    }

    #[test]
    fn code_block_copy_payload_uses_cached_content_ranges_without_fence_or_language() {
        let source = "```rust\nlet a = 1;\nlet b = 2;\n```\n\n```\nplain\n```\n";
        let cache = layout(source, 500.0);
        let ids = cache
            .blocks
            .iter()
            .filter_map(|block| match &block.kind {
                ReadBlockKind::Code(_) => Some(block.source_range.start),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(ids.len(), 2);
        assert_eq!(
            cache.code_block_copy_text(source, ids[0]).as_deref(),
            Some("let a = 1;\nlet b = 2;\n")
        );
        assert_eq!(
            cache.code_block_copy_text(source, ids[1]).as_deref(),
            Some("plain\n")
        );
    }

    #[test]
    fn code_block_header_reserves_fixed_space_and_language_stays_left() {
        let source = "```rust\nlet a = 1;\nlet b = 2;\n```\n";
        let cache = layout(source, 520.0);
        let (block, code) = cache
            .blocks
            .iter()
            .find_map(|block| match &block.kind {
                ReadBlockKind::Code(code) => Some((block, code)),
                _ => None,
            })
            .expect("code block");
        let pad = code_block_padding(1.0);
        let header_h = code_header_height(1.0);
        let baseline_offset = (code.line_height * 0.82).round();
        let first_content_y = code.lines.first().expect("code line").y - baseline_offset;
        assert_eq!(first_content_y, block.top + pad + header_h);
        assert_eq!(
            block.bottom - block.top,
            pad * 2.0 + header_h + code.lines.len() as f32 * code.line_height
        );

        let left = code.x;
        let right = 520.0 - CONTENT_PAD;
        let header = code_header_geometry(left, right, block.top, 1.0);
        assert_eq!(header.language_x, left + pad);
        assert_eq!(header.button_size, CODE_ACTION_SIZE);
        assert_eq!(header.button_icon_size, CODE_ACTION_ICON_SIZE);
        assert!(header.button_x > header.language_x);
        assert!(header.language_max_w > 0.0);
        assert!(
            header.language_x + header.language_max_w
                <= header.button_x - CODE_LANGUAGE_COPY_GAP
        );
        assert!(header.button_y >= block.top + pad);
        assert!(header.button_y + header.button_size <= block.top + pad + header_h);
    }

    #[test]
    fn code_header_language_and_copy_geometry_never_overlap_at_fractional_dpi() {
        for scale in [1.0_f32, 1.25, 1.5, 1.75, 2.0] {
            let left = (28.0 * scale).round();
            let top = (40.0 * scale).round();
            let right = (260.0 * scale).round();
            let header = code_header_geometry(left, right, top, scale);
            let pad = code_block_padding(scale);
            let header_h = code_header_height(scale);
            let gap = (CODE_LANGUAGE_COPY_GAP * scale).round().max(1.0);

            assert_eq!(header.button_size, (CODE_ACTION_SIZE * scale).round());
            assert_eq!(header.button_icon_size, (CODE_ACTION_ICON_SIZE * scale).round());
            assert!(header.language_max_w >= 0.0);
            assert!(header.language_x + header.language_max_w + gap <= header.button_x);
            assert!(header.button_y >= top + pad);
            assert!(header.button_y + header.button_size <= top + pad + header_h);
            let old_text_y = (top + pad + 24.0 * scale * 0.68).round();
            assert!(header.text_y <= old_text_y - (2.0 * scale).round());
            assert_eq!(header.text_y.fract(), 0.0);
        }
    }

    #[test]
    fn code_header_compacts_copy_inside_narrow_and_partially_visible_blocks() {
        let cases = [
            (100.0, 130.0, 1.0),
            (118.0, 142.0, 1.25),
            (-18.0, 20.0, 1.0),
            (164.0, 174.0, 1.5),
        ];
        for (left, right, scale) in cases {
            let header = code_header_geometry(left, right, 40.0, scale);
            let rounded_left = left.round();
            let rounded_right = right.round().max(rounded_left);

            assert!(header.button_x >= rounded_left, "case {left}..{right}");
            assert!(
                header.button_x + header.button_size <= rounded_right,
                "case {left}..{right}"
            );
            assert!(header.button_icon_size <= header.button_size);
            assert!(header.language_x <= header.button_x);
            assert_eq!(header.language_max_w, 0.0);
        }

        let narrow = code_header_geometry(100.0, 130.0, 0.0, 1.0);
        assert_eq!(narrow.button_x, 100.0);
        assert_eq!(narrow.button_size, 30.0);
        assert_eq!(narrow.button_icon_size, 16.0);
        assert_eq!(narrow.button_x + narrow.button_size, 130.0);
    }

    #[test]
    fn nested_quote_list_code_header_gives_copy_priority_at_narrow_width() {
        let source = "> - nested\n>\n>   ```rust\n>   code\n>   ```\n";
        let cache = layout(source, 180.0);
        let (block, code) = cache
            .blocks
            .iter()
            .find_map(|block| match &block.kind {
                ReadBlockKind::Code(code) => Some((block, code)),
                _ => None,
            })
            .expect("nested quoted code block");
        assert!(code.quote_depth > 0);
        assert!(code.x > CONTENT_PAD);

        let header = code_header_geometry(code.x, code.x + 30.0, block.top, 1.0);
        assert_eq!(header.language_max_w, 0.0);
        assert_eq!(header.button_x, code.x.round());
        assert_eq!(header.button_x + header.button_size, (code.x + 30.0).round());
    }

    #[test]
    fn quoted_code_copy_preserves_newlines_without_quote_or_fence_markup() {
        let source = "> ```bash\n> echo one\n> echo two\n> ```\n";
        let cache = layout(source, 500.0);
        let id = cache
            .blocks
            .iter()
            .find_map(|block| matches!(block.kind, ReadBlockKind::Code(_)).then_some(block.source_range.start))
            .expect("code block");
        let copied = cache.code_block_copy_text(source, id).expect("copy payload");
        assert_eq!(copied, "echo one\necho two\n");
        assert!(!copied.contains('>'));
        assert!(!copied.contains("```"));
        assert!(!copied.contains("bash"));
    }

    #[test]
    fn code_block_hover_uses_cached_block_geometry_and_distinguishes_blocks() {
        let source = "```rust\none\n```\n\ntext\n\n```\ntwo\n```\n";
        let cache = layout(source, 500.0);
        let code_blocks = cache
            .blocks
            .iter()
            .filter_map(|block| match &block.kind {
                ReadBlockKind::Code(code) => Some((block, code)),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(code_blocks.len(), 2);
        let frame = (10.0, 20.0, 500.0, 600.0);
        for (block, code) in code_blocks {
            let x = frame.0 + code.x + 8.0;
            let y = frame.1 + (block.top + block.bottom) * 0.5;
            assert_eq!(
                markdown_read_code_block_at_if_hover_valid(
                    true, &cache, frame, 0.0, 1.0, x, y,
                ),
                Some(block.source_range.start)
            );
            assert_eq!(
                markdown_read_code_block_at_if_hover_valid(
                    false, &cache, frame, 0.0, 1.0, x, y,
                ),
                None,
                "stale in-block mouse coordinates must not reactivate code-copy after leave"
            );
        }
        assert_eq!(
            markdown_read_code_block_at(&cache, frame, 0.0, 1.0, frame.0 - 1.0, frame.1 + 20.0),
            None
        );
    }
    #[test]
    fn reader_hidden_punctuation_search_target_falls_back_to_block_geometry() {
        let source = "# heading\n";
        let cache = layout(source, 500.0);
        assert!(cache.source_target_y(&(0..1)).is_some());
    }
    #[test]
    fn visible_selection_is_bounded_for_long_documents() {
        let source = (0..4000).map(|i| format!("paragraph {i}\n\n")).collect::<String>();
        let cache = layout(&source, 600.0);
        let visible = visible_block_range(&cache.blocks, 20_000.0, 20_800.0);
        assert!(visible.len() < cache.blocks.len() / 8);
    }
    #[test]
    fn huge_code_block_inner_selection_is_bounded() {
        let mut source = String::from("```rust\n");
        for i in 0..10_000 {
            source.push_str("let value_");
            source.push_str(&i.to_string());
            source.push_str(" = 42;\n");
        }
        source.push_str("```\n");
        let cache = layout(&source, 620.0);
        let code = cache
            .blocks
            .iter()
            .find_map(|block| match &block.kind {
                ReadBlockKind::Code(code) => Some(code),
                _ => None,
            })
            .expect("code block");
        assert!(code.lines.len() >= 10_000);
        let center = code.lines[5_000].y;
        let visible = visible_code_line_range(&code.lines, center - 500.0, center + 500.0);
        assert!(visible.len() < 64, "selected {} of {} code lines", visible.len(), code.lines.len());
    }
    #[test]
    fn huge_pipe_table_inner_selection_is_bounded() {
        let mut source = String::from("| left | right |\n| --- | --- |\n");
        for i in 0..3_000 {
            source.push('|');
            source.push_str(&i.to_string());
            source.push_str(" | value |\n");
        }
        let cache = layout(&source, 620.0);
        let table = cache
            .blocks
            .iter()
            .find_map(|block| match &block.kind {
                ReadBlockKind::Table(table) => Some(table),
                _ => None,
            })
            .expect("table block");
        assert!(table.rows.len() >= 3_000);
        assert!(table.cell_width > 0.0);
        assert_eq!(table.cell_width, table.width / 2.0);
        let center = table.rows[1_500].y;
        let visible = visible_table_row_range(&table.rows, center - 500.0, center + 500.0);
        assert!(visible.len() < 64, "selected {} of {} rows", visible.len(), table.rows.len());
    }
    #[test]
    fn huge_wrapped_table_cell_line_selection_is_bounded() {
        let mut source = String::from("| content |\n| --- |\n| ");
        for _ in 0..1_200 {
            source.push_str("слово😀слово😀 ");
        }
        source.push_str("|\n");

        let cache = layout(&source, 72.0);
        let table = cache
            .blocks
            .iter()
            .find_map(|block| match &block.kind {
                ReadBlockKind::Table(table) => Some(table),
                _ => None,
            })
            .expect("table block");
        assert!(table.rows.len() <= 3, "unexpected row count: {}", table.rows.len());

        let (row, cell) = table
            .rows
            .iter()
            .flat_map(|row| row.cells.iter().map(move |cell| (row, cell)))
            .max_by_key(|(_, cell)| cell.lines.len())
            .expect("table cell");
        assert!(
            cell.lines.len() >= 3_000,
            "expected thousands of wrapped cell lines, got {}",
            cell.lines.len()
        );

        let viewport_h = table.line_height * 40.0;
        let middle_idx = cell.lines.len() / 2;
        let middle_top = row.y
            + table.cell_padding
            + middle_idx as f32 * table.line_height;
        let middle = visible_table_cell_line_range(
            cell.lines.len(),
            row.y,
            table.cell_padding,
            table.line_height,
            middle_top,
            middle_top + viewport_h,
        );
        assert!(middle.len() < 64, "selected {} of {} cell lines", middle.len(), cell.lines.len());
        assert!(middle.len() * 20 < cell.lines.len());
        for idx in middle.clone() {
            let range = &cell.lines[idx];
            assert!(cell.styled.text.is_char_boundary(range.start));
            assert!(cell.styled.text.is_char_boundary(range.end));
        }

        let end_top = row.y
            + table.cell_padding
            + cell.lines.len().saturating_sub(24) as f32 * table.line_height;
        let end = visible_table_cell_line_range(
            cell.lines.len(),
            row.y,
            table.cell_padding,
            table.line_height,
            end_top,
            end_top + viewport_h,
        );
        assert!(end.len() < 64, "selected {} end cell lines", end.len());
        assert_eq!(end.end, cell.lines.len());
        for idx in end {
            let range = &cell.lines[idx];
            assert!(cell.styled.text.is_char_boundary(range.start));
            assert!(cell.styled.text.is_char_boundary(range.end));
        }
    }
    #[test]
    fn styled_run_selection_is_bounded_near_end_of_huge_unicode_paragraph() {
        let mut styled = StyledText::default();
        let plain = TextStyle::default();
        let strong = plain.with(TextStyle::STRONG);
        for i in 0..6_000 {
            let style = if i % 2 == 0 { plain } else { strong };
            styled.push(
                if i % 3 == 0 { "слово😀" } else { "text" },
                style,
                None,
            );
        }
        assert!(styled.runs.len() >= 5_000);

        let first = styled.runs.len() - 24;
        let last = styled.runs.len() - 8;
        let text_range = styled.runs[first].range.start..styled.runs[last].range.end;
        assert!(styled.text.is_char_boundary(text_range.start));
        assert!(styled.text.is_char_boundary(text_range.end));

        let visible = visible_styled_run_range(&styled.runs, &text_range);
        assert!(visible.start >= first);
        assert!(visible.len() <= last - first + 1);
        assert!(visible.len() < 32, "selected {} of {} styled runs", visible.len(), styled.runs.len());
        for run in &styled.runs[visible] {
            assert!(run.range.end > text_range.start);
            assert!(run.range.start < text_range.end);
            assert!(styled.text.is_char_boundary(run.range.start));
            assert!(styled.text.is_char_boundary(run.range.end));
        }
    }
    #[test]
    fn huge_wrapped_paragraph_inner_selection_is_bounded() {
        let mut source = String::new();
        for i in 0..12_000 {
            if i > 0 {
                source.push(' ');
            }
            source.push_str("слово😀");
        }
        source.push('\n');
        let cache = layout(&source, 140.0);
        let text = cache
            .blocks
            .iter()
            .find_map(|block| match &block.kind {
                ReadBlockKind::Text(text) => Some(text),
                _ => None,
            })
            .expect("paragraph block");
        assert!(text.lines.len() > 1_000);
        let center = text.lines[text.lines.len() / 2].y;
        let visible = visible_text_line_range(&text.lines, center - 500.0, center + 500.0);
        assert!(visible.len() < 64, "selected {} of {} text lines", visible.len(), text.lines.len());
        for idx in visible {
            let range = &text.lines[idx].range;
            assert!(text.styled.text.is_char_boundary(range.start));
            assert!(text.styled.text.is_char_boundary(range.end));
        }
    }

    #[test]
    fn reader_hit_test_helpers_choose_nearest_cached_geometry_in_large_layout() {
        let blocks = (0..20_000usize)
            .map(|idx| {
                let top = idx as f32 * 12.0;
                ReadBlock {
                    source_range: idx..idx + 1,
                    parent_source_ranges: Vec::new(),
                    top,
                    bottom: top + 8.0,
                    kind: ReadBlockKind::Rule {
                        x: 0.0,
                        width: 1.0,
                        quote_depth: 0,
                    },
                }
            })
            .collect::<Vec<_>>();
        assert_eq!(nearest_block_index(&blocks, 0.0), Some(0));
        assert_eq!(nearest_block_index(&blocks, blocks[15_000].top + 1.0), Some(15_000));
        let gap_y = (blocks[999].bottom + blocks[1_000].top) * 0.5;
        assert_eq!(nearest_block_index(&blocks, gap_y), Some(999));
        assert_eq!(nearest_block_index(&blocks, blocks[19_999].bottom + 100.0), Some(19_999));

        let lines = (0..50_000usize)
            .map(|idx| {
                let top = idx as f32 * 18.0;
                CodeLine {
                    source_range: idx..idx + 1,
                    top,
                    y: top + 15.0,
                    bottom: top + 18.0,
                }
            })
            .collect::<Vec<_>>();
        assert_eq!(
            line_box_index(&lines, lines[42_000].top + 1.0, |line| line.top, |line| line.bottom),
            Some(42_000)
        );
        assert_eq!(
            line_box_index(&lines, -100.0, |line| line.top, |line| line.bottom),
            Some(0)
        );
        assert_eq!(
            line_box_index(&lines, 1_000_000.0, |line| line.top, |line| line.bottom),
            Some(lines.len() - 1)
        );
    }

    #[test]
    fn reader_line_box_hit_test_uses_the_visible_line_not_nearest_baseline() {
        let lines = [
            CodeLine { source_range: 0..1, top: 10.0, y: 26.0, bottom: 30.0 },
            CodeLine { source_range: 1..2, top: 30.0, y: 46.0, bottom: 50.0 },
        ];
        for y in [10.0, 18.0, 29.99] {
            assert_eq!(line_box_index(&lines, y, |line| line.top, |line| line.bottom), Some(0), "y={y}");
        }
        for y in [30.0, 31.0, 40.0, 49.99] {
            assert_eq!(line_box_index(&lines, y, |line| line.top, |line| line.bottom), Some(1), "y={y}");
        }

        let gapped = [
            CodeLine { source_range: 0..1, top: 10.0, y: 26.0, bottom: 30.0 },
            CodeLine { source_range: 1..2, top: 34.0, y: 50.0, bottom: 54.0 },
        ];
        assert_eq!(line_box_index(&gapped, 31.0, |line| line.top, |line| line.bottom), Some(0));
        assert_eq!(line_box_index(&gapped, 33.0, |line| line.top, |line| line.bottom), Some(1));
    }

    fn link_hits(cache: &MarkdownReadLayoutCache, y: f32) -> Vec<(f32, Option<u32>)> {
        let mut advance = |_: char, _: bool, _: Option<f32>| 8.0;
        (0..400)
            .map(|x| (x as f32, cache.link_at(x as f32, y, 1.0, &mut advance)))
            .collect()
    }

    #[test]
    fn link_text_is_hit_only_over_its_own_glyphs() {
        let cache = layout("see [**bold** link](x.md#a) and more tail text\n", 800.0);
        assert_eq!(cache.links().len(), 1);
        let linked: Vec<u32> = cache
            .blocks
            .iter()
            .filter_map(|block| match &block.kind {
                ReadBlockKind::Text(text) => Some(text.styled.runs.iter().filter_map(|run| run.link)),
                _ => None,
            })
            .flatten()
            .collect();
        assert!(linked.len() >= 2, "bold and plain parts both carry the link");
        assert!(linked.iter().all(|index| *index == 0));
        let y = cache.blocks[0].top + 2.0;
        let hits = link_hits(&cache, y);
        let hit_xs: Vec<f32> = hits.iter().filter(|(_, hit)| *hit == Some(0)).map(|(x, _)| *x).collect();
        assert!(!hit_xs.is_empty());
        let (first, last) = (hit_xs[0], hit_xs[hit_xs.len() - 1]);
        // `bold link` is ten characters of 8 px, contiguous, and neither neighbour is hit.
        assert!((last - first + 1.0 - 80.0).abs() <= 8.0, "span {first}..{last}");
        assert!(hits.iter().filter(|(x, _)| *x > first && *x < last).all(|(_, hit)| *hit == Some(0)));
        assert_eq!(hits[0].1, None);
        assert_eq!(hits[399].1, None);
    }

    #[test]
    fn wrapped_link_runs_keep_one_index_and_hits_cover_both_lines_only() {
        let cache = layout("before [**жирный** текст](x.md) after\n", 145.0);
        assert_eq!(cache.links().len(), 1);
        let text = cache
            .blocks
            .iter()
            .find_map(|block| match &block.kind {
                ReadBlockKind::Text(text) => Some(text),
                _ => None,
            })
            .expect("text block");
        assert!(text.lines.len() >= 2, "link fixture must wrap: {:?}", text.lines);
        assert!(text
            .styled
            .runs
            .iter()
            .filter(|run| run.link.is_some())
            .all(|run| run.link == Some(0)));

        let line_hits: Vec<Vec<(f32, Option<u32>)>> =
            text.lines.iter().map(|line| link_hits(&cache, line.y)).collect();
        let linked_lines: Vec<_> = line_hits
            .iter()
            .filter(|hits| hits.iter().any(|(_, hit)| *hit == Some(0)))
            .collect();
        assert_eq!(linked_lines.len(), 2, "link hits on both wrapped lines");
        for hits in &line_hits {
            let link_pixels: Vec<f32> = hits
                .iter()
                .filter(|(_, hit)| *hit == Some(0))
                .map(|(x, _)| *x)
                .collect();
            if let (Some(first), Some(last)) = (link_pixels.first(), link_pixels.last()) {
                assert_eq!(hits.iter().find(|(x, _)| *x < *first).map(|(_, hit)| *hit), Some(None));
                assert_eq!(hits.iter().find(|(x, _)| *x > *last).map(|(_, hit)| *hit), Some(None));
            }
        }
    }

    #[test]
    fn unsupported_and_undefined_links_get_no_link_index() {
        let cache = layout("[a](javascript:alert(1)) [b][nope] [c]() plain\n", 800.0);
        assert!(cache.links().is_empty());
        let any_link = cache.blocks.iter().any(|block| match &block.kind {
            ReadBlockKind::Text(text) => text.styled.runs.iter().any(|run| run.link.is_some()),
            _ => false,
        });
        assert!(!any_link);
        let y = cache.blocks[0].top + 2.0;
        assert!(link_hits(&cache, y).iter().all(|(_, hit)| hit.is_none()));
        // Outside of any block there is nothing to hit either.
        let mut advance = |_: char, _: bool, _: Option<f32>| 8.0;
        assert_eq!(cache.link_at(10.0, -50.0, 1.0, &mut advance), None);
        assert_eq!(cache.link_at(f32::NAN, f32::NAN, 1.0, &mut advance), None);
    }

    #[test]
    fn table_line_hit_test_uses_uniform_line_boxes_at_boundaries() {
        assert_eq!(uniform_line_box_index(3, 20.0, 18.0, 19.0), Some(0));
        assert_eq!(uniform_line_box_index(3, 20.0, 18.0, 20.0), Some(0));
        assert_eq!(uniform_line_box_index(3, 20.0, 18.0, 37.99), Some(0));
        assert_eq!(uniform_line_box_index(3, 20.0, 18.0, 38.0), Some(1));
        assert_eq!(uniform_line_box_index(3, 20.0, 18.0, 56.0), Some(2));
        assert_eq!(uniform_line_box_index(3, 20.0, 18.0, 100.0), Some(2));
    }

}
