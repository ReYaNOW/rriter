//! Headless interaction coverage for the terminal scrollbar.

use crate::headless::tests_support::{
    dump, run_script, run_terminal_command, shell_failed, terminal_has_line, terminal_session,
    ui_rect, wait_until,
};

#[test]
fn headless_terminal_scrollbar_track_click_returns_toward_bottom() {
    let (dir, mut session) = terminal_session("ui-terminal-scrollbar-track-click");
    if shell_failed(&session, 0) {
        let _ = std::fs::remove_dir_all(dir);
        return;
    }

    run_terminal_command(&mut session, "seq 1 200");
    wait_until(&mut session, 8000, "terminal seq output", |session| {
        terminal_has_line(session, 0, "200")
    });
    wait_until(&mut session, 5000, "terminal initial bottom", |session| {
        session.app.ide_panel.terminals[0].scroll_y.is_settled()
    });

    let [lane_x, lane_y, lane_w, lane_h] = ui_rect(&dump(&mut session), "TerminalScrollY");
    let body = ui_rect(&dump(&mut session), "TerminalBody");
    let max_scroll = {
        let renderer = session.app.renderer.as_ref().expect("headless renderer");
        let terminal = &session.app.ide_panel.terminals[0];
        let grid = crate::app::terminal::lock_terminal_grid(&terminal.grid);
        let total_lines = grid.scrollback.len() + grid.lines.len();
        let char_h = renderer.line_height * crate::render_view::terminal_ui::TERMINAL_TEXT_SCALE;
        crate::render_view::terminal_ui::terminal_max_scroll(
            total_lines,
            char_h,
            body[3] as f32,
            renderer.scale_factor,
        )
    };
    assert!(max_scroll > 0.0, "terminal must overflow its viewport");

    let lane_x = lane_x + lane_w * 0.5;
    let body = ui_rect(&dump(&mut session), "TerminalBody");
    let lines = run_script(
        &mut session,
        format!("mouse_move {} {}\nwheel 0 1\n", body[0] + body[2] / 2.0, body[1] + body[3] / 2.0)
            .as_bytes(),
    );
    assert!(lines.iter().all(|line| line == "ok"), "{lines:?}");
    wait_until(&mut session, 5000, "terminal wheel scroll", |session| {
        session.app.ide_panel.terminals[0].scroll_y.is_settled()
            && session.app.ide_panel.terminals[0].scroll_y.current > 0.0
    });
    let wheel_offset = session.app.ide_panel.terminals[0].scroll_y.current;
    assert!(wheel_offset <= max_scroll, "wheel offset {wheel_offset} > {max_scroll}");

    let track_y = lane_y + lane_h - 1.0;
    let lines = run_script(
        &mut session,
        format!("mouse_move {lane_x} {track_y}\nclick left down\nclick left up\n").as_bytes(),
    );
    assert!(lines.iter().all(|line| line == "ok"), "{lines:?}");
    wait_until(&mut session, 5000, "terminal track click toward bottom", |session| {
        session.app.ide_panel.terminals[0].scroll_y.is_settled()
    });
    let clicked_offset = session.app.ide_panel.terminals[0].scroll_y.current;
    assert!(
        clicked_offset < wheel_offset,
        "track click offset {clicked_offset} should be below wheel offset {wheel_offset}"
    );
    assert!(clicked_offset >= 0.0 && clicked_offset <= max_scroll);
    let _ = std::fs::remove_dir_all(dir);
}
