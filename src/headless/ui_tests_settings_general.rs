//! UI coverage for the General settings tab.

use crate::app::DartSettings;
use crate::headless::tests_support::{
    assert_ui_rect_inside_window, click_ui, dump, has_ui, run_script, session_for_test, ui_rect,
    wait_until,
};
use crate::headless::HeadlessSession;

const TEST_WIDTH: u32 = 1280;
const TEST_HEIGHT: u32 = 720;
const DEFAULT_SCALE: f32 = 1.0;
const FRACTIONAL_SCALE: f32 = 4.0 / 3.0;

fn open_general_settings(session: &mut HeadlessSession, scale: f32) {
    let script = format!("scale {scale}\nkey f1\nwait 900\n");
    let lines = run_script(session, script.as_bytes());
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
    assert_eq!(dump(session)["overlays"]["settings"], true);

    click_ui(session, "SettingsTab(1)");
    wait_until(session, 2000, "General settings content", |session| {
        has_ui(&dump(session), "SettingsRefreshTools")
    });
}

#[test]
fn headless_settings_general_toggles_dart_support_and_workspace_analysis() {
    let mut session = session_for_test(TEST_WIDTH, TEST_HEIGHT);
    open_general_settings(&mut session, DEFAULT_SCALE);

    let support_before = session.app.dart_settings.enabled;
    click_ui(&mut session, "SettingsDartToggleSupport");
    assert_eq!(session.app.dart_settings.enabled, !support_before);
    click_ui(&mut session, "SettingsDartToggleSupport");
    assert_eq!(session.app.dart_settings.enabled, support_before);

    let analysis_before = session.app.dart_settings.workspace_analysis;
    click_ui(&mut session, "SettingsDartToggleWorkspaceAnalysis");
    assert_eq!(session.app.dart_settings.workspace_analysis, !analysis_before);
    click_ui(&mut session, "SettingsDartToggleWorkspaceAnalysis");
    assert_eq!(session.app.dart_settings.workspace_analysis, analysis_before);
}

#[test]
fn headless_settings_general_cycles_and_restores_dart_closing_labels() {
    let mut session = session_for_test(TEST_WIDTH, TEST_HEIGHT);
    open_general_settings(&mut session, DEFAULT_SCALE);

    let original = session.app.dart_settings.closing_labels;
    click_ui(&mut session, "SettingsDartCycleClosingLabels");
    assert_eq!(session.app.dart_settings.closing_labels, original.next());

    for _ in 0..2 {
        if session.app.dart_settings.closing_labels != original {
            click_ui(&mut session, "SettingsDartCycleClosingLabels");
        }
    }
    assert_eq!(session.app.dart_settings.closing_labels, original);
}

#[test]
fn headless_settings_general_adjusts_and_restores_dart_hint_thresholds() {
    let mut session = session_for_test(TEST_WIDTH, TEST_HEIGHT);
    open_general_settings(&mut session, DEFAULT_SCALE);

    let nesting_before = session.app.dart_settings.minimum_nesting_depth;
    let nesting_delta = if nesting_before < DartSettings::MAX_NESTING_DEPTH {
        1
    } else {
        -1
    };
    let nesting_after = if nesting_delta > 0 {
        nesting_before + 1
    } else {
        nesting_before - 1
    };
    click_ui(
        &mut session,
        &format!("SettingsDartAdjustNesting({nesting_delta})"),
    );
    assert_eq!(session.app.dart_settings.minimum_nesting_depth, nesting_after);
    click_ui(&mut session, &format!("SettingsDartAdjustNesting({})", -nesting_delta));
    assert_eq!(session.app.dart_settings.minimum_nesting_depth, nesting_before);

    let block_lines_before = session.app.dart_settings.minimum_block_lines;
    let block_lines_delta = if block_lines_before < DartSettings::MAX_BLOCK_LINES {
        1
    } else {
        -1
    };
    let block_lines_after = if block_lines_delta > 0 {
        block_lines_before + 1
    } else {
        block_lines_before - 1
    };
    click_ui(
        &mut session,
        &format!("SettingsDartAdjustBlockLines({block_lines_delta})"),
    );
    assert_eq!(session.app.dart_settings.minimum_block_lines, block_lines_after);
    click_ui(
        &mut session,
        &format!("SettingsDartAdjustBlockLines({})", -block_lines_delta),
    );
    assert_eq!(session.app.dart_settings.minimum_block_lines, block_lines_before);
}

#[test]
fn headless_settings_general_dart_controls_are_reachable_at_fractional_scale() {
    let mut session = session_for_test(TEST_WIDTH, TEST_HEIGHT);
    open_general_settings(&mut session, FRACTIONAL_SCALE);
    let id = "SettingsDartToggleSupport";
    let before = dump(&mut session);
    let fully_inside_window = |state: &serde_json::Value| {
        if !has_ui(state, id) {
            return false;
        }
        let [x, y, width, height] = ui_rect(state, id);
        let window_width = state["size"][0].as_f64().unwrap();
        let window_height = state["size"][1].as_f64().unwrap();
        x >= 0.0 && y >= 0.0 && x + width <= window_width && y + height <= window_height
    };
    assert!(
        !fully_inside_window(&before),
        "{id} should be absent or clipped before scrolling: {before}"
    );

    let lines = run_script(&mut session, b"mouse_move 900 550\nwheel 0 -8\n");
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
    wait_until(&mut session, 5000, "General settings scroll", |session| {
        let lines = run_script(session, b"settle 2000\n");
        lines.iter().any(|line| line.ends_with("settled=true"))
    });

    let after = dump(&mut session);
    assert!(session.app.settings_general_scroll.current > 0.0);
    assert!(fully_inside_window(&after), "{id} is not inside after scrolling: {after}");
    assert_ui_rect_inside_window(&after, id);
    let support_before = session.app.dart_settings.enabled;
    click_ui(&mut session, id);
    assert_eq!(session.app.dart_settings.enabled, !support_before);
    click_ui(&mut session, id);
    assert_eq!(session.app.dart_settings.enabled, support_before);
}
