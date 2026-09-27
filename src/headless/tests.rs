//! Integration tests of the headless session on a real offscreen EGL context, plus the
//! shared fixtures (`tests_support`) later headless tasks build on.

pub(crate) mod tests_support {
    use crate::headless::HeadlessSession;
    use crate::headless::profile::HeadlessOptions;
    use std::io::{BufRead, BufReader, Cursor};
    use std::path::{Path, PathBuf};
    use std::process::{ChildStdin, Command, Stdio};
    use std::sync::mpsc;
    use std::sync::OnceLock;
    use std::time::{Duration, Instant};

    static TEST_PROFILE_ROOT: OnceLock<PathBuf> = OnceLock::new();
    const POSTGRES_FIXTURE_DATABASE: &str = "rriter_pgo";
    const POSTGRES_FIXTURE_USER: &str = "rriter_pgo";

    pub(crate) struct PostgresFixture {
        pub port: u16,
        pub database: &'static str,
        pub user: &'static str,
        child: crate::platform::ManagedChild,
        stdin: Option<ChildStdin>,
    }

    pub(crate) fn postgres_fixture() -> PostgresFixture {
        let script = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("scripts")
            .join("postgres_fixture.py");
        let mut command = Command::new("python3");
        command
            .arg(script)
            .args(["--port", "0"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped());
        let mut child = crate::platform::ManagedChild::spawn(&mut command)
            .expect("start shared PostgreSQL fixture process");
        let stdin = child
            .take_stdin()
            .expect("shared PostgreSQL fixture stdin unavailable");
        let stdout = child
            .take_stdout()
            .expect("shared PostgreSQL fixture stdout unavailable");
        let (sender, receiver) = mpsc::channel();
        std::thread::spawn(move || {
            let mut line = String::new();
            let result = BufReader::new(stdout).read_line(&mut line).map(|_| line);
            let _ = sender.send(result);
        });
        let port_line = receiver
            .recv_timeout(Duration::from_secs(10))
            .unwrap_or_else(|error| panic!("timed out waiting for PostgreSQL fixture port: {error}"))
            .expect("failed to read PostgreSQL fixture port");
        let port = port_line
            .trim()
            .parse::<u16>()
            .expect("PostgreSQL fixture did not print a valid port");
        assert_ne!(port, 0, "PostgreSQL fixture printed port zero");
        PostgresFixture {
            port,
            database: POSTGRES_FIXTURE_DATABASE,
            user: POSTGRES_FIXTURE_USER,
            child,
            stdin: Some(stdin),
        }
    }

    impl Drop for PostgresFixture {
        fn drop(&mut self) {
            self.stdin.take();
            let _ = self.child.terminate(Duration::from_millis(250));
        }
    }

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
                reset_api_test_state();
                root
            })
            .clone()
    }

    /// Under `cfg(test)` the API client and API Mock state ignore the profile root and live
    /// in fixed temp dirs shared by every test process (`api_config_dir`, `api_mock_data_dir`).
    /// App-level API tests persist a URL spec and mock routes there, and `workspace` loads
    /// them back, so headless tests start from the defaults instead of the last writer.
    pub(crate) fn reset_api_test_state() {
        let api_dir = std::env::temp_dir().join("rriter_api_client_tests");
        for file in ["api_specs.json", "api_auth.json"] {
            let _ = std::fs::remove_file(api_dir.join(file));
        }
        let _ = std::fs::remove_file(crate::app::api_mock::persist::api_mocks_path());
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

    pub(crate) fn open_file_session(w: u32, h: u32, scale: f32, path: &Path) -> HeadlessSession {
        let mut session = session_for_test(w, h);
        let lines = run_script(
            &mut session,
            format!("scale {scale}\nopen {}\nsettle 2000\n", path.display()).as_bytes(),
        );
        assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
        session
    }

    pub(crate) fn workspace_with_explorer(
        w: u32,
        h: u32,
        scale: f32,
        dir: &Path,
    ) -> HeadlessSession {
        let mut session = session_for_test(w, h);
        let lines = run_script(
            &mut session,
            format!("scale {scale}\nworkspace {}\nsettle 2000\n", dir.display()).as_bytes(),
        );
        assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");

        let state = dump(&mut session);
        let explorer_open = state["ide_panel"]["open"]
            .as_array()
            .is_some_and(|panels| panels.iter().any(|panel| panel == "explorer"));
        if !explorer_open {
            click_ui(&mut session, "SidebarSlot(Explorer)");
        }
        let lines = run_script(&mut session, b"settle 2000\n");
        assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
        session
    }

    pub(crate) fn scratch_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir()
            .join(format!("rriter-headless-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create scratch dir");
        dir
    }

    pub(crate) fn git(dir: &Path, args: &[&str]) -> String {
        let output = std::process::Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(args)
            .output()
            .expect("run git");
        assert!(
            output.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).into_owned()
    }

    pub(crate) fn git_init(dir: &Path) {
        git(dir, &["init", "-q"]);
        git(dir, &["config", "user.name", "Headless Test"]);
        git(dir, &["config", "user.email", "headless@example.invalid"]);
        git(dir, &["config", "commit.gpgsign", "false"]);
    }

    /// Git repo with one commit, then a modified, a deleted and an untracked file.
    pub(crate) fn git_fixture(dir: &Path) {
        std::fs::write(dir.join("changed.txt"), "before\n").unwrap();
        std::fs::write(dir.join("deleted.txt"), "delete me\n").unwrap();
        git_init(dir);
        git(dir, &["add", "."]);
        git(dir, &["commit", "-qm", "fixture"]);
        std::fs::write(dir.join("changed.txt"), "after\n").unwrap();
        std::fs::remove_file(dir.join("deleted.txt")).unwrap();
        std::fs::write(dir.join("untracked.txt"), "new\n").unwrap();
    }

    pub(crate) fn run_script(session: &mut HeadlessSession, script: &[u8]) -> Vec<String> {
        let mut out = Vec::new();
        assert!(session.run_loop(Cursor::new(script.to_vec()), &mut out));
        String::from_utf8(out)
            .expect("utf-8 responses")
            .lines()
            .map(str::to_string)
            .collect()
    }

    pub(crate) fn wait_until(
        session: &mut HeadlessSession,
        timeout_ms: u64,
        what: &str,
        mut done: impl FnMut(&mut HeadlessSession) -> bool,
    ) {
        let started = Instant::now();
        while started.elapsed() < Duration::from_millis(timeout_ms) {
            let lines = run_script(session, b"wait 50\n");
            assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
            if done(session) {
                return;
            }
        }
        panic!("timed out waiting for {what}; last dump: {}", dump(session));
    }

    pub(crate) fn sample_file(dir: &Path) -> PathBuf {
        let path = dir.join("sample.txt");
        let mut text = String::new();
        for i in 0..40 {
            text.push_str(&format!("line {i:02} headless screenshot content\n"));
        }
        std::fs::write(&path, text).expect("write sample file");
        path
    }

    pub(crate) fn dump(session: &mut HeadlessSession) -> serde_json::Value {
        let lines = run_script(session, b"dump\n");
        let payload = lines[0].strip_prefix("ok ").unwrap_or_else(|| panic!("{lines:?}"));
        serde_json::from_str(payload).expect("dump payload is JSON")
    }

    pub(crate) fn ui_center(dump: &serde_json::Value, id: &str) -> (f64, f64) {
        let [x, y, width, height] = ui_rect(dump, id);
        (x + width / 2.0, y + height / 2.0)
    }

    pub(crate) fn ui_rect(dump: &serde_json::Value, id: &str) -> [f64; 4] {
        let element = dump["ui"]
            .as_array()
            .unwrap()
            .iter()
            .find(|element| element["id"] == id)
            .unwrap_or_else(|| panic!("no {id} in ui"));
        let rect = element["rect"].as_array().unwrap();
        [
            rect[0].as_f64().unwrap(),
            rect[1].as_f64().unwrap(),
            rect[2].as_f64().unwrap(),
            rect[3].as_f64().unwrap(),
        ]
    }

    pub(crate) fn assert_rect_inside_window(
        [x, y, width, height]: [f64; 4],
        window_width: f64,
        window_height: f64,
        start_tolerance: f64,
        end_tolerance: f64,
        label: &str,
    ) {
        assert!(
            x >= -start_tolerance
                && y >= -start_tolerance
                && x + width <= window_width + end_tolerance
                && y + height <= window_height + end_tolerance,
            "{label} outside {window_width}x{window_height}: [{x}, {y}, {width}, {height}]"
        );
    }

    pub(crate) fn assert_ui_rect_inside_window(dump: &serde_json::Value, id: &str) {
        let [x, y, width, height] = ui_rect(dump, id);
        let window_width = dump["size"][0].as_f64().unwrap();
        let window_height = dump["size"][1].as_f64().unwrap();
        assert!(
            x >= 0.0 && y >= 0.0 && width > 0.0 && height > 0.0,
            "{id} has invalid rect [{x}, {y}, {width}, {height}]"
        );
        assert_rect_inside_window(
            [x, y, width, height],
            window_width,
            window_height,
            0.0,
            0.5,
            id,
        );
    }

    pub(crate) fn assert_ui_y_integral(y: f64, tolerance: f64, label: &str) {
        assert!((y - y.round()).abs() <= tolerance, "{label} has fractional y={y}");
    }

    pub(crate) fn click_ui(session: &mut HeadlessSession, id: &str) {
        let (x, y) = ui_center(&dump(session), id);
        let lines = run_script(session, format!("mouse_move {x} {y}\nclick\n").as_bytes());
        assert!(lines.iter().all(|line| line == "ok"), "{lines:?}");
    }

    /// Point at (`x_fraction`, `y_fraction`) of the `id` hitbox, `(0, 0)` is its top-left.
    pub(crate) fn ui_point(
        dump: &serde_json::Value,
        id: &str,
        x_fraction: f64,
        y_fraction: f64,
    ) -> (f64, f64) {
        let [x, y, width, height] = ui_rect(dump, id);
        (x + width * x_fraction, y + height * y_fraction)
    }

    /// `click_ui` at a fractional point of the hitbox instead of its center.
    pub(crate) fn click_ui_fraction(
        session: &mut HeadlessSession,
        id: &str,
        x_fraction: f64,
        y_fraction: f64,
    ) -> (f64, f64) {
        let (x, y) = ui_point(&dump(session), id, x_fraction, y_fraction);
        let lines = run_script(session, format!("mouse_move {x} {y}\nclick\n").as_bytes());
        assert!(lines.iter().all(|line| line == "ok"), "{lines:?}");
        (x, y)
    }

    /// Replaces the text of a Database connection dialog field (`DatabaseFormField` name).
    pub(crate) fn set_database_dialog_field(session: &mut HeadlessSession, field: &str, value: &str) {
        let id = format!("DatabaseDialogField({field})");
        let state = dump(session);
        let is_input = state["ui"].as_array().is_some_and(|ui| {
            ui.iter().any(|element| element["id"] == id.as_str() && element["kind"] == "TextInput")
        });
        assert!(is_input, "missing database text input {id}: {}", state["ui"]);
        let (x, y) = ui_center(&state, &id);
        let lines = run_script(
            session,
            format!("mouse_move {x} {y}\nclick\nkey ctrl+a\ntype {value}\n").as_bytes(),
        );
        assert!(lines.iter().all(|line| line == "ok"), "{lines:?}");
    }

    /// Adds a PostgreSQL connection to the fixture credentials through the Database panel
    /// form and returns its index. Connections persist in the shared test profile, so the
    /// index is looked up by `display_name`, which callers keep unique per test.
    pub(crate) fn add_database_connection_through_ui(
        session: &mut HeadlessSession,
        display_name: &str,
        port: u16,
    ) -> usize {
        click_ui(session, "SidebarSlot(Database)");
        click_ui(session, "DatabaseAdd");
        set_database_dialog_field(session, "DisplayName", display_name);
        set_database_dialog_field(session, "Host", "127.0.0.1");
        set_database_dialog_field(session, "Port", &port.to_string());
        set_database_dialog_field(session, "Username", POSTGRES_FIXTURE_USER);
        set_database_dialog_field(session, "PostgresPassword", "fixture");
        set_database_dialog_field(session, "MaintenanceDatabase", POSTGRES_FIXTURE_DATABASE);
        click_ui(session, "DatabaseDialogSave");
        let find = |session: &HeadlessSession| {
            session
                .app
                .ide_panel
                .database
                .connections
                .iter()
                .position(|connection| connection.config.display_name == display_name)
        };
        wait_until(session, 5000, "saved Database connection", |session| {
            find(session).is_some_and(|index| {
                has_ui(&dump(session), &format!("DatabaseConnectionRow({index})"))
            })
        });
        find(session).unwrap()
    }

    /// Adds a connection to `fixture` through the UI, expands it and waits until the
    /// fixture database is in the loaded catalog. Returns the connection index.
    pub(crate) fn connect_postgres_fixture_through_ui(
        session: &mut HeadlessSession,
        fixture: &PostgresFixture,
        display_name: &str,
    ) -> usize {
        let index = add_database_connection_through_ui(session, display_name, fixture.port);
        click_ui(session, &format!("DatabaseConnectionArrow({index})"));
        wait_until(session, 8000, "PostgreSQL fixture database catalog", |session| {
            session.app.ide_panel.database.connections.get(index).is_some_and(|connection| {
                connection.status == crate::app::database::DatabaseConnectionStatus::Ready
                    && connection.databases_loaded
                    && connection
                        .databases
                        .iter()
                        .any(|database| database.name == fixture.database)
            })
        });
        index
    }

    pub(crate) fn has_ui(dump: &serde_json::Value, id: &str) -> bool {
        dump["ui"].as_array().is_some_and(|ui| ui.iter().any(|element| element["id"] == id))
    }

    /// Scrolls down one wheel line at a time with the cursor at (`x`, `y`) until `id` is
    /// registered with a hitbox at least `min_height` px tall (an element clipped by the
    /// viewport edge registers only a sliver). `settle` lets the smooth scroll finish, so
    /// the next click is not dropped by panels that disable interactions while scrolling.
    pub(crate) fn wheel_until_visible(
        session: &mut HeadlessSession,
        (x, y): (f64, f64),
        id: &str,
        min_height: f64,
        max_steps: usize,
    ) -> bool {
        for step in 0..=max_steps {
            let state = dump(session);
            if has_ui(&state, id) && ui_rect(&state, id)[3] >= min_height {
                return true;
            }
            if step == max_steps {
                break;
            }
            let lines = run_script(
                session,
                format!("mouse_move {x} {y}\nwheel 0 -1\nsettle 2000\n").as_bytes(),
            );
            assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
        }
        false
    }

    pub(crate) fn long_file(dir: &Path) -> PathBuf {
        let path = dir.join("long.txt");
        let text: String = (0..600).map(|i| format!("line {i:04} of a long bench document\n")).collect();
        std::fs::write(&path, text).expect("write long file");
        path
    }

    pub(crate) fn ok_json(line: &str) -> serde_json::Value {
        let payload = line.strip_prefix("ok ").unwrap_or_else(|| panic!("not ok: {line}"));
        serde_json::from_str(payload).expect("json payload")
    }
}

mod postgres_fixture_cases {
    use crate::headless::tests_support::postgres_fixture;
    use std::net::{SocketAddr, TcpStream};
    use std::thread;
    use std::time::{Duration, Instant};

    #[test]
    fn postgres_fixture_starts_and_accepts_tcp() {
        let fixture = postgres_fixture();
        assert_eq!(fixture.database, "rriter_pgo");
        assert_eq!(fixture.user, "rriter_pgo");
        let address = SocketAddr::from(([127, 0, 0, 1], fixture.port));
        assert!(TcpStream::connect(address).is_ok(), "fixture rejected TCP connection");

        drop(fixture);
        let deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < deadline {
            if TcpStream::connect_timeout(&address, Duration::from_millis(50)).is_err() {
                return;
            }
            thread::sleep(Duration::from_millis(25));
        }
        panic!("PostgreSQL fixture still accepts connections after drop");
    }
}

mod session_cases {
    use crate::headless::tests_support::{
        dump, long_file, ok_json, run_script, sample_file, scratch_dir, session_for_test,
        ui_center,
    };
    use crate::app::events::host_loop::HostLoop;
    use crate::headless::HeadlessSession;
    use std::io::{self, Cursor, Write};
    use std::path::PathBuf;

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
    fn headless_session_unreadable_input_is_an_error_exit() {
        let mut session = session_for_test(640, 400);
        // Reading a directory fails with EISDIR, like `--script <dir>`.
        let dir = std::fs::File::open(scratch_dir("unreadable-input")).expect("open dir");
        assert!(!session.run_loop(io::BufReader::new(dir), Vec::new()));
        assert_eq!(session.exit_code(), 1);
    }

    #[test]
    fn headless_session_idle_settle_converges_without_frames() {
        let mut session = session_for_test(640, 400);
        let lines = run_script(&mut session, b"settle 2000\nsettle 2000\n");
        assert_eq!(lines.len(), 2, "{lines:?}");
        assert!(lines[0].ends_with(" settled=true"), "{lines:?}");
        assert_eq!(lines[1], "ok frames=0 settled=true", "{lines:?}");
    }

    /// Workspace with one dirty file tab, in IDE mode where Ctrl+4 asks before closing.
    fn dirty_ide_tab(name: &str) -> (PathBuf, PathBuf, HeadlessSession) {
        let dir = scratch_dir(name);
        let file = sample_file(&dir);
        let mut session = session_for_test(1280, 800);
        let script = format!("workspace {}\nopen {}\ntype xyz\n", dir.display(), file.display());
        let lines = run_script(&mut session, script.as_bytes());
        assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
        (dir, file, session)
    }

    #[test]
    fn headless_dump_has_all_keys_and_welcome_mode() {
        let mut session = session_for_test(640, 400);
        // `ui` is the last frame's registry: draw one first.
        run_script(&mut session, b"mouse_move 1 1\n");
        let dump = dump(&mut session);
        let mut keys: Vec<&str> = dump.as_object().unwrap().keys().map(String::as_str).collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            [
                "clipboard", "cursor_icon", "dialog", "editor", "external_request", "hover", "ide_panel", "mode",
                "overlays", "scale", "size", "tabs", "ui", "writes_allowed"
            ]
        );
        assert_eq!(dump["mode"], "welcome");
        assert_eq!(dump["size"], serde_json::json!([640, 400]));
        assert_eq!(dump["clipboard"], serde_json::json!({"mode": "memory", "text": null}));
        assert_eq!(dump["dialog"], serde_json::Value::Null);
        assert!(!dump["ui"].as_array().unwrap().is_empty());
    }

    #[test]
    fn headless_dump_to_file_and_io_error() {
        let dir = scratch_dir("dump-file");
        let out = dir.join("nested").join("d.json");
        let mut session = session_for_test(640, 400);
        let lines = run_script(&mut session, format!("dump {}\ndump /proc/rriter-nope/d.json\n", out.display()).as_bytes());
        assert_eq!(lines[0], format!("ok {}", out.display()));
        assert!(lines[1].starts_with("err io: "), "{lines:?}");
        let text = std::fs::read_to_string(&out).expect("dump file");
        assert_eq!(serde_json::from_str::<serde_json::Value>(&text).unwrap()["mode"], "welcome");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn headless_dump_tracks_opened_tab() {
        let dir = scratch_dir("dump-tab");
        let file = sample_file(&dir);
        let mut session = session_for_test(640, 400);
        run_script(&mut session, format!("open {}\n", file.display()).as_bytes());
        let dump = dump(&mut session);
        assert_eq!(dump["mode"], "editor");
        let tabs = dump["tabs"].as_array().unwrap();
        let tab = tabs.iter().find(|tab| tab["path"] == file.display().to_string()).expect("opened tab");
        assert_eq!(tab["active"], true);
        assert_eq!(tab["modified"], false);
        assert_eq!(dump["editor"]["lines"], 41);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn headless_dump_ui_rect_click_and_settings_key() {
        let dir = scratch_dir("dump-click");
        let file = dir.join("doc.md");
        std::fs::write(&file, "# Title\n\nBody text.\n").expect("write markdown");
        let mut session = session_for_test(1280, 800);
        run_script(&mut session, format!("workspace {}\nopen {}\n", dir.display(), file.display()).as_bytes());
        let before = dump(&mut session);
        assert_eq!(before["mode"], "ide");
        let markdown = |dump: &serde_json::Value| {
            dump["tabs"].as_array().unwrap().iter().find(|tab| tab["active"] == true).unwrap()["markdown"].clone()
        };
        assert_eq!(markdown(&before), false);
        // The settings overlay has no button, only F1; a click is checked on the markdown toggle.
        let (x, y) = ui_center(&before, "MarkdownModeToggle");
        let lines = run_script(&mut session, format!("mouse_move {x} {y}\nclick\n").as_bytes());
        assert_eq!(lines, ["ok", "ok"]);
        assert_eq!(markdown(&dump(&mut session)), true);
        run_script(&mut session, b"key f1\n");
        assert_eq!(dump(&mut session)["overlays"]["settings"], true);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn headless_dialog_cancel_then_discard_close_tab() {
        let (dir, file, mut session) = dirty_ide_tab("dialog-discard");
        let shot = dir.join("before.png");
        run_script(&mut session, format!("screenshot {}\nkey ctrl+4\n", shot.display()).as_bytes());
        let open = dump(&mut session);
        let dialog = &open["dialog"];
        assert_eq!(dialog["action"], "CloseTab", "{open}");
        let buttons = dialog["buttons"].as_array().unwrap();
        let names: Vec<&str> = buttons.iter().map(|button| button["name"].as_str().unwrap()).collect();
        assert_eq!(names, ["save", "discard", "cancel"]);
        for button in buttons {
            let r: Vec<f64> = button["rect"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect();
            assert!(r[0] >= 0.0 && r[1] >= 0.0 && r[2] > 0.0 && r[3] > 0.0, "{button}");
            assert!(r[0] + r[2] <= 1280.0 && r[1] + r[3] <= 800.0, "{button}");
        }
        // The dialog is drawn into the frame: the buffer center changes.
        let with_dialog = dir.join("dialog.png");
        run_script(&mut session, format!("screenshot {}\n", with_dialog.display()).as_bytes());
        let center = |path: &PathBuf| image::open(path).expect("decode").to_rgba8().get_pixel(640, 400).0;
        assert_ne!(center(&shot), center(&with_dialog));

        assert_eq!(run_script(&mut session, b"dialog cancel\n"), ["ok"]);
        let cancelled = dump(&mut session);
        assert_eq!(cancelled["dialog"], serde_json::Value::Null);
        assert_eq!(cancelled["tabs"].as_array().unwrap().len(), 1);

        assert_eq!(run_script(&mut session, b"key ctrl+4\ndialog discard\n"), ["ok", "ok"]);
        let discarded = dump(&mut session);
        assert_eq!(discarded["dialog"], serde_json::Value::Null);
        assert!(discarded["tabs"].as_array().unwrap().iter().all(|tab| tab["path"] != file.display().to_string()));
        assert!(!std::fs::read_to_string(&file).unwrap().contains("xyz"));
        assert_eq!(session.exit_code(), 0);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn headless_dialog_save_writes_file() {
        let (dir, file, mut session) = dirty_ide_tab("dialog-save");
        assert_eq!(run_script(&mut session, b"key ctrl+4\ndialog save\n"), ["ok", "ok"]);
        assert_eq!(dump(&mut session)["dialog"], serde_json::Value::Null);
        assert!(std::fs::read_to_string(&file).unwrap().contains("xyz"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn headless_dialog_without_dialog_errs() {
        let mut session = session_for_test(640, 400);
        assert_eq!(run_script(&mut session, b"dialog save\n"), ["err no dialog"]);
        assert_eq!(session.exit_code(), 1);
    }

    #[test]
    fn headless_dialog_escape_closes_like_window() {
        // Editor mode: Ctrl+Q on a dirty file asks with CloseFile, which Escape does not reopen.
        let dir = scratch_dir("dialog-escape");
        let file = sample_file(&dir);
        let mut session = session_for_test(1280, 800);
        run_script(&mut session, format!("open {}\ntype xyz\nkey ctrl+q\n", file.display()).as_bytes());
        assert_eq!(dump(&mut session)["dialog"]["action"], "CloseFile");
        assert_eq!(run_script(&mut session, b"key escape\n"), ["ok"]);
        assert_eq!(dump(&mut session)["dialog"], serde_json::Value::Null);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn headless_record_frames_csv_and_motion() {
        let dir = scratch_dir("record");
        let path = long_file(&dir);
        let out = dir.join("rec");
        let mut session = session_for_test(640, 400);
        let script = format!("open {}\nrecord 3 {} wheel 0 -3\n", path.display(), out.display());
        let lines = run_script(&mut session, script.as_bytes());
        assert_eq!(lines.len(), 2, "{lines:?}");
        let summary = ok_json(&lines[1]);
        assert_eq!(summary["frames"], 3);
        assert_eq!(summary["csv"], out.join("frames.csv").display().to_string());
        for key in ["scroll_y_delta_min", "scroll_y_delta_max", "nonmonotonic_frames"] {
            assert!(summary["motion"].get(key).is_some(), "missing motion.{key}: {summary}");
        }
        for frame in 0..3 {
            let png = out.join(format!("frame-{frame:04}.png"));
            assert_eq!(image::image_dimensions(&png).expect("png written"), (640, 400));
        }
        assert!(!out.join("frame-0003.png").exists());
        let text = std::fs::read_to_string(out.join("frames.csv")).expect("csv written");
        let rows: Vec<&str> = text.lines().collect();
        assert_eq!(rows.len(), 4, "{text}");
        assert!(rows[0].ends_with(",scroll_y,sticky_anim_progress,search_anim_y,tab_scroll"), "{}", rows[0]);
        assert!(rows.iter().all(|row| row.split(',').count() == rows[0].split(',').count()), "{text}");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn headless_record_io_error_then_continues() {
        let mut session = session_for_test(640, 400);
        let lines = run_script(&mut session, b"record 3 /proc/rriter-nope\ndump\n");
        assert_eq!(lines.len(), 2, "{lines:?}");
        assert!(lines[0].starts_with("err io:"), "{lines:?}");
        assert!(lines[1].starts_with("ok {"), "{lines:?}");
    }

    #[test]
    fn headless_bench_session_csv_summary_and_telemetry_restored() {
        use std::sync::atomic::Ordering;
        let dir = scratch_dir("bench-csv");
        let csv = dir.join("a.csv");
        let mut session = session_for_test(640, 400);
        let before = crate::render_view::TELEMETRY_ENABLED.load(Ordering::Relaxed);
        let lines = run_script(&mut session, format!("bench 5 csv={}\n", csv.display()).as_bytes());
        assert_eq!(crate::render_view::TELEMETRY_ENABLED.load(Ordering::Relaxed), before);
        assert_eq!(lines.len(), 1, "{lines:?}");
        let summary = ok_json(&lines[0]);
        assert_eq!(summary["frames"], 5);
        assert_eq!(summary["hz_source"], "default");
        assert_eq!(summary["hz"], 240.0);
        assert_eq!(summary["csv"], csv.display().to_string());
        for key in ["budget_ms", "total_ms", "gpu_ms", "draw_cpu_ms", "over_budget", "worst", "system"] {
            assert!(summary.get(key).is_some(), "missing {key}: {summary}");
        }
        assert_eq!(summary["worst"].as_array().map(Vec::len), Some(5));
        for key in ["loadavg_before", "loadavg_after", "cpus", "gpu_util_before", "gpu_util_after", "process_cpu_ms"] {
            assert!(summary["system"].get(key).is_some(), "missing system.{key}: {summary}");
        }
        let text = std::fs::read_to_string(&csv).expect("csv written");
        let rows: Vec<&str> = text.lines().collect();
        assert_eq!(rows.len(), 6, "{text}");
        assert!(rows[0].starts_with("frame,update_ms,draw_cpu_ms,gpu_ms,total_ms,"), "{}", rows[0]);
        assert!(rows[0].ends_with(",scroll_y"), "{}", rows[0]);
        assert!(rows[1].starts_with("0,") && rows[5].starts_with("4,"), "{text}");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn headless_bench_wheel_scrolls_long_document() {
        let dir = scratch_dir("bench-wheel");
        let path = long_file(&dir);
        let csv = dir.join("wheel.csv");
        let mut session = session_for_test(640, 400);
        let script = format!("open {}\nbench 3 csv={} wheel 0 -3\n", path.display(), csv.display());
        let lines = run_script(&mut session, script.as_bytes());
        assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
        let text = std::fs::read_to_string(&csv).expect("csv written");
        let scroll: Vec<f64> = text
            .lines()
            .skip(1)
            .map(|row| row.rsplit(',').next().and_then(|v| v.parse().ok()).expect("scroll_y column"))
            .collect();
        assert_eq!(scroll.len(), 3, "{text}");
        assert!(scroll[2] > scroll[0], "wheel must scroll: {scroll:?}");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn headless_bench_csv_io_error_then_continues() {
        let mut session = session_for_test(640, 400);
        let lines = run_script(&mut session, b"bench 2 csv=/proc/rriter-nope/a.csv\ndump\n");
        assert_eq!(lines.len(), 2, "{lines:?}");
        assert!(lines[0].starts_with("err io:"), "{lines:?}");
        assert!(lines[1].starts_with("ok {"), "{lines:?}");
        assert!(session.had_error);
    }

    #[test]
    fn headless_info_reports_budget_gl_and_policy() {
        let mut session = session_for_test(640, 400);
        let lines = run_script(&mut session, b"info\n");
        assert_eq!(lines.len(), 1, "{lines:?}");
        let info = ok_json(&lines[0]);
        for key in ["hz", "budget_ms", "hz_source", "gl_renderer", "gl_version", "gl_vendor", "writes_allowed", "profile"] {
            assert!(info.get(key).is_some(), "missing {key}: {info}");
        }
        assert_eq!(info["hz_source"], "default");
        assert_eq!(info["profile"], session.profile_root.display().to_string());
        assert!(info["gl_version"].as_str().is_some_and(|v| !v.is_empty()), "{info}");
    }
}

mod protocol_fd_cases {
    use crate::headless::split_protocol_fd;
    use std::fs::File;
    use std::io::{Read, Write};
    use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};

    fn pipe() -> (File, File) {
        let mut fds = [0; 2];
        assert_eq!(unsafe { libc::pipe2(fds.as_mut_ptr(), libc::O_CLOEXEC) }, 0);
        unsafe { (File::from(OwnedFd::from_raw_fd(fds[0])), File::from(OwnedFd::from_raw_fd(fds[1]))) }
    }

    #[test]
    fn headless_protocol_fd_keeps_replies_and_moves_stray_output() {
        let (mut protocol_read, protocol_write) = pipe();
        let (mut stray_read, stray_write) = pipe();
        // `protocol_write` plays fd 1, `stray_write` plays fd 2.
        let mut protocol = split_protocol_fd(protocol_write.as_raw_fd(), stray_write.as_raw_fd())
            .expect("split protocol fd");
        writeln!(protocol, "ok").expect("write reply");
        let mut former_stdout = &protocol_write;
        writeln!(former_stdout, "[GIT status] ok").expect("write stray line");
        drop((protocol, protocol_write, stray_write));
        let (mut replies, mut stray) = (String::new(), String::new());
        protocol_read.read_to_string(&mut replies).expect("read replies");
        stray_read.read_to_string(&mut stray).expect("read stray");
        assert_eq!(replies, "ok\n");
        assert_eq!(stray, "[GIT status] ok\n");
    }
}
