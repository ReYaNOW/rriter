// Stage-3 independent integration regressions. Included inside the existing
// Linux offscreen production Renderer harness; no transition or mapping logic
// is reimplemented here.

fn stage3_ranges_overlap(left: &std::ops::Range<usize>, right: &std::ops::Range<usize>) -> bool {
    left.start < right.end && right.start < left.end
}

fn stage3_install_normal_ide_tab(app: &mut App) {
    app.tabs.clear();
    app.tabs.push(crate::app::EditorTab {
        editor: crate::app::reviewer_stage2_editor_with(""),
        file_path: Some(std::path::PathBuf::from("/tmp/reviewer-stage3.md")),
        file_key: None,
        text_file_format: crate::platform::TextFileFormat::default(),
        base_title: "reviewer-stage3.md".to_string(),
        file_extension: "md".to_string(),
        markdown: Default::default(),
        scroll_y: crate::scroll::ScrollState::new(15.0),
        scroll_x: crate::scroll::ScrollState::new(15.0),
        spans: Vec::new(),
        completions: Vec::new(),
        foldable_ranges: Vec::new(),
        syntax_errors: Vec::new(),
        last_sent_version: 0,
        search_results: Vec::new(),
        search_current_idx: None,
        is_highlighted_once: false,
        is_highlight_complete: false,
        icon_key: "default_file",
        closing_hints: Default::default(),
        kind: crate::app::EditorTabKind::Normal,
    });
    app.active_tab = 0;
}

fn stage3_mixed_document() -> String {
    let mut source = String::new();
    for idx in 0..24 {
        source.push_str(&format!("lead-{idx:02} alpha beta gamma delta\n\n"));
    }
    source.push_str("# Large heading\n\n");
    source.push_str("## Smaller heading\n\n");
    source.push_str("- list alpha\n- list beta\n\n");
    source.push_str("```rust\nlet alpha = 1;\nlet beta = 2;\n```\n\n");
    source.push_str("first paragraph in section\n\n");
    source.push_str("second paragraph in section\n\n");
    source.push_str("third paragraph target stays source backed\n\n");
    for idx in 0..70 {
        source.push_str(&format!("tail-{idx:02} alpha beta gamma delta epsilon\n\n"));
    }
    source
}

#[test]
fn reviewer_stage3_mixed_blocks_track_third_paragraph_and_ignore_hidden_cursor() {
    let source = stage3_mixed_document();
    let target = source.find("third paragraph target").unwrap();
    let target_range = target..target + "third paragraph target".len();
    let (_context, mut app) = fixture(&source, 760.0, 1.25);
    app.renderer.as_mut().unwrap().height = 460.0;
    app.editor.cursor = 0;
    app.editor.selection_anchor = Some(1);

    let edit_line_y = app
        .renderer
        .as_mut()
        .unwrap()
        .markdown_edit_source_y(&app.editor, &target_range)
        .unwrap();
    let edit_start = edit_line_y + 3.25;
    app.scroll_y.jump_to(edit_start);
    review_v3_root_frame(&mut app);
    set_motion(&mut app, 67.0, 29.0);

    app.set_markdown_mode(MarkdownMode::Read);
    review_v3_root_frame(&mut app);
    let read_y = app.scroll_y.current;
    let read_anchor = app
        .markdown
        .read_layout
        .viewport_source_anchor(read_y)
        .expect("mixed Read viewport anchor");
    assert!(stage3_ranges_overlap(
        &read_anchor.source_range,
        &target_range
    ));
    assert!((app.scroll_y.target - app.scroll_y.current - 67.0).abs() <= 0.01);
    assert_eq!(app.scroll_y.velocity, 29.0);
    assert_eq!(app.scroll_y.anim_speed, 7.0);

    app.set_markdown_mode(MarkdownMode::Edit);
    review_v3_root_frame(&mut app);
    let returned = app.scroll_y.current;
    let edit_anchor = app
        .renderer
        .as_mut()
        .unwrap()
        .markdown_edit_viewport_anchor(&app.editor, returned)
        .expect("mixed Edit viewport anchor");
    let error = (returned - edit_start).abs();
    println!(
        "STAGE3_MIXED source={:?} edit_start={edit_start} read_y={read_y} returned={returned} error={error} current={} target={} velocity={} anim_speed={}",
        read_anchor.source_range,
        app.scroll_y.current,
        app.scroll_y.target,
        app.scroll_y.velocity,
        app.scroll_y.anim_speed
    );
    assert!(stage3_ranges_overlap(
        &edit_anchor.source_range,
        &target_range
    ));
    assert!(error <= 1.0);
    assert_eq!(app.editor.cursor, 0);
    assert_eq!(app.editor.selection_anchor, Some(1));
}

#[test]
fn reviewer_stage3_nested_quote_list_code_keeps_inner_line_and_motion() {
    let mut source = document();
    source.push_str("> - nested\n>\n>   ```rust\n>   let first = 1;\n>   target_code_line();\n>   let last = 2;\n>   ```\n\n");
    source.push_str(&document());
    let target = source.find("target_code_line").unwrap();
    let target_range = target..target + "target_code_line".len();
    let (_context, mut app) = fixture(&source, 520.0, 1.25);
    app.renderer.as_mut().unwrap().height = 420.0;

    let read_start = review_v3_root_read_at(&mut app, target, 4.25);
    let initial_anchor = app
        .markdown
        .read_layout
        .viewport_source_anchor(read_start)
        .expect("nested code Read anchor");
    assert!(stage3_ranges_overlap(
        &initial_anchor.source_range,
        &target_range
    ));
    set_motion(&mut app, -53.0, -31.0);

    app.set_markdown_mode(MarkdownMode::Edit);
    review_v3_root_frame(&mut app);
    let edit_y = app.scroll_y.current;
    let edit_anchor = app
        .renderer
        .as_mut()
        .unwrap()
        .markdown_edit_viewport_anchor(&app.editor, edit_y)
        .expect("nested code Edit anchor");
    assert!(stage3_ranges_overlap(
        &edit_anchor.source_range,
        &target_range
    ));

    app.set_markdown_mode(MarkdownMode::Read);
    review_v3_root_frame(&mut app);
    let returned = app.scroll_y.current;
    let error = (returned - read_start).abs();
    println!(
        "STAGE3_CODE source={:?} read_start={read_start} edit_y={edit_y} returned={returned} error={error} residual={} velocity={} anim_speed={}",
        initial_anchor.source_range,
        app.scroll_y.target - app.scroll_y.current,
        app.scroll_y.velocity,
        app.scroll_y.anim_speed
    );
    assert!(error <= 1.0);
    assert!((app.scroll_y.target - app.scroll_y.current + 53.0).abs() <= 0.01);
    assert_eq!(app.scroll_y.velocity, -31.0);
    assert_eq!(app.scroll_y.anim_speed, 7.0);
}

#[test]
fn reviewer_stage3_wrapped_table_cell_carry_restores_deep_fragment() {
    let mut source = document();
    source.push_str("| left | middle | right |\n| --- | --- | --- |\n| ");
    source.push_str(&"alpha beta gamma delta epsilon zeta eta theta ".repeat(18));
    source.push_str("deep_target omega sigma tau | center | right |\n\n");
    source.push_str(&document());
    let row_start = source.find("alpha beta gamma").unwrap();
    let target = source.find("deep_target").unwrap();
    let target_range = target..target + "deep_target".len();
    let (_context, mut app) = fixture(&source, 340.0, 1.0);
    app.renderer.as_mut().unwrap().height = 420.0;

    app.set_markdown_mode(MarkdownMode::Read);
    review_v3_root_frame(&mut app);
    let row_y = app
        .markdown
        .read_layout
        .source_anchor_y(&(row_start..row_start + 5))
        .unwrap();
    let deep_y = app
        .markdown
        .read_layout
        .source_anchor_y(&target_range)
        .unwrap();
    assert!(
        deep_y - row_y > 40.0,
        "fixture must reach a deep wrapped cell line"
    );
    let read_start = deep_y + 3.75;
    app.scroll_y.jump_to(read_start);
    review_v3_root_frame(&mut app);
    set_motion(&mut app, 41.0, 17.0);

    app.set_markdown_mode(MarkdownMode::Edit);
    review_v3_root_frame(&mut app);
    let edit_y = app.scroll_y.current;
    let carry = app
        .markdown
        .scroll_carry
        .as_ref()
        .expect("lossy table projection keeps carry")
        .anchor
        .clone();
    assert!(stage3_ranges_overlap(&carry.source_range, &target_range));

    app.set_markdown_mode(MarkdownMode::Read);
    review_v3_root_frame(&mut app);
    let returned = app.scroll_y.current;
    let error = (returned - read_start).abs();
    println!(
        "STAGE3_TABLE source={:?} row_y={row_y} deep_y={deep_y} edit_y={edit_y} returned={returned} error={error} residual={} velocity={} anim_speed={}",
        carry.source_range,
        app.scroll_y.target - app.scroll_y.current,
        app.scroll_y.velocity,
        app.scroll_y.anim_speed
    );
    assert!(error <= 1.0);
    assert!((app.scroll_y.target - app.scroll_y.current - 41.0).abs() <= 0.01);
    assert_eq!(app.scroll_y.velocity, 17.0);
    assert_eq!(app.scroll_y.anim_speed, 7.0);
}

#[test]
fn reviewer_stage3_edit_revision_discards_old_carry_and_uses_live_viewport() {
    let source = document();
    let target = source.find("paragraph030").unwrap() + 150;
    let (_context, mut app) = fixture(&source, 520.0, 1.25);
    app.renderer.as_mut().unwrap().height = 420.0;

    let read_start = review_v3_root_read_at(&mut app, target, 3.25);
    app.set_markdown_mode(MarkdownMode::Edit);
    review_v3_root_frame(&mut app);
    let old_carry = app
        .markdown
        .scroll_carry
        .as_ref()
        .expect("wrapped paragraph carry")
        .clone();
    let old_version = app.editor.version;
    let before_edit = app
        .renderer
        .as_mut()
        .unwrap()
        .markdown_edit_viewport_anchor(&app.editor, app.scroll_y.current)
        .unwrap();

    app.editor.cursor = before_edit.source_range.start;
    let _ = app
        .editor
        .insert_str("inserted-one\ninserted-two\ninserted-three\n");
    assert_ne!(app.editor.version, old_version);
    let live_after_edit = app
        .renderer
        .as_mut()
        .unwrap()
        .markdown_edit_viewport_anchor(&app.editor, app.scroll_y.current)
        .expect("live viewport after revision");
    assert_ne!(live_after_edit.source_range, old_carry.anchor.source_range);

    app.set_markdown_mode(MarkdownMode::Read);
    let captured = app
        .markdown
        .scroll_transition
        .as_ref()
        .and_then(|transition| transition.anchor.clone())
        .expect("fresh transition anchor");
    assert_eq!(captured.source_range, live_after_edit.source_range);
    assert!(app.markdown.scroll_carry.is_none());
    review_v3_root_frame(&mut app);
    let expected = captured.projected_scroll_y(
        app.markdown
            .read_layout
            .source_anchor_y(&captured.source_range)
            .unwrap(),
    );
    let error = (app.scroll_y.current - expected).abs();
    println!(
        "STAGE3_REVISION old_version={old_version} new_version={} old_carry={:?} live={:?} read_start={read_start} expected={expected} actual={} error={error}",
        app.editor.version,
        old_carry.anchor.source_range,
        live_after_edit.source_range,
        app.scroll_y.current
    );
    assert!(error <= 1.0);
    assert_eq!(app.markdown.read_model_version, Some(app.editor.version));
}

#[test]
fn reviewer_stage3_dpi_sidebar_bottom_panel_geometry_round_trips() {
    let source = document();
    let target = source.find("paragraph050").unwrap();
    let target_range = target..target + "paragraph050".len();
    let (_context, mut app) = fixture(&source, 920.0, 1.0);
    app.renderer.as_mut().unwrap().height = 640.0;
    app.is_ide_mode = true;
    stage3_install_normal_ide_tab(&mut app);
    app.ide_panel.open(crate::app::PanelId::Explorer);
    app.ide_panel.left_width = 170.0;
    app.ide_panel.bottom_height = 105.0;
    if let Some(slot) = app
        .ide_panel
        .slots
        .iter_mut()
        .find(|slot| slot.id == crate::app::PanelId::LspServers)
    {
        slot.group = crate::app::PanelGroup::Bottom;
        slot.open = true;
    }

    for (index, scale) in [1.0_f32, 1.25, 1.5, 1.75, 2.0].into_iter().enumerate() {
        let renderer = app.renderer.as_mut().unwrap();
        renderer.update_scale_factor(scale);
        renderer.width = 920.0 - index as f32 * 35.0;
        review_v3_root_frame(&mut app);
        let edit_line_y = app
            .renderer
            .as_mut()
            .unwrap()
            .markdown_edit_source_y(&app.editor, &target_range)
            .unwrap();
        let edit_start = edit_line_y + 3.25;
        app.scroll_y.jump_to(edit_start);
        review_v3_root_frame(&mut app);
        let rebuilds_before = app.markdown.read_layout.rebuild_count();

        app.set_markdown_mode(MarkdownMode::Read);
        review_v3_root_frame(&mut app);
        let read_anchor = app
            .markdown
            .read_layout
            .viewport_source_anchor(app.scroll_y.current)
            .expect("scaled Read anchor");
        assert!(stage3_ranges_overlap(
            &read_anchor.source_range,
            &target_range
        ));
        let rebuilds_after = app.markdown.read_layout.rebuild_count();
        assert!(rebuilds_after > rebuilds_before);

        app.set_markdown_mode(MarkdownMode::Edit);
        review_v3_root_frame(&mut app);
        let error = (app.scroll_y.current - edit_start).abs();
        println!(
            "STAGE3_GEOMETRY scale={scale} font={} width={} left={} bottom={} edit_start={edit_start} read_y={} returned={} error={error} rebuilds={rebuilds_before}->{rebuilds_after}",
            app.renderer.as_ref().unwrap().font_size,
            app.renderer.as_ref().unwrap().width,
            app.ide_panel.visible_left_width(scale),
            app.ide_panel.editor_reserved_bottom_height(scale),
            read_anchor.projected_scroll_y(
                app.markdown
                    .read_layout
                    .source_anchor_y(&read_anchor.source_range)
                    .unwrap()
            ),
            app.scroll_y.current
        );
        assert!(error <= 1.0);
    }
}

#[test]
fn reviewer_stage3_empty_markup_and_eof_states_stay_finite_and_clamped() {
    for source in ["", "\n\n\n", "<!-- only markup -->\n", "---\n"] {
        let (_context, mut app) = fixture(source, 520.0, 1.0);
        app.renderer.as_mut().unwrap().height = 420.0;
        app.scroll_y.current = 93.0;
        app.scroll_y.target = 141.0;
        app.scroll_y.velocity = 21.0;
        app.scroll_y.anim_speed = 7.0;
        app.set_markdown_mode(MarkdownMode::Read);
        review_v3_root_frame(&mut app);
        let max_scroll = app.markdown.read_scroll_bounds().unwrap_or(0.0);
        println!(
            "STAGE3_EMPTY len={} max={max_scroll} current={} target={} velocity={} anim_speed={}",
            source.len(),
            app.scroll_y.current,
            app.scroll_y.target,
            app.scroll_y.velocity,
            app.scroll_y.anim_speed
        );
        assert!(max_scroll.is_finite() && max_scroll >= 0.0);
        assert!(app.scroll_y.current.is_finite() && app.scroll_y.current >= 0.0);
        assert!(app.scroll_y.target.is_finite() && app.scroll_y.target >= 0.0);
        assert!(app.scroll_y.current <= max_scroll + 0.01);
        assert!(app.scroll_y.target <= max_scroll + 0.01);
        app.set_markdown_mode(MarkdownMode::Edit);
        review_v3_root_frame(&mut app);
        assert!(app.scroll_y.current.is_finite() && app.scroll_y.current >= 0.0);
        assert!(app.scroll_y.target.is_finite() && app.scroll_y.target >= 0.0);
    }

    let mut no_newline = document();
    no_newline.push_str("final-tail-without-newline");
    let (_context, mut app) = fixture(&no_newline, 520.0, 1.0);
    app.renderer.as_mut().unwrap().height = 420.0;
    app.set_markdown_mode(MarkdownMode::Read);
    review_v3_root_frame(&mut app);
    let eof = no_newline.len()..no_newline.len();
    let eof_y = app
        .markdown
        .read_layout
        .source_anchor_y(&eof)
        .expect("EOF without newline maps to Reader geometry");
    let max_scroll = app.markdown.read_scroll_bounds().unwrap();
    app.scroll_y.jump_to(eof_y.min(max_scroll));
    review_v3_root_frame(&mut app);
    println!(
        "STAGE3_EOF source={:?} y={eof_y} max={max_scroll} current={} target={}",
        eof, app.scroll_y.current, app.scroll_y.target
    );
    assert!(eof_y.is_finite());
    assert!(app.scroll_y.current.is_finite());
    assert!(app.scroll_y.current >= 0.0 && app.scroll_y.current <= max_scroll + 0.01);
}

#[test]
fn reviewer_stage3_ten_thousand_block_source_index_reuses_built_cache() {
    let mut source = String::new();
    for index in 0..10_000 {
        source.push_str(&format!("block-{index:05} alpha beta gamma\n\n"));
    }
    let cache = crate::render_view::markdown_read::build_test_markdown_read_layout(&source, 420.0);
    let rebuilds = cache.rebuild_count();
    let mut ys = Vec::new();
    for index in [0, 5_000, 9_999] {
        let needle = format!("block-{index:05}");
        let byte = source.find(&needle).unwrap();
        let range = byte..byte + needle.len();
        let y = cache
            .source_anchor_y(&range)
            .expect("large block source lookup");
        let anchor = cache
            .viewport_source_anchor(y + 2.5)
            .expect("large block viewport lookup");
        let projected = cache
            .source_anchor_y(&anchor.source_range)
            .map(|line_y| anchor.projected_scroll_y(line_y))
            .unwrap();
        assert!((projected - (y + 2.5)).abs() <= 1.0);
        ys.push(y);
    }
    let tail = source.find("block-09999").unwrap();
    for _ in 0..100 {
        assert!(cache.source_anchor_y(&(tail..tail + 11)).is_some());
    }
    println!(
        "STAGE3_10K rebuilds={rebuilds} y_start={} y_mid={} y_end={} repeated_lookups=100",
        ys[0], ys[1], ys[2]
    );
    assert_eq!(cache.rebuild_count(), rebuilds);
    assert!(ys[0] < ys[1] && ys[1] < ys[2]);
}

fn stage4_mixed_document() -> String {
    let mut source = stage3_mixed_document();
    source.push_str("### H3 third heading\n\n#### H4 fourth heading\n\n##### H5 fifth heading\n\n###### H6 sixth heading\n\n");
    source.push_str("Latin Кириллица e\u{301} emoji 😀 inline `code` and [linked label](https://example.test/path). ");
    source.push_str(&"wrapped words alpha beta gamma delta epsilon ".repeat(18));
    source.push_str("\n\n> outer quote\n> - nested item\n>   - child item with **bold** and `mono`\n>\n> quoted tail\n\n");
    source.push_str("```rust\n");
    for index in 0..160 {
        source.push_str(&format!(
            "fn fenced_{index:03}() {{ println!(\"строка 😀 {index}\"); }}\n"
        ));
    }
    source.push_str("```\n\n");
    source.push_str("| left aligned | centered | right aligned |\n| --- | :---: | ---: |\n");
    source.push_str("| short | center | 42 |\n");
    source.push_str("| ");
    source.push_str(&"table wrapped latin кириллица 😀 ".repeat(18));
    source.push_str(" | centered wrapped value value value value | 123456789 |\n\n\n");
    source.push_str("final paragraph without trailing newline");
    source
}

#[test]
fn reviewer_final_reverse_autoscroll_uses_displayed_code_table_rows_and_root_toggle() {
    let source = stage4_mixed_document();
    for scale in [1.25, 2.0] {
        for needle in ["fenced_080", "table wrapped latin"] {
            let (_context, mut app) = fixture(&source, 900.0 * scale, scale);
            app.renderer.as_mut().unwrap().height = 500.0 * scale;
            app.is_ide_mode = true;
            stage3_install_normal_ide_tab(&mut app);
            app.set_markdown_mode(MarkdownMode::Read);
            review_v3_root_frame(&mut app);
            let body = app
                .ui_registry
                .rect_for(crate::ui_system::UiId::MarkdownReadBody)
                .unwrap();
            let byte = source.find(needle).unwrap();
            let line_y = app
                .markdown
                .read_layout
                .source_anchor_y(&(byte..byte + needle.len()))
                .unwrap();
            let max_scroll = app.markdown.read_scroll_bounds().unwrap();
            app.scroll_y
                .jump_to((line_y - body.3 * 0.5 + 0.37).clamp(0.0, max_scroll));
            review_v3_root_frame(&mut app);
            let body = app
                .ui_registry
                .rect_for(crate::ui_system::UiId::MarkdownReadBody)
                .unwrap();
            let hidden_editor = (
                app.editor.cursor,
                app.editor.selection_anchor,
                app.editor.version,
            );
            let x = body.0 + 100.0 * scale;
            assert!(app.begin_markdown_read_selection_at(x, body.1 + body.3 * 0.5));
            let anchor = app.markdown.read_selection_anchor;
            let start_scroll = app.scroll_y.current;
            let top_y = body.1 + 1.0;
            app.renderer.as_mut().unwrap().last_mouse_x = x;
            app.renderer.as_mut().unwrap().last_mouse_y = top_y;
            assert!(app.reviewer_tick_markdown_read_selection_autoscroll(0.016, false));
            let mut distinguished_target = false;
            for _ in 0..24 {
                let moved = app.scroll_y.update(0.016);
                let _ = app.reviewer_tick_markdown_read_selection_autoscroll(0.016, moved);
                let renderer = app.renderer.as_mut().unwrap();
                let displayed = renderer
                    .markdown_read_source_byte_at(
                        &app.markdown,
                        app.editor.version,
                        body,
                        app.scroll_y.current,
                        x,
                        top_y,
                    )
                    .unwrap();
                let target = renderer
                    .markdown_read_source_byte_at(
                        &app.markdown,
                        app.editor.version,
                        body,
                        app.scroll_y.target,
                        x,
                        top_y,
                    )
                    .unwrap();
                distinguished_target |= displayed != target;
                assert_eq!(app.markdown.read_selection_cursor, Some(displayed));
                assert_eq!(app.markdown.read_selection_anchor, anchor);
                assert!(source.is_char_boundary(displayed));
                assert!(app.scroll_y.target <= app.scroll_y.current);
            }
            assert!(
                distinguished_target,
                "fixture must distinguish displayed and target rows"
            );
            assert!(app.scroll_y.current < start_scroll);
            assert!(app.markdown.read_selection_cursor < anchor);
            let selected = app.markdown.read_selection_range().unwrap();
            review_v3_root_frame(&mut app);
            // Use the toggle registered by the complete IDE frame, without
            // clearing/replacing its registry with an isolated status-bar draw.
            let toggle = app
                .ui_registry
                .rect_for(crate::ui_system::UiId::MarkdownModeToggle)
                .unwrap();
            let x = toggle.0 + toggle.2 * 0.5;
            let y = toggle.1 + toggle.3 * 0.5;
            let hit = app.ui_registry.find_at(x, y).unwrap();
            assert_eq!(hit, crate::ui_system::UiId::MarkdownModeToggle);
            // The IDE mouse entry needs a native Window for resize handling.
            // Dispatch the real registry hit through its production UI action.
            app.handle_ui_click(hit);
            assert_eq!(app.markdown_mode(), MarkdownMode::Edit);
            assert!(!app.markdown.read_selecting);
            assert!(!app.markdown.read_selection_autoscrolling);
            assert_eq!(app.scroll_y.current, app.scroll_y.target);
            assert_eq!(app.scroll_y.velocity, 0.0);
            assert_eq!(app.markdown.read_selection_range(), Some(selected.clone()));
            assert_eq!(
                (
                    app.editor.cursor,
                    app.editor.selection_anchor,
                    app.editor.version
                ),
                hidden_editor
            );
            println!(
                "REVIEW_FINAL_REVERSE dpi={scale} block={needle:?} selected={selected:?} current={} target={} distinguished_target={distinguished_target}",
                app.scroll_y.current, app.scroll_y.target
            );
        }
    }
}

#[test]
fn reviewer_stage4_mixed_markdown_fixture_is_source_stable_across_dpi_and_widths() {
    let source = stage4_mixed_document();
    let probes = [
        "Large heading",
        "Smaller heading",
        "H3 third heading",
        "H4 fourth heading",
        "H5 fifth heading",
        "H6 sixth heading",
        "Latin Кириллица e\u{301} emoji 😀",
        "linked label",
        "nested item",
        "child item",
        "fenced_080",
        "table wrapped latin кириллица 😀",
        "centered wrapped value",
        "final paragraph without trailing newline",
    ];

    for scale in [1.0, 1.25, 1.5, 2.0] {
        for css_width in [360.0, 900.0] {
            let (_context, mut app) = fixture(&source, css_width * scale, scale);
            app.renderer.as_mut().unwrap().height = 420.0 * scale;
            app.set_markdown_mode(MarkdownMode::Read);
            review_v3_root_frame(&mut app);

            let (document_source_len, block_count) = {
                let document = app
                    .markdown
                    .read_document(app.editor.version)
                    .expect("mixed Markdown document");
                (document.source_len, document.blocks.len())
            };
            assert_eq!(document_source_len, source.len());
            assert!(block_count >= 35, "unexpectedly small mixed parse");
            let rebuilds = app.markdown.read_layout.rebuild_count();
            let content_height = app.markdown.read_layout.content_height();
            let max_scroll = app.markdown.read_scroll_bounds().expect("Reader bounds");
            assert!(content_height.is_finite() && content_height > 0.0);
            assert!(max_scroll.is_finite() && max_scroll > 0.0);
            assert!(
                app.ui_registry
                    .rect_for(crate::ui_system::UiId::MarkdownReadScrollbar)
                    .is_some()
            );

            for needle in probes {
                let start = source.find(needle).expect("mixed probe");
                let range = start..start + needle.len();
                assert!(source.is_char_boundary(range.start));
                assert!(source.is_char_boundary(range.end));
                let y = app
                    .markdown
                    .read_layout
                    .source_anchor_y(&range)
                    .unwrap_or_else(|| panic!("no Reader Y for {needle:?}"));
                let anchor = app
                    .markdown
                    .read_layout
                    .viewport_source_anchor(y + 2.25)
                    .unwrap_or_else(|| panic!("no viewport anchor for {needle:?}"));
                let reverse_y = app
                    .markdown
                    .read_layout
                    .source_anchor_y(&anchor.source_range)
                    .expect("reverse Reader source Y");
                assert!(
                    (reverse_y - y).abs() <= 0.01,
                    "dpi={scale} width={css_width} probe={needle:?} anchor={:?} range={range:?} y={y} reverse_y={reverse_y}",
                    anchor.source_range
                );
                let projected = anchor.projected_scroll_y(reverse_y);
                assert!(
                    (projected - (y + 2.25)).abs() <= 1.0,
                    "dpi={scale} width={css_width} probe={needle:?} projected={projected} expected={}",
                    y + 2.25
                );
            }

            let eof = source.len()..source.len();
            let eof_y = app
                .markdown
                .read_layout
                .source_anchor_y(&eof)
                .expect("EOF must remain source-backed");
            assert!(eof_y.is_finite());
            review_v3_root_frame(&mut app);
            assert_eq!(
                app.markdown.read_layout.rebuild_count(),
                rebuilds,
                "identical warmed root draw must reuse the mixed Reader layout"
            );
            println!(
                "STAGE4_MIXED dpi={scale} css_width={css_width} blocks={} content_height={content_height} max_scroll={max_scroll} rebuilds={rebuilds} eof_y={eof_y}",
                block_count
            );
        }
    }
}

fn stage4_status_toggle_rect(app: &mut App) -> (f32, f32, f32, f32) {
    let path = app
        .file_path
        .clone()
        .unwrap_or_else(|| std::path::PathBuf::from("/tmp/reviewer-stage4.md"));
    let mode = app.markdown_mode();
    app.ui_registry.clear();
    let renderer = app.renderer.as_mut().expect("renderer");
    renderer.draw_status_bar(
        &app.editor,
        Some((&path, crate::platform::TextEncoding::Utf8)),
        mode,
        None,
        &mut app.ui_registry,
        renderer.scale_factor,
        -1.0,
        -1.0,
        0.0,
        None,
        None,
        None,
    );
    renderer.flush();
    app.ui_registry
        .rect_for(crate::ui_system::UiId::MarkdownModeToggle)
        .expect("production Markdown status toggle")
}

#[test]
fn reviewer_stage4_full_gesture_resize_toggle_sequence_preserves_source_and_motion() {
    let mut source = String::new();
    for i in 0..260 {
        source.push_str(&format!(
            "paragraph {i:03} alpha beta gamma delta epsilon кириллица 😀 `code`\n\n"
        ));
    }
    let target_start = source.find("paragraph 120").unwrap();
    let target_range = target_start..target_start + "paragraph 120".len();
    let (_context, mut app) = fixture(&source, 1125.0, 1.25);
    app.renderer.as_mut().unwrap().height = 650.0;
    app.editor.cursor = target_start;
    app.editor.selection_anchor = Some(target_start + 3);
    let hidden_editor = (
        app.editor.cursor,
        app.editor.selection_anchor,
        app.editor.version,
        app.editor.get_full_text().to_string(),
    );

    let edit_y = app
        .renderer
        .as_mut()
        .unwrap()
        .markdown_edit_source_y(&app.editor, &target_range)
        .expect("target Edit Y");
    app.scroll_y.jump_to(edit_y + 3.25);
    app.scroll_y.target = app.scroll_y.current + 44.5;
    app.scroll_y.velocity = 18.0;
    app.scroll_y.anim_speed = 7.0;
    review_v3_root_frame(&mut app);
    let initial_edit_anchor = app
        .renderer
        .as_mut()
        .unwrap()
        .markdown_edit_viewport_anchor(&app.editor, app.scroll_y.current)
        .expect("initial Edit anchor");
    assert!(stage3_ranges_overlap(
        &initial_edit_anchor.source_range,
        &target_range
    ));

    let first_toggle = stage4_status_toggle_rect(&mut app);
    app.renderer.as_mut().unwrap().last_mouse_x = first_toggle.0 + first_toggle.2 * 0.5;
    app.renderer.as_mut().unwrap().last_mouse_y = first_toggle.1 + first_toggle.3 * 0.5;
    app.reviewer_markdown_read_mouse_input(
        winit::event::ElementState::Pressed,
        winit::event::MouseButton::Left,
    );
    assert_eq!(app.markdown_mode(), MarkdownMode::Read);
    assert_eq!(app.scroll_y.current, edit_y + 3.25);
    assert_eq!(app.scroll_y.target, edit_y + 47.75);
    assert_eq!(app.scroll_y.velocity, 18.0);
    assert_eq!(app.scroll_y.anim_speed, 7.0);
    review_v3_root_frame(&mut app);

    let first_read_anchor = app
        .markdown
        .read_layout
        .viewport_source_anchor(app.scroll_y.current)
        .expect("first Reader anchor");
    assert!(stage3_ranges_overlap(
        &first_read_anchor.source_range,
        &target_range
    ));
    assert!(
        (first_read_anchor.viewport_offset_y - initial_edit_anchor.viewport_offset_y).abs() <= 1.0
    );

    let body = app
        .ui_registry
        .rect_for(crate::ui_system::UiId::MarkdownReadBody)
        .expect("Reader body");
    let x = body.0 + 110.0;
    let mut began = false;
    for fraction in [0.30, 0.45, 0.60] {
        if app.begin_markdown_read_selection_at(x, body.1 + body.3 * fraction) {
            began = true;
            break;
        }
    }
    assert!(began, "selection must start on visible Reader text");
    app.renderer.as_mut().unwrap().last_mouse_x = x;
    app.renderer.as_mut().unwrap().last_mouse_y = body.1 + body.3 - 2.0;
    let _ = app.update_markdown_read_selection_at(x, body.1 + body.3 - 2.0);
    assert!(app.reviewer_tick_markdown_read_selection_autoscroll(0.016, false));
    for _ in 0..16 {
        let moved = app.scroll_y.update(0.016);
        let _ = app.reviewer_tick_markdown_read_selection_autoscroll(0.016, moved);
    }
    assert!(app.finish_markdown_read_selection_gesture());
    assert!(!app.markdown.read_selecting);
    assert!(!app.markdown.read_selection_autoscrolling);
    let selection = app
        .markdown
        .read_selection_range()
        .expect("Reader selection");
    assert!(selection.start < selection.end);
    assert!(source.is_char_boundary(selection.start));
    assert!(source.is_char_boundary(selection.end));
    assert_eq!(
        (
            app.editor.cursor,
            app.editor.selection_anchor,
            app.editor.version,
            app.editor.get_full_text().to_string(),
        ),
        hidden_editor
    );

    review_v3_root_frame(&mut app);
    let body = app
        .ui_registry
        .rect_for(crate::ui_system::UiId::MarkdownReadBody)
        .unwrap();
    let scrollbar = app
        .ui_registry
        .rect_for(crate::ui_system::UiId::MarkdownReadScrollbar)
        .expect("Reader scrollbar");
    let thumb = crate::render_view::markdown_read::markdown_read_scrollbar_thumb(
        body.1,
        body.3,
        app.markdown.read_layout.content_height(),
        app.scroll_y.current.round(),
        1.25,
    )
    .unwrap();
    app.renderer.as_mut().unwrap().last_mouse_x = scrollbar.0 + scrollbar.2 * 0.5;
    app.renderer.as_mut().unwrap().last_mouse_y = thumb.start + thumb.len * 0.5;
    app.reviewer_markdown_read_mouse_input(
        winit::event::ElementState::Pressed,
        winit::event::MouseButton::Left,
    );
    assert!(app.scroll_y.is_dragging);
    assert!(app.drag_markdown_read_scrollbar_to(body.1 + body.3 * 0.72));
    app.reviewer_markdown_read_mouse_input(
        winit::event::ElementState::Released,
        winit::event::MouseButton::Left,
    );
    assert!(!app.scroll_y.is_dragging);
    assert_eq!(app.scroll_y.current, app.scroll_y.target);
    assert_eq!(app.scroll_y.velocity, 0.0);
    assert_eq!(app.scroll_y.anim_speed, 7.0);
    review_v3_root_frame(&mut app);

    let before_resize_anchor = app
        .markdown
        .read_layout
        .viewport_source_anchor(app.scroll_y.current)
        .unwrap();
    let before_resize_toggle = stage4_status_toggle_rect(&mut app);
    app.renderer.as_mut().unwrap().width = 900.0;
    review_v3_root_frame(&mut app);
    let after_resize_anchor = app
        .markdown
        .read_layout
        .viewport_source_anchor(app.scroll_y.current)
        .unwrap();
    assert!(stage3_ranges_overlap(
        &before_resize_anchor.source_range,
        &after_resize_anchor.source_range
    ));
    assert!(
        (before_resize_anchor.viewport_offset_y - after_resize_anchor.viewport_offset_y).abs()
            <= 1.0
    );
    let resized_toggle = stage4_status_toggle_rect(&mut app);
    assert_ne!(before_resize_toggle.0, resized_toggle.0);

    let max_scroll = app.markdown.read_scroll_bounds().unwrap();
    let residual = 38.5_f32.min((max_scroll - app.scroll_y.current).max(0.0));
    assert!(residual > 1.0);
    app.scroll_y.target = app.scroll_y.current + residual;
    app.scroll_y.velocity = 13.0;
    let stable_read_anchor = app
        .markdown
        .read_layout
        .viewport_source_anchor(app.scroll_y.current)
        .unwrap();
    app.renderer.as_mut().unwrap().last_mouse_x = resized_toggle.0 + resized_toggle.2 * 0.5;
    app.renderer.as_mut().unwrap().last_mouse_y = resized_toggle.1 + resized_toggle.3 * 0.5;
    app.reviewer_markdown_read_mouse_input(
        winit::event::ElementState::Pressed,
        winit::event::MouseButton::Left,
    );
    assert_eq!(app.markdown_mode(), MarkdownMode::Edit);
    assert!((app.scroll_y.target - app.scroll_y.current - residual).abs() <= 0.01);
    assert_eq!(app.scroll_y.velocity, 13.0);
    review_v3_root_frame(&mut app);
    let first_edit_after_resize = app
        .renderer
        .as_mut()
        .unwrap()
        .markdown_edit_viewport_anchor(&app.editor, app.scroll_y.current)
        .unwrap();
    assert!(stage3_ranges_overlap(
        &stable_read_anchor.source_range,
        &first_edit_after_resize.source_range
    ));
    assert!(
        (stable_read_anchor.viewport_offset_y - first_edit_after_resize.viewport_offset_y).abs()
            <= 1.0
    );

    let toggle = stage4_status_toggle_rect(&mut app);
    app.renderer.as_mut().unwrap().last_mouse_x = toggle.0 + toggle.2 * 0.5;
    app.renderer.as_mut().unwrap().last_mouse_y = toggle.1 + toggle.3 * 0.5;
    app.reviewer_markdown_read_mouse_input(
        winit::event::ElementState::Pressed,
        winit::event::MouseButton::Left,
    );
    assert_eq!(app.markdown_mode(), MarkdownMode::Read);
    review_v3_root_frame(&mut app);
    let returned_read_anchor = app
        .markdown
        .read_layout
        .viewport_source_anchor(app.scroll_y.current)
        .unwrap();
    assert!(stage3_ranges_overlap(
        &stable_read_anchor.source_range,
        &returned_read_anchor.source_range
    ));
    assert!(
        (stable_read_anchor.viewport_offset_y - returned_read_anchor.viewport_offset_y).abs()
            <= 1.0
    );

    for _ in 0..20 {
        let toggle = stage4_status_toggle_rect(&mut app);
        app.renderer.as_mut().unwrap().last_mouse_x = toggle.0 + toggle.2 * 0.5;
        app.renderer.as_mut().unwrap().last_mouse_y = toggle.1 + toggle.3 * 0.5;
        app.reviewer_markdown_read_mouse_input(
            winit::event::ElementState::Pressed,
            winit::event::MouseButton::Left,
        );
        assert!((app.scroll_y.target - app.scroll_y.current - residual).abs() <= 0.02);
        assert_eq!(app.scroll_y.velocity, 13.0);
        assert_eq!(app.scroll_y.anim_speed, 7.0);
        review_v3_root_frame(&mut app);
        let anchor = match app.markdown_mode() {
            MarkdownMode::Read => app
                .markdown
                .read_layout
                .viewport_source_anchor(app.scroll_y.current)
                .unwrap(),
            MarkdownMode::Edit => app
                .renderer
                .as_mut()
                .unwrap()
                .markdown_edit_viewport_anchor(&app.editor, app.scroll_y.current)
                .unwrap(),
        };
        assert!(stage3_ranges_overlap(
            &stable_read_anchor.source_range,
            &anchor.source_range
        ));
        assert!((stable_read_anchor.viewport_offset_y - anchor.viewport_offset_y).abs() <= 1.0);
    }
    assert_eq!(app.markdown_mode(), MarkdownMode::Read);
    assert_eq!(
        (
            app.editor.cursor,
            app.editor.selection_anchor,
            app.editor.version,
            app.editor.get_full_text().to_string(),
        ),
        hidden_editor
    );
    let max_scroll = app.markdown.read_scroll_bounds().unwrap();
    assert!(app.scroll_y.current >= 0.0 && app.scroll_y.current <= max_scroll + 0.01);
    assert!(app.scroll_y.target >= 0.0 && app.scroll_y.target <= max_scroll + 0.01);
    println!(
        "STAGE4_SEQUENCE selection={selection:?} stable_source={:?} viewport_offset={} current={} target={} velocity={} anim_speed={} max_scroll={max_scroll}",
        stable_read_anchor.source_range,
        stable_read_anchor.viewport_offset_y,
        app.scroll_y.current,
        app.scroll_y.target,
        app.scroll_y.velocity,
        app.scroll_y.anim_speed
    );
}

#[test]
fn reviewer_stage4_cold_home_end_and_consumed_font_rebase_use_live_geometry() {
    let source = document();

    for end_navigation in [false, true] {
        let (_context, mut app) = fixture(&source, 720.0, 1.0);
        app.renderer.as_mut().unwrap().height = 420.0;
        let target = source.find("paragraph030").unwrap();
        let target_range = target..target + "paragraph030".len();
        let edit_y = app
            .renderer
            .as_mut()
            .unwrap()
            .markdown_edit_source_y(&app.editor, &target_range)
            .unwrap();
        app.scroll_y.jump_to(edit_y + 3.25);
        let edit_anchor = app
            .renderer
            .as_mut()
            .unwrap()
            .markdown_edit_viewport_anchor(&app.editor, app.scroll_y.current)
            .unwrap();
        assert_eq!(app.markdown.read_layout.rebuild_count(), 0);
        app.set_markdown_mode(MarkdownMode::Read);
        assert!(app.prepare_markdown_absolute_scroll_target_navigation());
        let max_scroll = app.markdown.read_scroll_bounds().unwrap();
        if end_navigation {
            app.markdown.mark_absolute_scroll_end_navigation();
            app.scroll_y.animate_to(max_scroll);
            app.markdown
                .remember_pending_absolute_scroll_target_y(max_scroll);
        } else {
            app.markdown.mark_absolute_scroll_start_navigation();
            app.scroll_y.animate_to(0.0);
            app.markdown.remember_pending_absolute_scroll_target_y(0.0);
        }
        review_v3_root_frame(&mut app);
        let read_anchor = app
            .markdown
            .read_layout
            .viewport_source_anchor(app.scroll_y.current)
            .unwrap();
        assert!(stage3_ranges_overlap(
            &edit_anchor.source_range,
            &read_anchor.source_range
        ));
        let expected_target = if end_navigation { max_scroll } else { 0.0 };
        assert!((app.scroll_y.target - expected_target).abs() <= 1.0);
        println!(
            "STAGE4_COLD_NAV kind={} source={:?} current={} target={} max_scroll={max_scroll} rebuilds={}",
            if end_navigation { "End" } else { "Home" },
            read_anchor.source_range,
            app.scroll_y.current,
            app.scroll_y.target,
            app.markdown.read_layout.rebuild_count()
        );
    }

    let (_context, mut app) = fixture(&source, 720.0, 1.0);
    app.renderer.as_mut().unwrap().height = 420.0;
    let live_anchor = review_v7_applied_read_anchor_before_new_geometry(&mut app, &source);
    let old_font_size = app.renderer.as_ref().unwrap().font_size;
    app.renderer.as_mut().unwrap().font_size = old_font_size + 2.0;
    review_v4_search(&mut app, &source, "paragraph038");
    let expected_current = review_v7_expected_read_current(&mut app, &live_anchor);
    let expected_target = app.scroll_y.target;
    review_v3_root_frame(&mut app);
    assert!((app.scroll_y.current - expected_current).abs() <= 1.0);
    assert!((app.scroll_y.target - expected_target).abs() <= 1.0);
    assert!(app.markdown.read_layout.is_valid_for_geometry(
        app.editor.version,
        app.markdown_read_content_width_for(
            app.renderer.as_ref().unwrap().width,
            app.renderer.as_ref().unwrap().scale_factor,
        ),
        app.renderer.as_ref().unwrap().scale_factor,
        old_font_size + 2.0,
    ));
    println!(
        "STAGE4_FONT_REBASE old_font={old_font_size} new_font={} source={:?} expected_current={expected_current} actual_current={} expected_target={expected_target} actual_target={}",
        app.renderer.as_ref().unwrap().font_size,
        live_anchor.source_range,
        app.scroll_y.current,
        app.scroll_y.target
    );
}
