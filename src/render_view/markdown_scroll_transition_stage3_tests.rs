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
