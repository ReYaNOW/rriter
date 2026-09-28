use crate::app::{ProtectedSaves, ProtectedWriter};
use crate::headless::HeadlessSession;
use crate::headless::profile::HeadlessOptions;
use crate::headless::tests_support::{dump, ensure_test_profile_root, run_script, scratch_dir, wait_until};
use crate::platform::TextFileFormat;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

fn writable_headless_session(path: &Path) -> HeadlessSession {
    let options = HeadlessOptions {
        size: (1280, 720),
        allow_writes: true,
        ..Default::default()
    };
    let mut session = match HeadlessSession::new(&options, ensure_test_profile_root()) {
        Ok(session) => session,
        Err((code, message)) => panic!("headless session (code {code}): {message}"),
    };
    session.hz_probe = || None;
    let lines = run_script(
        &mut session,
        format!("open {}\nsettle 2000\n", path.display()).as_bytes(),
    );
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
    session
}

fn protect_file(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o444))
            .unwrap_or_else(|error| panic!("make fixture read-only: {error}"));
        let parent = path.parent().expect("fixture parent");
        std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o555))
            .unwrap_or_else(|error| panic!("make fixture directory read-only: {error}"));
    }
}

fn unprotect_file(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let parent = path.parent().expect("fixture parent");
        std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o755))
            .unwrap_or_else(|error| panic!("make fixture directory writable: {error}"));
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o644))
            .unwrap_or_else(|error| panic!("make fixture writable: {error}"));
    }
}

#[test]
fn headless_protected_save_uses_injected_writer_and_marks_tab_saved() {
    let dir = scratch_dir("ui-protected-save-success");
    let path = dir.join("protected.txt");
    std::fs::write(&path, "before").expect("write fixture");
    let mut session = writable_headless_session(&path);
    protect_file(&path);
    let writes = Arc::new(Mutex::new(Vec::<(PathBuf, String)>::new()));
    let writer: ProtectedWriter = {
        let writes = Arc::clone(&writes);
        Arc::new(move |path, text, _format: TextFileFormat, _cancel| {
            writes.lock().expect("record writes").push((path.to_path_buf(), text.to_owned()));
            Ok(())
        })
    };
    session.app.protected_saves = ProtectedSaves::with_writer(writer);

    run_script(&mut session, b"type !\nkey ctrl+s\n");
    wait_until(&mut session, 5000, "injected protected save", |session| {
        let received = writes
            .lock()
            .expect("recorded writes")
            .iter()
            .any(|(written_path, text)| written_path == &path && text == "!before");
        received && dump(session)["tabs"][0]["modified"] == false
    });
    unprotect_file(&path);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_protected_save_failure_keeps_tab_dirty_and_shows_error() {
    let dir = scratch_dir("ui-protected-save-failure");
    let path = dir.join("protected.txt");
    std::fs::write(&path, "before").expect("write fixture");
    let mut session = writable_headless_session(&path);
    protect_file(&path);
    let writer: ProtectedWriter = Arc::new(|_, _, _, _| {
        Err(io::Error::new(io::ErrorKind::PermissionDenied, "fake denied"))
    });
    session.app.protected_saves = ProtectedSaves::with_writer(writer);

    run_script(&mut session, b"type !\nkey ctrl+s\n");
    wait_until(&mut session, 5000, "injected protected save failure", |session| {
        session.app.ide_panel.file_tree_error.is_some()
    });
    assert_eq!(dump(&mut session)["tabs"][0]["modified"], true);
    let error = session.app.ide_panel.file_tree_error.as_deref().unwrap_or_default();
    assert!(error.contains(path.to_string_lossy().as_ref()), "{error}");
    assert!(error.contains("fake denied"), "{error}");
    unprotect_file(&path);
    let _ = std::fs::remove_dir_all(dir);
}
