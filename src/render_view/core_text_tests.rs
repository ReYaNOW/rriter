    use super::*;

    fn editor_with_text(text: &str) -> Editor {
        let mut editor = Editor::new(text.len() + 16);
        let _ = editor.insert_str(text);
        editor
    }

    fn test_glyph(
        offset_x: f32,
        offset_y: f32,
        width: f32,
        height: f32,
    ) -> crate::renderer::GlyphInfo {
        crate::renderer::GlyphInfo {
            u: 0.0,
            v: 0.0,
            uw: 1.0,
            vh: 1.0,
            width,
            height,
            offset_x,
            offset_y,
            advance: 8.0,
            is_emoji: 0.0,
        }
    }

    #[test]
    fn compact_ui_glyphs_snap_final_edges_not_offset_and_size_separately() {
        let glyph = test_glyph(0.4, 12.4, 7.4, 10.4);
        let rect = pixel_stable_glyph_rect(10.0, 100.0, glyph, 0.74).expect("visible glyph");
        let vertices = crate::renderer::quad_vertices(
            rect.0, rect.1, rect.2, rect.3, 0.0, 0.0, 1.0, 1.0, [1.0; 4], 0.0,
        );

        assert_eq!(vertices[0].pos, [10.0, 91.0]);
        assert_eq!(vertices[2].pos, [16.0, 99.0]);
    }

    #[test]
    fn markdown_mono_glyph_geometry_is_pixel_stable_at_fractional_scale() {
        let glyph = test_glyph(0.35, 12.55, 7.45, 10.6);
        let rect = pixel_stable_glyph_rect(13.4, 101.6, glyph, 0.96).expect("visible mono glyph");
        let first = crate::renderer::quad_vertices(
            rect.0, rect.1, rect.2, rect.3, 0.0, 0.0, 1.0, 1.0, [1.0; 4], 0.0,
        )
        .map(|vertex| vertex.pos);
        let repeated = pixel_stable_glyph_rect(13.4, 101.6, glyph, 0.96)
            .expect("repeat mono glyph");
        let second = crate::renderer::quad_vertices(
            repeated.0,
            repeated.1,
            repeated.2,
            repeated.3,
            0.0,
            0.0,
            1.0,
            1.0,
            [1.0; 4],
            0.0,
        )
        .map(|vertex| vertex.pos);

        assert!(first.iter().flatten().all(|value| value.fract() == 0.0));
        assert_eq!(first, second);
    }

    #[test]
    fn pixel_stable_editor_glyph_passes_preserve_punctuation_semantics() {
        let glyph = test_glyph(0.35, 12.55, 7.45, 10.6);
        let rect = pixel_stable_glyph_rect(13.4, 101.6, glyph, 1.0)
            .expect("visible editor glyph");
        let vertices = crate::renderer::quad_vertices(
            rect.0, rect.1, rect.2, rect.3, 0.0, 0.0, 1.0, 1.0, [1.0; 4], 0.0,
        );
        let q_x = vertices[0].pos[0];

        let (normal_x, normal_count) = editor_glyph_pass_x_positions('a', q_x);
        assert_eq!(normal_count, 1);
        assert_eq!(&normal_x[..normal_count], &[q_x]);

        for punctuation in ['.', ':'] {
            let (pass_x, pass_count) = editor_glyph_pass_x_positions(punctuation, q_x);
            assert_eq!(pass_count, 2);
            assert_eq!(pass_x[0], q_x);
            assert_eq!(pass_x[1], q_x + 1.0);
            assert!(pass_x[..pass_count].iter().all(|x| x.fract() == 0.0));
        }

        let (emoji_x, emoji_count) = editor_glyph_pass_x_positions('😀', q_x);
        assert_eq!(emoji_count, 1);
        assert_eq!(&emoji_x[..emoji_count], &[q_x]);
    }

    #[test]
    fn compact_ui_glyphs_skip_empty_quads() {
        assert_eq!(
            pixel_stable_glyph_rect(10.0, 100.0, test_glyph(0.0, 0.0, 0.0, 10.0), 0.74),
            None
        );
    }

    #[test]
    fn compact_tree_label_stable_geometry_preserves_shared_glyph_edge() {
        let scale = 0.86;
        let baseline = 100.0;
        let first = test_glyph(0.0, 8.0, 6.0, 5.98);
        let second = test_glyph(0.0, 8.42, 6.0, 6.4);
        let bottom = |x: f32, glyph| {
            let rect = pixel_stable_glyph_rect(x, baseline, glyph, scale)
                .expect("visible aligned glyph");
            crate::renderer::quad_vertices(
                rect.0, rect.1, rect.2, rect.3, 0.0, 0.0, 1.0, 1.0, [1.0; 4], 0.0,
            )[2]
                .pos[1]
        };

        assert_eq!(bottom(10.0, first), bottom(18.0, second));
    }

    #[test]
    fn markdown_language_label_geometry_preserves_shared_edges_at_fractional_dpi() {
        let baseline = 100.0;
        let first = test_glyph(0.0, 8.0, 6.0, 5.98);
        let second = test_glyph(0.0, 8.42, 6.0, 6.4);
        let descender = test_glyph(0.0, 8.0, 6.0, 7.4);

        for dpi in [1.0, 1.25, 1.5, 1.75, 2.0] {
            let scale = 0.80 * dpi;
            let bottom = |x: f32, glyph| {
                let rect = pixel_stable_glyph_rect(x, baseline, glyph, scale)
                    .expect("visible markdown label glyph");
                crate::renderer::quad_vertices(
                    rect.0, rect.1, rect.2, rect.3, 0.0, 0.0, 1.0, 1.0, [1.0; 4], 0.0,
                )[2]
                    .pos[1]
            };
            assert_eq!(bottom(10.0, first), bottom(18.0, second), "dpi {dpi}");
            assert_ne!(bottom(10.0, first), bottom(26.0, descender), "dpi {dpi}");
        }
    }

    #[test]
    fn compact_tree_label_stable_geometry_is_repeatable_at_fractional_dpi() {
        let glyph = test_glyph(0.25, 9.35, 7.2, 8.1);
        for dpi in [1.0, 1.25, 1.5, 1.75, 2.0] {
            let scale = 0.86 * dpi;
            let rect = crate::renderer::glyph_quad_rect(12.0, 80.0, glyph, scale);
            let first = crate::renderer::quad_vertices(
                rect.0, rect.1, rect.2, rect.3, 0.0, 0.0, 1.0, 1.0, [1.0; 4], 0.0,
            )
            .map(|vertex| vertex.pos);
            let second = crate::renderer::quad_vertices(
                rect.0, rect.1, rect.2, rect.3, 0.0, 0.0, 1.0, 1.0, [0.5; 4], 0.0,
            )
            .map(|vertex| vertex.pos);
            assert_eq!(first, second);
        }
    }

    #[test]
    fn spanned_ui_chars_keep_utf8_offsets_and_exact_span_colors() {
        let expected = [0.1, 0.2, 0.3, 1.0];
        let spans = vec![crate::highlighter::ColorSpan {
            start: 2,
            end: 6,
            color: expected,
        }];
        let mut seen = Vec::new();
        for_each_spanned_ui_char("xабy", &spans, Some(1), |ch, color| {
            seen.push((ch, color));
        });
        assert!(seen[0].1[0].is_nan());
        assert_eq!(seen[1], ('а', expected));
        assert_eq!(seen[2], ('б', expected));
        assert!(seen[3].1[0].is_nan());
    }

    #[test]
    fn spanned_ui_chars_skip_joiner_and_variation_without_losing_utf8_span_offsets() {
        let text = "a\u{200D}\u{FE0F}Ж";
        let zhe_start = text.find('Ж').expect("cyrillic glyph");
        let expected = [0.2, 0.7, 0.4, 1.0];
        let spans = [crate::highlighter::ColorSpan {
            start: zhe_start,
            end: zhe_start + 'Ж'.len_utf8(),
            color: expected,
        }];
        let mut emitted = Vec::new();
        let mut emitted_width = 0.0;
        for_each_spanned_ui_char(text, &spans, Some(0), |ch, color| {
            let advance = match ch {
                'a' => 8.0,
                'Ж' => 11.0,
                _ => panic!("non-rendering control reached glyph metrics: {ch:?}"),
            };
            emitted_width += advance;
            emitted.push((ch, color));
        });

        assert_eq!(emitted.len(), 2);
        assert_eq!(emitted[0].0, 'a');
        assert!(emitted[0].1[0].is_nan());
        assert_eq!(emitted[1], ('Ж', expected));
        assert_eq!(emitted_width, 19.0);
    }

    #[test]
    fn bug_1_database_sql_renderer_uses_shared_spanned_utf8_walk() {
        let expected = [0.1, 0.2, 0.3, 1.0];
        let spans = [crate::highlighter::ColorSpan {
            start: 7,
            end: 9,
            color: expected,
        }];
        let mut seen = Vec::new();
        for_each_spanned_ui_char("SELECT Ж", &spans, Some(0), |ch, color| seen.push((ch, color)));
        assert_eq!(seen.last(), Some(&('Ж', expected)));
    }

    #[test]
    fn bug_2_api_python_renderer_uses_shared_utf8_byte_offsets() {
        let expected = [0.9, 0.4, 0.2, 1.0];
        let spans = [crate::highlighter::ColorSpan {
            start: 2,
            end: 6,
            color: expected,
        }];
        let mut colored = Vec::new();
        for_each_spanned_ui_char("xабy", &spans, Some(1), |ch, color| {
            if !color[0].is_nan() {
                colored.push(ch);
            }
        });
        assert_eq!(colored, vec!['а', 'б']);
    }

    #[test]
    fn bug_3_inline_git_renderer_emits_one_callback_per_character() {
        let text = "a.Ж:b";
        let mut visited = String::new();
        for_each_spanned_ui_char(text, &[], None, |ch, _| visited.push(ch));
        assert_eq!(visited, text);
    }

    #[test]
    fn bug_4_punctuation_is_visited_once_without_duplicate_quad_workaround() {
        let mut chars = Vec::new();
        for_each_spanned_ui_char("a.:b", &[], None, |ch, _| chars.push(ch));
        assert_eq!(chars, vec!['a', '.', ':', 'b']);
        assert_eq!(chars.iter().filter(|&&ch| ch == '.').count(), 1);
        assert_eq!(chars.iter().filter(|&&ch| ch == ':').count(), 1);
    }

    #[test]
    fn wrapped_text_uses_breaks_and_keeps_unicode_boundaries() {
        let text = "Ошибка: очень длинное предупреждение";
        let lines = wrapped_text_ranges(text, 10.0, |_| 1.0);
        assert!(lines.len() > 1);
        assert!(lines.iter().all(|&(start, end)| {
            start <= end && text.is_char_boundary(start) && text.is_char_boundary(end)
        }));
        let mut rebuilt = String::new();
        let mut previous_end = 0usize;
        for &(start, end) in &lines {
            rebuilt.push_str(&text[previous_end..start]);
            rebuilt.push_str(&text[start..end]);
            previous_end = end;
        }
        rebuilt.push_str(&text[previous_end..]);
        assert_eq!(rebuilt, text);
    }

    #[test]
    fn folded_block_suffix_keeps_comma_before_inline_comment() {
        let editor = editor_with_text("exception_handlers={\n    Exception: handler,\n},  # ty\n");
        let (suffix, len) = folded_block_suffix(&editor, 0, 2);
        assert_eq!(len, 2);
        assert_eq!(&suffix[..2], &['}', ',']);
    }

    #[test]
    fn folded_block_suffix_keeps_plain_closer_and_comma() {
        let editor = editor_with_text("type_encoders={\n    Any: encoder,\n},\n");
        let (suffix, len) = folded_block_suffix(&editor, 0, 2);
        assert_eq!(len, 2);
        assert_eq!(&suffix[..2], &['}', ',']);
    }
