//! Characterization of keyboard routing on PDF and image tabs before keymap changes.

use super::ui_tests_hotkeys_characterization::assert_chord_effect;
use crate::headless::profile::HeadlessOptions;
use crate::headless::tests_support::{
    dump, ensure_test_profile_root, run_script, scratch_dir, session_for_test, wait_until,
};
use crate::headless::HeadlessSession;

pub(super) const EXCLUDED_SECTION_1_ROWS: &[(&str, &str)] = &[
    (
        "pdf.copy",
        "requires selecting PDF text first; selection and clipboard copying are covered separately in ui_tests_pdf.rs",
    ),
    (
        "image.zoom_in/image.zoom_out keyboard commands",
        "image_tab::handle_image_key has no zoom-in or zoom-out key branch; zooming is handled by the mouse wheel",
    ),
];

fn run_ok(session: &mut HeadlessSession, script: &str) {
    let lines = run_script(session, script.as_bytes());
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
}

fn writable_session() -> HeadlessSession {
    let options = HeadlessOptions {
        size: (1280, 720),
        allow_writes: true,
        ..HeadlessOptions::default()
    };
    let mut session = match crate::headless::HeadlessSession::new(
        &options,
        ensure_test_profile_root(),
    ) {
        Ok(session) => session,
        Err((code, message)) => panic!("headless session (code {code}): {message}"),
    };
    session.hz_probe = || None;
    session
}

#[test]
fn headless_hotkeys_pdf_image_excluded_rows_have_reasons() {
    for (row, reason) in EXCLUDED_SECTION_1_ROWS {
        assert!(!row.is_empty() && !reason.is_empty());
    }
}

#[test]
fn headless_hotkeys_pdf_filter_preserves_find_save_and_allowed_actions() {
    let dir = scratch_dir("ui-hotkeys-pdf-image-filter");
    let pdf_path = crate::pdf::fixture::write_fixture_pdf(&dir);
    let image_path = dir.join("sample.png");
    let text_path = dir.join("sample.txt");
    std::fs::write(&text_path, "third tab\n").expect("write text fixture");
    image::RgbaImage::from_pixel(1600, 1000, image::Rgba([90, 140, 210, 255]))
        .save_with_format(&image_path, image::ImageFormat::Png)
        .expect("write image fixture");

    let mut session = writable_session();
    run_ok(&mut session, &format!("workspace {}\nopen {}\n", dir.display(), pdf_path.display()));
    wait_until(&mut session, 5000, "PDF tab ready", |session| {
        dump(session)["tabs"][0]["pdf"]["phase"] == "ready"
    });
    wait_until(&mut session, 5000, "PDF text available", |session| {
        session.app.active_pdf_tab().is_some_and(|pdf| pdf.text.first().is_some_and(Option::is_some))
    });

    let before = std::fs::read(&pdf_path).expect("read fixture PDF");
    let before_modified = std::fs::metadata(&pdf_path)
        .expect("read fixture PDF metadata")
        .modified()
        .expect("read fixture PDF modification time");
    assert_eq!(dump(&mut session)["writes_allowed"], true);
    session.app.editor = crate::app::reviewer_stage2_editor_with("hidden editor text");
    session.app.editor.cursor = 6;

    assert_chord_effect(
        &mut session,
        "ctrl+f",
        |_| {},
        |session| {
            assert_eq!(dump(session)["overlays"]["search"], true);
            assert_eq!(session.app.editor.get_full_text(), "hidden editor text");
        },
    );
    assert_chord_effect(
        &mut session,
        "escape",
        |_| {},
        |session| assert_eq!(dump(session)["overlays"]["search"], false),
    );
    assert_chord_effect(
        &mut session,
        "ctrl+s",
        |_| {},
        |session| {
            assert_eq!(session.app.editor.get_full_text(), "hidden editor text");
            assert_eq!(std::fs::read(&pdf_path).expect("read fixture PDF after Save"), before);
            assert_eq!(
                std::fs::metadata(&pdf_path)
                    .expect("read fixture PDF metadata after Save")
                    .modified()
                    .expect("read fixture PDF modification time after Save"),
                before_modified,
            );
        },
    );
    assert_chord_effect(
        &mut session,
        "f1",
        |_| {},
        |session| assert_eq!(dump(session)["overlays"]["settings"], true),
    );
    assert_chord_effect(
        &mut session,
        "f1",
        |_| {},
        |session| assert_eq!(dump(session)["overlays"]["settings"], false),
    );

    run_ok(&mut session, &format!("picker_answer {}\n", image_path.display()));
    assert_chord_effect(
        &mut session,
        "ctrl+o",
        |_| {},
        |session| {
            wait_until(session, 5000, "Open picker result", |session| {
                let state = dump(session);
                state["tabs"].as_array().is_some_and(|tabs| {
                    tabs.len() == 2 && tabs[1]["kind"] == "image" && tabs[1]["active"] == true
                })
            });
        },
    );

    run_ok(&mut session, &format!("open {}\n", text_path.display()));
    wait_until(&mut session, 5000, "third text tab", |session| {
        let state = dump(session);
        state["tabs"].as_array().is_some_and(|tabs| {
            tabs.len() == 3 && tabs[2]["kind"] == "normal" && tabs[2]["active"] == true
        })
    });
    run_ok(&mut session, "key ctrl+pageup\n");
    let state = dump(&mut session);
    assert_eq!(state["tabs"][1]["kind"], "image");
    assert_eq!(state["tabs"][1]["active"], true);
    run_ok(&mut session, "key ctrl+pagedown\n");
    let state = dump(&mut session);
    assert_eq!(state["tabs"][2]["kind"], "normal");
    assert_eq!(state["tabs"][2]["active"], true);
    run_ok(&mut session, "key ctrl+pageup\n");
    let state = dump(&mut session);
    assert_eq!(state["tabs"][1]["active"], true);
    run_ok(&mut session, "key ctrl+pageup\n");
    let state = dump(&mut session);
    assert_eq!(state["tabs"][0]["kind"], "pdf");
    assert_eq!(state["tabs"][0]["active"], true);
    assert_chord_effect(
        &mut session,
        "ctrl+4",
        |_| {},
        |session| {
            let state = dump(session);
            assert_eq!(state["tabs"].as_array().map(Vec::len), Some(2));
            assert_eq!(state["tabs"][0]["kind"], "image");
            assert_eq!(state["tabs"][1]["kind"], "normal");
            assert_eq!(std::fs::read(&pdf_path).expect("read fixture PDF after close"), before);
        },
    );

    run_ok(&mut session, &format!("open {}\n", pdf_path.display()));
    wait_until(&mut session, 5000, "PDF reopened", |session| {
        let state = dump(session);
        state["tabs"].as_array().is_some_and(|tabs| {
            tabs.len() == 3 && tabs[2]["kind"] == "pdf" && tabs[2]["active"] == true
        })
    });
    assert_chord_effect(
        &mut session,
        "ctrl+q",
        |_| {},
        |session| assert!(dump(session)["tabs"].as_array().is_some_and(Vec::is_empty)),
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_hotkeys_image_only_bare_zero_resets_fit() {
    let dir = scratch_dir("ui-hotkeys-image-reset");
    let image_path = dir.join("sample.png");
    image::RgbaImage::from_pixel(1600, 1000, image::Rgba([90, 140, 210, 255]))
        .save_with_format(&image_path, image::ImageFormat::Png)
        .expect("write image fixture");
    let mut session = session_for_test(1280, 720);
    run_ok(&mut session, &format!("workspace {}\nopen {}\n", dir.display(), image_path.display()));
    wait_until(&mut session, 5000, "image tab ready", |session| {
        let state = dump(session);
        state["tabs"][0]["kind"] == "image" && state["tabs"][0]["image"]["phase"] == "ready"
    });

    let [x, y, width, height] = crate::headless::tests_support::ui_rect(&dump(&mut session), "PdfBody")
        .map(|value| value as f32);
    let image = session.app.tabs[session.app.active_tab].image.as_deref_mut().expect("image state");
    image.body = (x, y, width, height);
    image.zoom_at(2.0, x + width * 0.5, y + height * 0.5);
    let zoomed = image.zoom;
    let fit = image.fit_scale(width, height);
    assert!(zoomed > fit, "fixture zoom must differ from fit: zoom={zoomed}, fit={fit}");

    assert_chord_effect(
        &mut session,
        "ctrl+0",
        |_| {},
        |session| {
            let zoom = session.app.tabs[session.app.active_tab].image.as_deref().expect("image state").zoom;
            assert_eq!(zoom, zoomed, "modified 0 must not reset image fit");
        },
    );
    assert_chord_effect(
        &mut session,
        "0",
        |_| {},
        |session| {
            let image = session.app.tabs[session.app.active_tab].image.as_deref().expect("image state");
            assert!((image.zoom - fit).abs() <= f32::EPSILON, "bare 0 restores fit: {} != {fit}", image.zoom);
        },
    );
    let _ = std::fs::remove_dir_all(dir);
}
