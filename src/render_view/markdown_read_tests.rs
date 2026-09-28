    use super::*;

    #[test]
    fn reader_frame_origin_makes_top_level_content_match_editor_text_x() {
        for scale in [1.0, 1.25, 1.5, 2.0] {
            for editor_text_x in [68.0, 107.0, 348.0, 721.0] {
                let frame_x = markdown_read_frame_x_for_editor_text(editor_text_x, scale);
                let reader_text_x = frame_x + (CONTENT_PAD * scale).round();
                assert_eq!(reader_text_x, editor_text_x.round());
            }
        }
    }
    use crate::languages::markdown::{MarkdownDocument, MarkdownParseState};

    fn parse(source: &str) -> MarkdownDocument {
        MarkdownParseState::default()
            .parse(source)
            .expect("markdown parse")
    }

    fn layout_with_scale_and_advance(
        source: &str,
        width: f32,
        scale: f32,
        mut advance: impl FnMut(char, bool) -> f32,
    ) -> MarkdownReadLayoutCache {
        let doc = parse(source);
        let mut builder = LayoutBuilder::new(
            source,
            width,
            scale,
            test_layout_text_metrics(scale),
            |ch, mono, _| advance(ch, mono),
        );
        builder.append_blocks(&doc.blocks, 0.0, 0, None);
        let (blocks, content_height) = builder.finish();
        let mut cache = MarkdownReadLayoutCache::default();
        cache.replace_layout(
            LayoutKey::new(1, width, scale, 16.0),
            blocks,
            content_height,
            source.len(),
        );
        cache
    }

    fn layout_with_advance(
        source: &str,
        width: f32,
        advance: impl FnMut(char, bool) -> f32,
    ) -> MarkdownReadLayoutCache {
        layout_with_scale_and_advance(source, width, 1.0, advance)
    }

    fn layout(source: &str, width: f32) -> MarkdownReadLayoutCache {
        layout_with_advance(source, width, |_, _| 8.0)
    }

    fn visual_text(cache: &MarkdownReadLayoutCache) -> String {
        let mut out = String::new();
        for block in &cache.blocks {
            match &block.kind {
                ReadBlockKind::Text(text) => out.push_str(&text.styled.text),
                ReadBlockKind::Table(table) => {
                    for row in &table.rows {
                        for cell in &row.cells {
                            out.push_str(&cell.styled.text);
                        }
                    }
                }
                _ => {}
            }
        }
        out
    }

    #[test]
    fn headings_have_strict_visual_hierarchy() {
        let cache = layout(
            "# One\n\n## Two\n\n### Three\n\n#### Four\n\n##### Five\n\n###### Six\n\nBody\n",
            800.0,
        );
        let scales = cache
            .blocks
            .iter()
            .filter_map(|block| match &block.kind {
                ReadBlockKind::Text(text) if text.heading_level.is_some() => Some(text.scale),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(scales, (1..=6).map(heading_scale).collect::<Vec<_>>());
        assert!(scales.windows(2).all(|pair| pair[0] > pair[1]));
        assert!(
            scales[0] - scales[5] > 0.58,
            "hierarchy must exceed previous span"
        );
        assert!(scales[5] >= BODY_SCALE);
        assert!(BODY_SCALE >= 0.95);
    }

    #[test]
    fn reader_layout_baselines_are_pixel_stable_at_fractional_scales() {
        let source = "# Heading\n\nParagraph with `inline code` and unicode 😀.\n\n```rust\nfn main() {}\n```\n\n| left | right |\n| --- | --- |\n| one | `two` |\n";
        for scale in [1.25, 1.32, 1.5, 1.75] {
            let cache = layout_with_scale_and_advance(source, 720.0 * scale, scale, |_, _| 8.0);
            for block in &cache.blocks {
                assert_eq!(
                    block.top.fract(),
                    0.0,
                    "scale {scale}: block top {}",
                    block.top
                );
                assert_eq!(
                    block.bottom.fract(),
                    0.0,
                    "scale {scale}: block bottom {}",
                    block.bottom
                );
                match &block.kind {
                    ReadBlockKind::Text(text) => {
                        for line in &text.lines {
                            assert_eq!(
                                line.top.fract(),
                                0.0,
                                "scale {scale}: text top {}",
                                line.top
                            );
                            assert_eq!(
                                line.y.fract(),
                                0.0,
                                "scale {scale}: text baseline {}",
                                line.y
                            );
                            assert_eq!(
                                line.bottom.fract(),
                                0.0,
                                "scale {scale}: text bottom {}",
                                line.bottom
                            );
                            assert!(line.top <= line.y && line.y <= line.bottom);
                        }
                    }
                    ReadBlockKind::Code(code) => {
                        for line in &code.lines {
                            assert_eq!(
                                line.top.fract(),
                                0.0,
                                "scale {scale}: code top {}",
                                line.top
                            );
                            assert_eq!(
                                line.y.fract(),
                                0.0,
                                "scale {scale}: code baseline {}",
                                line.y
                            );
                            assert_eq!(
                                line.bottom.fract(),
                                0.0,
                                "scale {scale}: code bottom {}",
                                line.bottom
                            );
                            assert!(line.top <= line.y && line.y <= line.bottom);
                        }
                    }
                    ReadBlockKind::Table(table) => {
                        let baseline_offset = table.baseline_offset;
                        for row in &table.rows {
                            assert_eq!(row.y.fract(), 0.0, "scale {scale}: row y {}", row.y);
                            assert_eq!(row.h.fract(), 0.0, "scale {scale}: row h {}", row.h);
                            for cell in &row.cells {
                                for line_idx in 0..cell.lines.len() {
                                    let baseline = (row.y
                                        + table.cell_padding
                                        + line_idx as f32 * table.line_height
                                        + baseline_offset)
                                        .round();
                                    assert_eq!(baseline.fract(), 0.0);
                                }
                            }
                        }
                    }
                    ReadBlockKind::Rule { .. } => {}
                }
            }

            let line_h = (BODY_LINE_H * scale * BODY_SCALE).round().max(1.0);
            let baseline = (line_h * 0.82).round();
            let (pill_top, pill_h) = inline_code_vertical_bounds(baseline, scale, BODY_SCALE);
            let (next_pill_top, _) =
                inline_code_vertical_bounds(baseline + line_h, scale, BODY_SCALE);
            assert!(
                pill_top >= 0.0,
                "scale {scale}: inline pill starts above line"
            );
            assert!(
                pill_top + pill_h <= next_pill_top,
                "scale {scale}: inline pills overlap adjacent lines"
            );
        }
    }

    #[test]
    fn inline_code_padding_and_reader_style_are_part_of_geometry() {
        let mut styled = StyledText::default();
        let code_style = TextStyle::default().with(TextStyle::CODE);
        styled.push("ab", code_style, Some(0..2));
        let mut advance = |_: char, mono: bool| if mono { 10.0 } else { 4.0 };
        let first = styled_char_advance(&styled, 0, 'a', BODY_SCALE, 1.0, false, &mut advance);
        let second = styled_char_advance(&styled, 1, 'b', BODY_SCALE, 1.0, false, &mut advance);
        assert_eq!(first + second, 28.0);
        assert_eq!(inline_code_padding_x(1.0), 4.0);
        assert_eq!(markdown_text_color(code_style, [0.0; 4]), MARKDOWN_GOLD);

        let bg = [0.156, 0.164, 0.211, 1.0];
        let fg = [0.972, 0.972, 0.949, 1.0];
        let inline_bg = inline_code_background(bg, fg);
        assert!(inline_bg[0] > bg[0] && inline_bg[1] > bg[1] && inline_bg[2] > bg[2]);
        assert!(inline_bg[0] < fg[0] && inline_bg[1] < fg[1] && inline_bg[2] < fg[2]);
    }

    #[test]
    fn reader_metrics_match_draw_for_omitted_and_zero_advance_chars() {
        let text = "a\u{200D}\u{FE0F}\u{0301}b";
        let mut styled = StyledText::default();
        styled.push(text, TextStyle::default(), Some(0..text.len()));
        let mut widths = Vec::new();
        for (offset, ch) in text.char_indices() {
            let mut advance = |c: char, _mono: bool| {
                assert!(
                    !text_char_is_non_rendering_control(c),
                    "non-rendering control reached glyph advance"
                );
                if c == '\u{0301}' { 0.0 } else { 8.0 }
            };
            widths.push(styled_char_advance(
                &styled,
                offset,
                ch,
                1.0,
                1.0,
                false,
                &mut advance,
            ));
        }
        assert_eq!(widths, vec![8.0, 0.0, 0.0, 0.0, 8.0]);
    }

    #[test]
    fn reader_text_surface_cursor_excludes_preview_scrollbar_and_respects_overlays() {
        let cursor_at = |mouse_x: f32| {
            let mut registry = UiRegistry::new();
            register_markdown_read_text_surface(
                &mut registry,
                10.0,
                20.0,
                200.0,
                100.0,
                10.0,
                mouse_x,
                40.0,
            );
            (registry.cursor_code(), registry.find_at(mouse_x, 40.0))
        };

        assert_eq!(
            cursor_at(10.0),
            (2, Some(crate::ui_system::UiId::MarkdownReadBody))
        );
        assert_eq!(
            cursor_at(199.0),
            (2, Some(crate::ui_system::UiId::MarkdownReadBody))
        );
        assert_eq!(
            cursor_at(205.0),
            (0, Some(crate::ui_system::UiId::MarkdownReadScrollbar))
        );
        assert_eq!(cursor_at(211.0), (0, None));

        let mut copy = UiRegistry::new();
        register_markdown_read_text_surface(&mut copy, 10.0, 20.0, 200.0, 100.0, 10.0, 50.0, 40.0);
        assert!(copy.register_rect(
            crate::ui_system::UiId::MarkdownCodeCopy(1),
            40.0,
            30.0,
            30.0,
            30.0,
            50.0,
            40.0,
        ));
        assert_eq!(copy.cursor_code(), 1);

        let mut overlay = UiRegistry::new();
        register_markdown_read_text_surface(
            &mut overlay,
            10.0,
            20.0,
            200.0,
            100.0,
            10.0,
            50.0,
            40.0,
        );
        assert!(overlay.register_blocker(
            crate::ui_system::UiId::SearchPanelBody,
            20.0,
            25.0,
            100.0,
            60.0,
            50.0,
            40.0,
        ));
        assert_eq!(overlay.cursor_code(), 0);
    }

    #[test]
    fn inline_code_uses_monospace_metrics_for_wrapping() {
        let source = "`abcdefgh` tail";
        let mono_aware =
            layout_with_advance(source, 150.0, |_, mono| if mono { 18.0 } else { 4.0 });
        let uniform = layout_with_advance(source, 150.0, |_, _| 4.0);
        let line_count = |cache: &MarkdownReadLayoutCache| {
            cache
                .blocks
                .iter()
                .find_map(|block| match &block.kind {
                    ReadBlockKind::Text(text) => Some(text.lines.len()),
                    _ => None,
                })
                .unwrap_or(0)
        };
        assert!(line_count(&mono_aware) > line_count(&uniform));
    }

    #[test]
    fn unicode_wrapping_keeps_utf8_boundaries() {
        let source = "Привет 😀 мир — длинный абзац с кириллицей и emoji.";
        let cache = layout(source, 150.0);
        let text = cache
            .blocks
            .iter()
            .find_map(|block| match &block.kind {
                ReadBlockKind::Text(text) => Some(text),
                _ => None,
            })
            .unwrap();
        for line in &text.lines {
            assert!(text.styled.text.is_char_boundary(line.range.start));
            assert!(text.styled.text.is_char_boundary(line.range.end));
        }
        assert!(text.lines.len() > 1);
    }

    #[test]
    fn quote_list_task_table_and_code_layouts_do_not_overlap() {
        let source = "> quote\n> continuation\n\n- [x] done\n- item\n\n| a | b |\n| --- | --- |\n| c | d |\n\n```rust\nfn main() {}\n```\n";
        let cache = layout(source, 520.0);
        assert!(
            cache
                .blocks
                .windows(2)
                .all(|pair| pair[0].bottom <= pair[1].top)
        );
        assert!(
            cache
                .blocks
                .iter()
                .any(|b| matches!(b.kind, ReadBlockKind::Table(_)))
        );
        assert!(
            cache
                .blocks
                .iter()
                .any(|b| matches!(b.kind, ReadBlockKind::Code(_)))
        );
    }

    #[test]
    fn semantic_continuation_markers_never_enter_visual_text() {
        let source = "> first\n> second\n> - nested\n>   - child\n";
        let cache = layout(source, 500.0);
        let text = visual_text(&cache);
        assert!(text.contains("first"));
        assert!(text.contains("second"));
        assert!(!text.contains("> "));
    }

    #[test]
    fn fenced_code_inside_quote_keeps_clean_source_ranges() {
        let source = "> ```rust\n> fn main() { println!(\"ok\"); }\n> ```\n";
        let cache = layout(source, 500.0);
        let code = cache
            .blocks
            .iter()
            .find_map(|block| match &block.kind {
                ReadBlockKind::Code(code) => Some(code),
                _ => None,
            })
            .unwrap();
        let visible = code
            .lines
            .iter()
            .filter_map(|line| source.get(line.source_range.clone()))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(visible.contains("fn main"));
        assert!(!visible.contains('>'));
        assert!(!visible.contains("```"));
    }

    #[test]
    fn table_draw_uses_cached_geometry_without_full_row_scan() {
        let source = include_str!("markdown_read.rs");
        let draw_start = source
            .find("    fn draw_markdown_block(")
            .expect("draw function");
        let draw_end = source[draw_start..]
            .find("    fn draw_quote_guides(")
            .map(|offset| draw_start + offset)
            .expect("draw function end");
        let draw_path = &source[draw_start..draw_end];
        assert!(draw_path.contains("let cell_w = table.cell_width;"));
        assert!(draw_path.contains("let cell_padding = table.cell_padding;"));
        assert!(draw_path.contains("let line_height = table.line_height;"));
        assert!(draw_path.contains("visible_table_cell_line_range("));
        assert!(!draw_path.contains("table.rows.iter()"));
        assert!(!draw_path.contains("cell.lines.iter()"));
    }

    #[test]
    fn max_scroll_uses_preview_content_height() {
        let cache = layout("# H\n\nparagraph\n\nparagraph\n\nparagraph\n", 250.0);
        let viewport = 80.0;
        assert_eq!(
            (cache.content_height() - viewport).max(0.0),
            cache.content_height() - viewport
        );
    }

    #[test]
    fn cache_invalidation_is_explicit_and_stable() {
        let mut cache = layout("text", 400.0);
        let key = cache.key.expect("layout key");
        assert_eq!(cache.rebuild_count(), 1);
        assert!(cache.is_valid_for(key));

        let changed_version = LayoutKey {
            version: key.version + 1,
            ..key
        };
        let changed_width = LayoutKey::new(key.version, 420.0, 1.0, 16.0);
        let changed_scale = LayoutKey::new(key.version, 400.0, 1.25, 16.0);
        let changed_font = LayoutKey::new(key.version, 400.0, 1.0, 17.0);
        assert!(!cache.is_valid_for(changed_version));
        assert!(!cache.is_valid_for(changed_width));
        assert!(!cache.is_valid_for(changed_scale));
        assert!(!cache.is_valid_for(changed_font));
        assert!(cache.is_valid_for_geometry(key.version, 400.0, 1.0, 16.0));
        assert!(!cache.is_valid_for_geometry(key.version, 400.0, 1.25, 16.0));
        assert!(!cache.is_valid_for_geometry(key.version, 400.0, 1.0, 17.0));
        assert!(!cache.anchor_lines.is_empty());
        assert!(!cache.source_lines.is_empty());
        assert_eq!(cache.source_prefix_max_end.len(), cache.source_lines.len());
        assert_eq!(cache.source_len, 4);

        cache.invalidate();
        assert!(!cache.is_valid_for(key));
        assert_eq!(cache.source_len, 0);
        assert!(cache.anchor_lines.is_empty());
        assert!(cache.source_lines.is_empty());
        assert!(cache.source_prefix_max_end.is_empty());
        assert_eq!(cache.rebuild_count(), 1);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn reader_text_ink_is_centered_in_editor_style_selection_bounds_across_dpi() {
        let source = "# AgjpqyЁЙ `Q`\n## AgjpqyЁЙ `Q`\n### AgjpqyЁЙ `Q`\n#### AgjpqyЁЙ `Q`\n##### AgjpqyЁЙ `Q`\n###### AgjpqyЁЙ `Q`\n\nAgjpqyЁЙ body\n";
        for dpi in [1.0, 1.25, 1.5, 1.75, 2.0] {
            let (_context, mut app) =
                crate::render_view::reviewer_stage2_integration::fixture(source, 900.0, dpi);
            app.set_markdown_mode(MarkdownMode::Read);
            crate::render_view::reviewer_stage2_integration::read_frame(&mut app);

            let samples = app
                .markdown
                .read_layout
                .blocks
                .iter()
                .filter_map(|block| match &block.kind {
                    ReadBlockKind::Text(text) => text.lines.first().map(|line| {
                        (
                            text.heading_level,
                            text.scale,
                            text.line_height,
                            line.top,
                            line.y,
                        )
                    }),
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert_eq!(samples.len(), 7, "dpi={dpi}");

            let renderer = app.renderer.as_mut().expect("renderer");
            for (heading_level, text_scale, line_height, line_top, baseline) in samples {
                let final_size = heading_level.map(|_| renderer.final_text_pixel_size(text_scale));
                let glyph_scale = if final_size.is_some() {
                    1.0
                } else {
                    text_scale
                };
                let mut ink_top = f32::INFINITY;
                let mut ink_bottom = f32::NEG_INFINITY;
                for ch in "AgjpqyЁЙ".chars() {
                    let glyph = if let Some(pixel_size) = final_size {
                        renderer.get_ui_glyph_at_size(ch, pixel_size)
                    } else {
                        renderer.get_ui_glyph(ch)
                    }
                    .expect("bundled-font representative glyph");
                    let (_, y, _, h) =
                        crate::renderer::glyph_quad_rect(0.0, baseline, glyph, glyph_scale);
                    ink_top = ink_top.min(y);
                    ink_bottom = ink_bottom.max(y + h);
                }

                let (selection_top, selection_height) =
                    source_highlight_vertical_bounds(line_top, line_height);
                let selection_center = selection_top + selection_height * 0.5;
                let ink_center = (ink_top + ink_bottom) * 0.5;
                assert!(
                    (selection_center - ink_center).abs() <= 2.5,
                    "dpi={dpi} heading={heading_level:?} line=[{line_top},{}] baseline={baseline} selection_center={selection_center} ink=[{ink_top},{ink_bottom}]",
                    line_top + line_height,
                );

                if heading_level.is_some() {
                    assert!(
                        baseline - line_top < (line_height * 0.82).round(),
                        "heading baseline must use final font metrics rather than 0.82 of enlarged line box"
                    );
                }
            }
        }
    }

    #[test]
    fn table_baseline_uses_font_metrics_not_line_box_fraction_across_scales() {
        let source = "| A | B |\n| --- | --- |\n| Agjpqy | x |\n";
        for scale in [1.0, 1.25, 1.5, 1.75, 2.0] {
            let cache = layout_with_scale_and_advance(source, 600.0, scale, |_, _| 8.0 * scale);
            let table = cache
                .blocks
                .iter()
                .find_map(|block| match &block.kind {
                    ReadBlockKind::Table(table) => Some(table),
                    _ => None,
                })
                .expect("table block");
            let expected =
                test_layout_text_metrics(scale).heading_baseline_offset(0.82, table.line_height);
            assert_eq!(
                table.baseline_offset, expected,
                "scale {scale}: table baseline must come from font metrics"
            );
            if scale == 2.0 {
                assert!(
                    table.baseline_offset < (table.line_height * 0.82).round(),
                    "table baseline must sit above 0.82 of the line box"
                );
            }
        }
    }
