//! Headless coverage for the mode 2026 hold deadline at app level: the event loop waits until
//! the deadline, merges it with the other terminal tabs, replays the held output when it passes
//! (also while rendering is suspended) and asks for a frame. Expected deadlines come from the
//! grid's own hold deadline, not from the app code under test.

use crate::app::events::about;
use crate::app::events::host_loop::HostLoop;
use crate::headless::HeadlessSession;
use crate::headless::frame::take_redraw_request;
use crate::headless::tests_support::{
    click_ui, dump, feed, has_ui, shell_failed, terminal_session, terminal_text, wait_until,
};
use std::path::PathBuf;
use std::time::{Duration, Instant};
use winit::event_loop::ControlFlow;

/// Shell tab 0 and a second shell tab 1; tab 0 is active, so tab 1 is the background tab.
fn two_terminal_session(name: &str) -> Option<(PathBuf, HeadlessSession)> {
    let (dir, mut session) = terminal_session(name);
    if shell_failed(&session, 0) {
        let _ = std::fs::remove_dir_all(dir);
        return None;
    }
    click_ui(&mut session, "TerminalAdd");
    wait_until(&mut session, 5000, "second terminal tab", |session| {
        session.app.ide_panel.terminals.len() == 2 && has_ui(&dump(session), "TerminalTab(1)")
    });
    if shell_failed(&session, 1) {
        let _ = std::fs::remove_dir_all(dir);
        return None;
    }
    click_ui(&mut session, "TerminalTab(0)");
    assert_eq!(session.app.ide_panel.active_terminal, 0);
    Some((dir, session))
}

/// One pass of the headless event loop; returns the control flow it requested.
fn about_wait(session: &mut HeadlessSession) -> ControlFlow {
    about::about_to_wait(&mut session.app, &HostLoop::headless(&session.loop_state));
    session.loop_state.last_control_flow.get()
}

fn hold_deadline(session: &HeadlessSession, index: usize) -> Instant {
    session.app.ide_panel.terminals[index]
        .sync_deadline()
        .expect("mode 2026 holds the output")
}

/// The UI thread replays a hold only this long after its deadline, as a fallback for a stalled
/// parser thread; the parser replays on the deadline itself (final-fix-1 decision M2).
const UI_FALLBACK_GRACE: Duration = Duration::from_millis(50);

/// Sleeps until `deadline` has passed, so the next pass sees it as expired.
fn sleep_past(deadline: Instant) {
    let remaining = deadline.saturating_duration_since(Instant::now());
    std::thread::sleep(remaining + Duration::from_millis(5));
}

fn held_text_visible(session: &HeadlessSession, index: usize, text: &str) -> bool {
    terminal_text(session, index).is_some_and(|rows| rows.iter().any(|row| row.contains(text)))
}

#[test]
fn sync_deadline_sets_wait_until() {
    let Some((dir, mut session)) = two_terminal_session("ui-terminal-sync-wait") else {
        return;
    };
    feed(&session, 1, b"\x1b[?2026hheld-wait");
    let deadline = hold_deadline(&session, 1);

    assert_eq!(about_wait(&mut session), ControlFlow::WaitUntil(deadline + UI_FALLBACK_GRACE));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn sync_deadline_merged_with_other_deadlines() {
    let Some((dir, mut session)) = two_terminal_session("ui-terminal-sync-merge") else {
        return;
    };
    // The first hold ends earlier than the second one, which starts 10 ms later in the other tab.
    feed(&session, 1, b"\x1b[?2026hheld-first");
    let first = hold_deadline(&session, 1);
    std::thread::sleep(Duration::from_millis(10));
    feed(&session, 0, b"\x1b[?2026hheld-second");
    let second = hold_deadline(&session, 0);
    assert!(first < second);

    assert_eq!(about_wait(&mut session), ControlFlow::WaitUntil(first + UI_FALLBACK_GRACE));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn sync_deadline_served_while_render_suspended() {
    let Some((dir, mut session)) = two_terminal_session("ui-terminal-sync-suspended") else {
        return;
    };
    feed(&session, 1, b"\x1b[?2026hheld-suspended");
    let deadline = hold_deadline(&session, 1);
    session.app.render_suspended = true;

    assert_eq!(about_wait(&mut session), ControlFlow::WaitUntil(deadline + UI_FALLBACK_GRACE));
    assert!(!held_text_visible(&session, 1, "held-suspended"));

    sleep_past(deadline + UI_FALLBACK_GRACE);
    about_wait(&mut session);
    assert!(held_text_visible(&session, 1, "held-suspended"));
    assert_eq!(session.app.ide_panel.terminals[1].sync_deadline(), None);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn sync_deadline_flushed_at_deadline_redraws() {
    let Some((dir, mut session)) = two_terminal_session("ui-terminal-sync-redraw") else {
        return;
    };
    feed(&session, 1, b"\x1b[?2026hheld-redraw");
    let deadline = hold_deadline(&session, 1);
    take_redraw_request(&session.app);

    sleep_past(deadline + UI_FALLBACK_GRACE);
    about_wait(&mut session);
    assert!(held_text_visible(&session, 1, "held-redraw"));
    assert!(take_redraw_request(&session.app), "the replay must request a frame");
    let _ = std::fs::remove_dir_all(dir);
}
