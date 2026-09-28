//! Headless interaction coverage for the integrated terminal.

use crate::headless::tests_support::{
    click_ui, dump, has_ui, run_script, run_terminal_command, scratch_dir, session_for_test,
    shell_failed, terminal_has_line, terminal_session, ui_rect, wait_until,
};

#[test]
fn headless_terminal_types_command_and_shows_its_output() {
    let (dir, mut session) = terminal_session("ui-terminal-command");
    if shell_failed(&session, 0) {
        let _ = std::fs::remove_dir_all(dir);
        return;
    }

    run_terminal_command(&mut session, "echo marker");
    wait_until(&mut session, 5000, "echo output", |session| {
        terminal_has_line(session, 0, "marker")
    });
    assert!(terminal_has_line(&session, 0, "marker"));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_terminal_tabs_add_switch_and_close() {
    let (dir, mut session) = terminal_session("ui-terminal-tabs");
    if shell_failed(&session, 0) {
        let _ = std::fs::remove_dir_all(dir);
        return;
    }

    run_terminal_command(&mut session, "echo first-terminal");
    wait_until(&mut session, 5000, "first terminal output", |session| {
        terminal_has_line(session, 0, "first-terminal")
    });

    click_ui(&mut session, "TerminalAdd");
    wait_until(&mut session, 5000, "second terminal tab", |session| {
        session.app.ide_panel.terminals.len() == 2 && has_ui(&dump(session), "TerminalTab(1)")
    });
    if shell_failed(&session, 1) {
        let _ = std::fs::remove_dir_all(dir);
        return;
    }
    run_terminal_command(&mut session, "echo second-terminal");
    wait_until(&mut session, 5000, "second terminal output", |session| {
        terminal_has_line(session, 1, "second-terminal")
    });

    click_ui(&mut session, "TerminalTab(0)");
    assert_eq!(session.app.ide_panel.active_terminal, 0);
    assert!(terminal_has_line(&session, 0, "first-terminal"));
    assert!(!terminal_has_line(&session, 0, "second-terminal"));
    click_ui(&mut session, "TerminalTab(1)");
    assert_eq!(session.app.ide_panel.active_terminal, 1);
    assert!(terminal_has_line(&session, 1, "second-terminal"));
    assert!(!terminal_has_line(&session, 1, "first-terminal"));

    click_ui(&mut session, "TerminalTabClose(1)");
    wait_until(&mut session, 5000, "terminal tab close", |session| {
        session.app.ide_panel.terminals.len() == 1
            && session.app.ide_panel.active_terminal == 0
    });
    assert!(has_ui(&dump(&mut session), "TerminalTab(0)"));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_terminal_search_highlights_navigates_and_closes() {
    let (dir, mut session) = terminal_session("ui-terminal-search");
    if shell_failed(&session, 0) {
        let _ = std::fs::remove_dir_all(dir);
        return;
    }

    run_terminal_command(&mut session, "echo marker");
    run_terminal_command(&mut session, "echo marker");
    wait_until(&mut session, 5000, "repeated terminal output", |session| {
        let terminal = &session.app.ide_panel.terminals[0];
        let grid = crate::app::terminal::lock_terminal_grid(&terminal.grid);
        grid.scrollback
            .iter()
            .chain(grid.lines.iter())
            .filter(|row| row.iter().map(|cell| cell.c).collect::<String>().trim() == "marker")
            .count()
            >= 2
    });

    let lines = run_script(&mut session, b"key ctrl+f\ntype marker\n");
    assert!(lines.iter().all(|line| line == "ok"), "{lines:?}");
    wait_until(&mut session, 1000, "terminal search match", |session| {
        session.app.ide_panel.term_show_search
            && session.app.ide_panel.term_search_focused
            && !session.app.ide_panel.term_search_results.is_empty()
    });
    let (current, expected) = {
        let search = &session.app.ide_panel;
        assert!(search.term_show_search && search.term_search_focused);
        assert!(search.term_search_results.len() >= 2);
        let current = search.term_search_current_idx.unwrap();
        (current, search.term_search_results[current])
    };
    assert!(has_ui(&dump(&mut session), "TerminalSearchInput"));
    let grid_selection = {
        let terminal = &session.app.ide_panel.terminals[0];
        crate::app::terminal::lock_terminal_grid(&terminal.grid).selection
    };
    assert_eq!(grid_selection, Some(expected));

    click_ui(&mut session, "TerminalSearchPrev");
    let search = &session.app.ide_panel;
    let previous = (current + search.term_search_results.len() - 1)
        % search.term_search_results.len();
    assert_eq!(search.term_search_current_idx, Some(previous));
    let expected = search.term_search_results[previous];
    let grid_selection = {
        let terminal = &session.app.ide_panel.terminals[0];
        crate::app::terminal::lock_terminal_grid(&terminal.grid).selection
    };
    assert_eq!(grid_selection, Some(expected));

    let lines = run_script(&mut session, b"key escape\n");
    assert!(lines.iter().all(|line| line == "ok"), "{lines:?}");
    assert!(!session.app.ide_panel.term_show_search);
    assert!(!session.app.ide_panel.term_search_focused);
    assert!(!has_ui(&dump(&mut session), "TerminalSearchInput"));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_terminal_mouse_selection_copies_selected_text() {
    let (dir, mut session) = terminal_session("ui-terminal-selection");
    if shell_failed(&session, 0) {
        let _ = std::fs::remove_dir_all(dir);
        return;
    }

    run_terminal_command(&mut session, "echo marker");
    wait_until(&mut session, 5000, "selectable terminal output", |session| {
        terminal_has_line(session, 0, "marker")
    });
    let body = ui_rect(&dump(&mut session), "TerminalBody");
    let (start_x, end_x, row_y) = {
        let renderer = session.app.renderer.as_ref().expect("headless renderer");
        let scale = renderer.scale_factor;
        let char_w = renderer.ascii_advances['A' as usize].max(1.0)
            * crate::render_view::terminal_ui::TERMINAL_TEXT_SCALE;
        let char_h = renderer.line_height * crate::render_view::terminal_ui::TERMINAL_TEXT_SCALE;
        let (_, bottom_pad) = crate::render_view::terminal_ui::terminal_text_padding(scale);
        let terminal = &session.app.ide_panel.terminals[0];
        let grid = crate::app::terminal::lock_terminal_grid(&terminal.grid);
        let scrollback_len = if grid.is_alt { 0 } else { grid.scrollback.len() };
        let total_lines = scrollback_len + grid.lines.len();
        // The shell may pad the output row, so select from the column where the text starts.
        let (row, col) = grid
            .scrollback
            .iter()
            .chain(grid.lines.iter())
            .enumerate()
            .find_map(|(row, line)| {
                let text = line.iter().map(|cell| cell.c).collect::<String>();
                (text.trim() == "marker")
                    .then(|| (row, text.chars().take_while(|c| c.is_whitespace()).count()))
            })
            .expect("echo output row");
        let max_scroll = crate::render_view::terminal_ui::terminal_max_scroll(
            total_lines,
            char_h,
            body[3] as f32,
            scale,
        );
        let scroll_offset = crate::render_view::terminal_ui::terminal_render_scroll_offset(
            terminal.scroll_y.current,
            max_scroll,
            grid.is_alt,
        );
        let offset_from_bottom = total_lines.saturating_sub(1).saturating_sub(row);
        let text_top = body[1] as f32 + body[3] as f32
            - bottom_pad
            - char_h
            - offset_from_bottom as f32 * char_h
            + scroll_offset;
        let text_x = body[0] as f32 + 10.0 * scale + col as f32 * char_w;
        (text_x + char_w * 0.25, text_x + char_w * 5.75, text_top + char_h * 0.5)
    };
    let lines = run_script(
        &mut session,
        format!(
            "mouse_move {start_x} {row_y}\nclick left down\nmouse_move {end_x} {row_y}\nclick left up\n"
        )
        .as_bytes(),
    );
    assert!(lines.iter().all(|line| line == "ok"), "{lines:?}");
    wait_until(&mut session, 1000, "terminal mouse selection", |session| {
        let terminal = &session.app.ide_panel.terminals[0];
        crate::app::terminal::lock_terminal_grid(&terminal.grid).selection.is_some()
    });

    let before_copy = dump(&mut session);
    if before_copy["clipboard"]["mode"] == "memory" {
        let lines = run_script(&mut session, b"key ctrl+c\n");
        assert!(lines.iter().all(|line| line == "ok"), "{lines:?}");
        assert_eq!(dump(&mut session)["clipboard"]["text"].as_str(), Some("marker"));
    } else {
        let grid = crate::app::terminal::lock_terminal_grid(
            &session.app.ide_panel.terminals[0].grid,
        );
        assert!(grid.selection.is_some());
    }
    let _ = std::fs::remove_dir_all(dir);
}
