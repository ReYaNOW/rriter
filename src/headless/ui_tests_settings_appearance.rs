//! Headless settings regressions for the Editor and Appearance tabs.

use crate::headless::tests_support::{click_ui, dump, has_ui, run_script, session_for_test, wait_until};
use crate::headless::HeadlessSession;

const TEST_WIDTH: u32 = 1280;
const TEST_HEIGHT: u32 = 720;
const TEST_SCALE: f32 = 4.0 / 3.0;

fn open_settings_tab(tab: usize) -> HeadlessSession {
    assert!(matches!(tab, 2 | 3));
    let mut session = session_for_test(TEST_WIDTH, TEST_HEIGHT);
    let lines = run_script(
        &mut session,
        format!("scale {TEST_SCALE}\nkey f1\n").as_bytes(),
    );
    assert!(lines.iter().all(|line| line == "ok"), "{lines:?}");

    wait_until(&mut session, 5000, "Settings overlay", |session| {
        let state = dump(session);
        state["overlays"]["settings"] == true && has_ui(&state, "SettingsTab(0)")
    });

    let tab_id = format!("SettingsTab({tab})");
    click_ui(&mut session, &tab_id);
    wait_until(&mut session, 5000, "selected Settings tab", |session| {
        let state = dump(session);
        session.app.settings_tab == tab && has_ui(&state, &tab_id)
    });
    session
}

fn adjust_ctrl_wheel_to(session: &mut HeadlessSession, target: f32) {
    for _ in 0..32 {
        let current = session.app.ctrl_wheel_multiplier;
        if current == target {
            break;
        }
        let direction = if current < target { 1 } else { -1 };
        click_ui(
            session,
            &format!("SettingsEditorCtrlWheelAdjust({direction})"),
        );
    }
}

#[test]
fn headless_settings_editor_ctrl_wheel_adjusts_and_restores_multiplier() {
    let mut session = open_settings_tab(2);
    let initial = session.app.ctrl_wheel_multiplier;
    adjust_ctrl_wheel_to(&mut session, crate::CTRL_WHEEL_MULTIPLIER_DEFAULT);

    let state = dump(&mut session);
    assert!(has_ui(&state, "SettingsEditorCtrlWheelAdjust(1)"), "{state}");
    assert!(has_ui(&state, "SettingsEditorCtrlWheelAdjust(-1)"), "{state}");
    click_ui(&mut session, "SettingsEditorCtrlWheelAdjust(1)");
    let increased = session.app.ctrl_wheel_multiplier;
    click_ui(&mut session, "SettingsEditorCtrlWheelAdjust(-1)");
    let returned_to_default = session.app.ctrl_wheel_multiplier;
    adjust_ctrl_wheel_to(&mut session, initial);
    let restored = session.app.ctrl_wheel_multiplier;

    assert_eq!(increased, crate::CTRL_WHEEL_MULTIPLIER_DEFAULT + crate::CTRL_WHEEL_MULTIPLIER_STEP);
    assert_eq!(returned_to_default, crate::CTRL_WHEEL_MULTIPLIER_DEFAULT);
    assert_eq!(restored, initial, "restore shared temporary test profile");
}

#[test]
fn headless_settings_editor_ctrl_wheel_clamps_at_minimum() {
    let mut session = open_settings_tab(2);
    let initial = session.app.ctrl_wheel_multiplier;
    adjust_ctrl_wheel_to(&mut session, crate::CTRL_WHEEL_MULTIPLIER_DEFAULT);

    adjust_ctrl_wheel_to(&mut session, crate::CTRL_WHEEL_MULTIPLIER_MIN);
    let at_minimum = session.app.ctrl_wheel_multiplier;
    click_ui(&mut session, "SettingsEditorCtrlWheelAdjust(-1)");
    let after_decrement_at_minimum = session.app.ctrl_wheel_multiplier;
    adjust_ctrl_wheel_to(&mut session, crate::CTRL_WHEEL_MULTIPLIER_DEFAULT);
    let restored_to_default = session.app.ctrl_wheel_multiplier;
    adjust_ctrl_wheel_to(&mut session, initial);
    let restored = session.app.ctrl_wheel_multiplier;

    assert_eq!(at_minimum, crate::CTRL_WHEEL_MULTIPLIER_MIN);
    assert_eq!(after_decrement_at_minimum, crate::CTRL_WHEEL_MULTIPLIER_MIN);
    assert_eq!(restored_to_default, crate::CTRL_WHEEL_MULTIPLIER_DEFAULT);
    assert_eq!(restored, initial, "restore shared temporary test profile");
}

#[test]
fn headless_settings_editor_ctrl_wheel_clamps_at_maximum() {
    let mut session = open_settings_tab(2);
    let initial = session.app.ctrl_wheel_multiplier;
    adjust_ctrl_wheel_to(&mut session, crate::CTRL_WHEEL_MULTIPLIER_DEFAULT);

    adjust_ctrl_wheel_to(&mut session, crate::CTRL_WHEEL_MULTIPLIER_MAX);
    let at_maximum = session.app.ctrl_wheel_multiplier;
    click_ui(&mut session, "SettingsEditorCtrlWheelAdjust(1)");
    let after_increment_at_maximum = session.app.ctrl_wheel_multiplier;
    adjust_ctrl_wheel_to(&mut session, crate::CTRL_WHEEL_MULTIPLIER_DEFAULT);
    let restored_to_default = session.app.ctrl_wheel_multiplier;
    adjust_ctrl_wheel_to(&mut session, initial);
    let restored = session.app.ctrl_wheel_multiplier;

    assert_eq!(at_maximum, crate::CTRL_WHEEL_MULTIPLIER_MAX);
    assert_eq!(after_increment_at_maximum, crate::CTRL_WHEEL_MULTIPLIER_MAX);
    assert_eq!(restored_to_default, crate::CTRL_WHEEL_MULTIPLIER_DEFAULT);
    assert_eq!(restored, initial, "restore shared temporary test profile");
}
