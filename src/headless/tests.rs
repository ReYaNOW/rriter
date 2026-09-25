//! Integration tests of the headless session on a real offscreen EGL context, plus the
//! shared fixtures (`tests_support`) later headless tasks build on.

pub(crate) mod tests_support {
    use crate::headless::HeadlessSession;
    use crate::headless::profile::HeadlessOptions;
    use std::path::PathBuf;
    use std::sync::OnceLock;

    static TEST_PROFILE_ROOT: OnceLock<PathBuf> = OnceLock::new();

    /// One temporary profile root per test process. The first call installs it as the
    /// app root override, so every later test writes RRiter state there, never into the
    /// real `~/.config`. Tests must not call `set_app_root_override` themselves.
    pub(crate) fn ensure_test_profile_root() -> PathBuf {
        TEST_PROFILE_ROOT
            .get_or_init(|| {
                let root = std::env::temp_dir()
                    .join(format!("rriter-headless-test-profile-{}", std::process::id()));
                std::fs::create_dir_all(&root).expect("create test profile root");
                let _ = crate::platform::set_app_root_override(root.clone());
                root
            })
            .clone()
    }

    /// Session on its own offscreen context; the global headless policy stays unset.
    pub(crate) fn session_for_test(w: u32, h: u32) -> HeadlessSession {
        let root = ensure_test_profile_root();
        let options = HeadlessOptions { size: (w, h), ..HeadlessOptions::default() };
        let mut session = match HeadlessSession::new(&options, root) {
            Ok(session) => session,
            Err((code, message)) => panic!("headless session (code {code}): {message}"),
        };
        session.hz_probe = || None;
        session
    }
}

mod session_cases {
    use crate::headless::tests_support::session_for_test;
    use crate::app::events::host_loop::HostLoop;
    use crate::headless::HeadlessSession;
    use std::io::{self, Cursor, Write};
    use std::path::PathBuf;

    fn scratch_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir()
            .join(format!("rriter-headless-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create scratch dir");
        dir
    }

    fn run_script(session: &mut HeadlessSession, script: &[u8]) -> Vec<String> {
        let mut out = Vec::new();
        assert!(session.run_loop(Cursor::new(script.to_vec()), &mut out));
        String::from_utf8(out)
            .expect("utf-8 responses")
            .lines()
            .map(str::to_string)
            .collect()
    }

    fn sample_file(dir: &std::path::Path) -> PathBuf {
        let path = dir.join("sample.txt");
        let mut text = String::new();
        for i in 0..40 {
            text.push_str(&format!("line {i:02} headless screenshot content\n"));
        }
        std::fs::write(&path, text).expect("write sample file");
        path
    }

    fn close_to(pixel: &[u8], expected: [u8; 3]) -> bool {
        pixel.iter().zip(expected).all(|(&have, want)| have.abs_diff(want) <= 2)
    }

    #[test]
    fn headless_session_open_and_screenshot_png() {
        let dir = scratch_dir("screenshot");
        let file = sample_file(&dir);
        let out = dir.join("shots").join("out.png");
        let mut session = session_for_test(640, 400);
        let bg = session.app.theme.bg;
        let bg = [0, 1, 2].map(|i| (bg[i].clamp(0.0, 1.0) * 255.0).round() as u8);
        let lines = run_script(
            &mut session,
            format!("open {}\nscreenshot {}\n", file.display(), out.display()).as_bytes(),
        );
        assert_eq!(lines.len(), 2, "{lines:?}");
        assert!(lines[0].starts_with("ok tabs="), "{lines:?}");
        assert_eq!(lines[1], format!("ok {} 640x400", out.display()));
        let image = image::open(&out).expect("decode screenshot").to_rgba8();
        assert_eq!(image.dimensions(), (640, 400));
        let pixels = image.as_raw().chunks_exact(4);
        assert!(pixels.clone().any(|px| close_to(px, bg)), "no theme background pixel {bg:?}");
        assert!(pixels.clone().any(|px| !close_to(px, bg)), "screenshot is a flat background");
        assert!(pixels.clone().all(|px| px[3] == 255));
        assert_eq!(session.exit_code(), 0);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn headless_session_open_rejects_missing_and_directory() {
        let dir = scratch_dir("open-errors");
        let mut session = session_for_test(640, 400);
        let lines = run_script(
            &mut session,
            format!("open {}/missing.txt\nopen {}\nworkspace {}/missing\n", dir.display(), dir.display(), dir.display())
                .as_bytes(),
        );
        assert_eq!(lines.len(), 3, "{lines:?}");
        assert!(lines.iter().all(|line| line.starts_with("err ")), "{lines:?}");
        assert!(lines[1].ends_with("is a directory"), "{lines:?}");
        assert_eq!(session.exit_code(), 1);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn headless_session_resize_changes_screenshot_size() {
        let dir = scratch_dir("resize");
        let out = dir.join("big.png");
        let mut session = session_for_test(640, 400);
        let lines = run_script(&mut session, format!("resize 800x600\nscreenshot {}\n", out.display()).as_bytes());
        assert_eq!(lines, vec!["ok 800x600".to_string(), format!("ok {} 800x600", out.display())]);
        let image = image::open(&out).expect("decode screenshot");
        assert_eq!((image.width(), image.height()), (800, 600));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn headless_session_quit_stops_reading() {
        let mut session = session_for_test(640, 400);
        let lines = run_script(&mut session, b"mouse_move 5 5\nquit\nbogus\nmouse_move 1 1\n");
        assert_eq!(lines, vec!["ok", "ok"]);
        assert_eq!(session.exit_code(), 0);
    }

    #[test]
    fn headless_session_app_exit_ends_loop_after_current_response() {
        let mut session = session_for_test(640, 400);
        HostLoop::headless(&session.loop_state).exit();
        let lines = run_script(&mut session, b"mouse_move 1 1\nmouse_move 2 2\n");
        assert_eq!(lines, vec!["ok"]);
        assert_eq!(session.exit_code(), 0);
    }

    #[test]
    fn headless_session_skips_blank_and_comment_lines_and_counts_errors() {
        let mut session = session_for_test(640, 400);
        let lines = run_script(&mut session, b"bogus\nmouse_move 10 10\n# c\n\nquit\nmouse_move 1 1\n");
        assert_eq!(lines, vec!["err unknown command 'bogus'", "ok", "ok"]);
        assert_eq!(session.exit_code(), 1);
    }

    #[test]
    fn headless_session_non_utf8_line_reports_error_and_continues() {
        let mut session = session_for_test(640, 400);
        let lines = run_script(&mut session, b"\xff\xfe\nmouse_move 1 1\n");
        assert_eq!(lines, vec!["err invalid utf-8", "ok"]);
        assert_eq!(session.exit_code(), 1);
    }

    #[test]
    fn headless_session_screenshot_io_error_then_continues() {
        let mut session = session_for_test(640, 400);
        let lines = run_script(&mut session, b"screenshot /proc/rriter-nope/x.png\nmouse_move 1 1\n");
        assert_eq!(lines.len(), 2, "{lines:?}");
        assert!(lines[0].starts_with("err io: "), "{lines:?}");
        assert_eq!(lines[1], "ok");
        assert_eq!(session.exit_code(), 1);
    }

    struct BrokenPipe;

    impl Write for BrokenPipe {
        fn write(&mut self, _buf: &[u8]) -> io::Result<usize> {
            Err(io::Error::from(io::ErrorKind::BrokenPipe))
        }

        fn flush(&mut self) -> io::Result<()> {
            Err(io::Error::from(io::ErrorKind::BrokenPipe))
        }
    }

    #[test]
    fn headless_session_broken_pipe_writer_ends_loop() {
        let mut session = session_for_test(640, 400);
        let done = session.run_loop(Cursor::new(b"mouse_move 1 1\nmouse_move 2 2\n".to_vec()), BrokenPipe);
        assert!(done);
        assert_eq!(session.exit_code(), 0);
    }

    #[test]
    fn headless_session_input_commands_answer_ok() {
        let mut session = session_for_test(640, 400);
        let script = "click\nclick right down\nclick right up\ndblclick\nwheel 0 -3\nwheel 0 -40 px\n\
                      key a\ntype héllo\nscale 1.5\nsettle 200\nwait 50\n";
        let lines = run_script(&mut session, script.as_bytes());
        assert_eq!(lines.len(), 11, "{lines:?}");
        assert!(lines[..9].iter().all(|line| line == "ok"), "{lines:?}");
        assert!(lines[9].starts_with("ok frames=") && lines[9].contains(" settled="), "{lines:?}");
        assert!(lines[10].starts_with("ok frames="), "{lines:?}");
        assert_eq!(session.exit_code(), 0);
    }

    #[test]
    fn headless_session_unported_commands_report_not_available() {
        let mut session = session_for_test(640, 400);
        let lines = run_script(&mut session, b"info\n");
        assert_eq!(lines, vec!["err 'info' is not available yet"]);
    }
}
