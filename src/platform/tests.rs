use super::*;
use std::collections::HashMap;

fn paths_for(platform: PlatformKind, values: &[(&str, &str)]) -> AppPaths {
    let values = values
        .iter()
        .map(|(key, value)| ((*key).to_string(), OsString::from(value)))
        .collect::<HashMap<_, _>>();
    app_paths_with(platform, |name| values.get(name).cloned())
}

#[test]
fn platform_directories_follow_native_conventions() {
    let linux = paths_for(
        PlatformKind::Linux,
        &[
            ("HOME", "/home/user"),
            ("XDG_CONFIG_HOME", "/cfg"),
            ("XDG_DATA_HOME", "/data"),
            ("XDG_CACHE_HOME", "/cache"),
            ("XDG_STATE_HOME", "/state"),
        ],
    );
    assert_eq!(linux.config, PathBuf::from("/cfg/RRiter"));
    assert_eq!(linux.data, PathBuf::from("/data/RRiter"));
    assert_eq!(linux.cache, PathBuf::from("/cache/RRiter"));
    assert_eq!(linux.state, PathBuf::from("/state/RRiter"));

    let windows = paths_for(
        PlatformKind::Windows,
        &[
            ("USERPROFILE", r"C:\Users\Reyan"),
            ("APPDATA", r"C:\Users\Reyan\AppData\Roaming"),
            ("LOCALAPPDATA", r"C:\Users\Reyan\AppData\Local"),
        ],
    );
    assert_eq!(
        windows.config,
        PathBuf::from(r"C:\Users\Reyan\AppData\Roaming").join("RRiter")
    );
    assert_eq!(
        windows.cache,
        PathBuf::from(r"C:\Users\Reyan\AppData\Local")
            .join("RRiter")
            .join("cache")
    );

    let macos = paths_for(PlatformKind::Macos, &[("HOME", "/Users/reyan")]);
    assert_eq!(
        macos.config,
        PathBuf::from("/Users/reyan/Library/Application Support/RRiter")
    );
    assert_eq!(
        macos.cache,
        PathBuf::from("/Users/reyan/Library/Caches/RRiter")
    );
}

#[test]
fn user_cache_root_follows_tool_cache_conventions() {
    let values = [
        ("USERPROFILE".to_string(), OsString::from(r"C:\Users\Reyan")),
        (
            "LOCALAPPDATA".to_string(),
            OsString::from(r"C:\Users\Reyan\AppData\Local"),
        ),
    ]
    .into_iter()
    .collect::<HashMap<_, _>>();
    assert_eq!(
        user_cache_root_with(PlatformKind::Windows, |name| values.get(name).cloned()),
        PathBuf::from(r"C:\Users\Reyan\AppData\Local")
    );

    let values = [("HOME".to_string(), OsString::from("/Users/reyan"))]
        .into_iter()
        .collect::<HashMap<_, _>>();
    assert_eq!(
        user_cache_root_with(PlatformKind::Macos, |name| values.get(name).cloned()),
        PathBuf::from("/Users/reyan/Library/Caches")
    );

    let values = [
        ("HOME".to_string(), OsString::from("/home/reyan")),
        ("XDG_CACHE_HOME".to_string(), OsString::from("/cache")),
    ]
    .into_iter()
    .collect::<HashMap<_, _>>();
    assert_eq!(
        user_cache_root_with(PlatformKind::Linux, |name| values.get(name).cloned()),
        PathBuf::from("/cache")
    );
}

#[test]
fn windows_path_keys_handle_drive_unc_case_and_extended_prefixes() {
    let key = |path: &str| PathKey::for_platform(Path::new(path), PlatformKind::Windows);
    assert_eq!(key(r"C:\Work\RRiter\src\main.rs"), key(r"c:/work/rriter/src/MAIN.rs"));
    assert_eq!(
        key(r"\\?\C:\Work\RRiter\src\.\main.rs"),
        key(r"c:\work\rriter\src\main.rs")
    );
    assert_eq!(
        key(r"\\?\UNC\Server\Share\Project\file.py"),
        key(r"\\server\share\project\FILE.py")
    );
    assert_ne!(key(r"C:\Work\one"), key(r"C:\Work\one-more"));
    assert_eq!(
        key(r"C:\ПРОЕКТ\ФАЙЛ.rs"),
        key(r"c:\проект\файл.RS")
    );
}

#[test]
fn containment_checks_component_boundaries_on_all_platforms() {
    assert!(path_is_within_for_platform(
        Path::new("/work/project/src/main.rs"),
        Path::new("/work/project"),
        PlatformKind::Linux,
    ));
    assert!(!path_is_within_for_platform(
        Path::new("/work/project-old/main.rs"),
        Path::new("/work/project"),
        PlatformKind::Linux,
    ));
    assert!(path_is_within_for_platform(
        Path::new(r"C:\WORK\Project\src\main.rs"),
        Path::new(r"c:\work\project"),
        PlatformKind::Windows,
    ));
    assert!(!path_is_within_for_platform(
        Path::new(r"C:\work\project-old\main.rs"),
        Path::new(r"C:\work\project"),
        PlatformKind::Windows,
    ));
}

#[test]
fn persisted_paths_roundtrip_and_legacy_lines_still_load() {
    let unix = PathBuf::from("/tmp/проект/file with spaces.py");
    let encoded = encode_persisted_path_for_platform(&unix, PlatformKind::Linux);
    assert_eq!(decode_persisted_path(&encoded), Some(unix.clone()));

    let windows = PathBuf::from(r"C:\Users\Reyan\Проект\file with spaces.py");
    let encoded = encode_persisted_path_for_platform(&windows, PlatformKind::Windows);
    assert_eq!(decode_persisted_path(&encoded), Some(windows.clone()));
    assert_eq!(
        decode_persisted_path("/legacy/path.rs"),
        Some(PathBuf::from("/legacy/path.rs"))
    );
    assert_eq!(decode_persisted_path("rriter-path-v1:u:xyz"), None);
}

#[cfg(unix)]
#[test]
fn unix_persisted_paths_and_relative_paths_preserve_non_utf8_bytes() {
    use std::os::unix::ffi::OsStringExt;

    let root = PathBuf::from(OsString::from_vec(b"/tmp/rriter-\xff".to_vec()));
    let path = root.join(OsString::from_vec(b"child\n\xfe.rs".to_vec()));
    let encoded = encode_persisted_path(&path);
    assert_eq!(decode_persisted_path(&encoded), Some(path.clone()));
    assert_eq!(
        relative_to(&path, &root),
        Some(PathBuf::from(OsString::from_vec(b"child\n\xfe.rs".to_vec())))
    );
}

#[test]
fn text_formats_roundtrip_bom_utf16_and_line_endings() {
    let text = "first\nПривет 🌍\n";
    for format in [
        TextFileFormat {
            encoding: TextEncoding::Utf8,
            line_ending: LineEnding::Lf,
        },
        TextFileFormat {
            encoding: TextEncoding::Utf8Bom,
            line_ending: LineEnding::CrLf,
        },
        TextFileFormat {
            encoding: TextEncoding::Utf16Le,
            line_ending: LineEnding::CrLf,
        },
        TextFileFormat {
            encoding: TextEncoding::Utf16Be,
            line_ending: LineEnding::Cr,
        },
    ] {
        let decoded = decode_text_bytes(&encode_text(text, format)).unwrap();
        assert_eq!(decoded.text, text);
        assert_eq!(decoded.format, format);
    }
}

#[test]
fn mixed_line_endings_choose_dominant_style_and_normalize_editor_text() {
    let decoded = decode_text_bytes(b"a\r\nb\r\nc\nd\r").unwrap();
    assert_eq!(decoded.text, "a\nb\nc\nd\n");
    assert_eq!(decoded.format.line_ending, LineEnding::CrLf);
}

#[test]
fn invalid_utf8_is_reported_instead_of_lossily_rewritten() {
    let error = decode_text_bytes(&[0xff, 0x00, 0x80]).unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::InvalidData);
}

#[test]
fn text_save_does_not_create_a_missing_parent_directory() {
    let root = std::env::temp_dir().join(format!(
        "rriter-platform-missing-parent-{}-{}",
        std::process::id(),
        TEMP_FILE_COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    let path = root.join("missing").join("document.txt");
    let error = write_text_file(&path, "payload", TextFileFormat::default()).unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::NotFound);
    assert!(!root.exists());
}

#[test]
fn atomic_write_replaces_content_and_leaves_no_temp_file() {
    let root = std::env::temp_dir().join(format!(
        "rriter-platform-atomic-{}-{}",
        std::process::id(),
        TEMP_FILE_COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&root).unwrap();
    let path = root.join("document.txt");
    fs::write(&path, b"old").unwrap();
    atomic_write(&path, b"new").unwrap();
    assert_eq!(fs::read(&path).unwrap(), b"new");
    assert_eq!(fs::read_dir(&root).unwrap().count(), 1);
    let _ = fs::remove_dir_all(root);
}

#[cfg(unix)]
#[test]
fn atomic_write_supports_non_utf8_target_names() {
    use std::os::unix::ffi::OsStringExt;

    let root = std::env::temp_dir().join(format!(
        "rriter-platform-atomic-bytes-{}-{}",
        std::process::id(),
        TEMP_FILE_COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&root).unwrap();
    let path = root.join(OsString::from_vec(b"document-\xff.txt".to_vec()));
    atomic_write(&path, b"payload").unwrap();
    assert_eq!(fs::read(&path).unwrap(), b"payload");
    assert_eq!(fs::read_dir(&root).unwrap().count(), 1);
    let _ = fs::remove_dir_all(root);
}

#[cfg(unix)]
#[test]
fn atomic_write_through_symlink_preserves_the_link() {
    let root = std::env::temp_dir().join(format!(
        "rriter-platform-atomic-symlink-{}-{}",
        std::process::id(),
        TEMP_FILE_COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&root).unwrap();
    let target = root.join("target.txt");
    let link = root.join("link.txt");
    fs::write(&target, b"old").unwrap();
    std::os::unix::fs::symlink(&target, &link).unwrap();

    atomic_write(&link, b"new").unwrap();

    assert!(fs::symlink_metadata(&link).unwrap().file_type().is_symlink());
    assert_eq!(fs::read(&target).unwrap(), b"new");
    let _ = fs::remove_dir_all(root);
}

#[cfg(unix)]
#[test]
fn broken_symlink_is_an_existing_removable_path_entry() {
    let root = std::env::temp_dir().join(format!(
        "rriter-platform-broken-link-{}-{}",
        std::process::id(),
        TEMP_FILE_COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&root).unwrap();
    let link = root.join("broken-link");
    std::os::unix::fs::symlink(root.join("missing-target"), &link).unwrap();
    assert!(path_entry_exists(&link));
    remove_path_entry(&link).unwrap();
    assert!(!path_entry_exists(&link));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn url_opener_rejects_non_http_schemes_before_spawning() {
    for url in ["file:///tmp/secret", "javascript:alert(1)", "not a url"] {
        assert_eq!(open_url(url).unwrap_err().kind(), io::ErrorKind::InvalidInput);
    }
}

#[test]
fn windows_name_validation_covers_reserved_and_trailing_names() {
    for invalid in [
        "CON",
        "con.txt",
        "AUX.log",
        "COM1",
        "LPT9.md",
        "trailing.",
        "trailing ",
        "bad:name",
        "bad?name",
    ] {
        assert!(
            validate_child_name_for_platform(invalid, PlatformKind::Windows).is_err(),
            "{invalid} must be rejected"
        );
    }
    for valid in ["console.txt", "COM10", "LPT0", "normal name.rs", "Привет.py"] {
        assert!(
            validate_child_name_for_platform(valid, PlatformKind::Windows).is_ok(),
            "{valid} must be accepted"
        );
    }
    assert!(validate_child_name_for_platform("bad:name", PlatformKind::Linux).is_ok());
}

#[test]
fn windows_absolute_path_detection_distinguishes_drive_relative_paths() {
    assert!(windows_path_is_absolute(r"C:\project\file.rs"));
    assert!(windows_path_is_absolute(r"\\server\share\file.rs"));
    assert!(!windows_path_is_absolute(r"C:project\file.rs"));
    assert!(!windows_path_is_absolute(r"project\file.rs"));
}

#[test]
fn altgr_is_not_forwarded_as_terminal_ctrl_alt() {
    assert_eq!(
        terminal_modifiers_for_platform(PlatformKind::Windows, true, true),
        (false, false)
    );
    assert_eq!(
        terminal_modifiers_for_platform(PlatformKind::Windows, true, false),
        (true, false)
    );
    assert_eq!(
        terminal_modifiers_for_platform(PlatformKind::Linux, true, true),
        (true, true)
    );
}

#[test]
fn shortcut_modifiers_follow_platform_conventions() {
    assert!(primary_modifier_for_platform(PlatformKind::Windows, true, false));
    assert!(!primary_modifier_for_platform(PlatformKind::Windows, false, true));
    assert!(primary_modifier_for_platform(PlatformKind::Macos, false, true));
    assert!(!primary_modifier_for_platform(PlatformKind::Macos, true, false));

    assert!(word_modifier_for_platform(PlatformKind::Windows, true, false));
    assert!(!word_modifier_for_platform(PlatformKind::Windows, false, true));
    assert!(!word_modifier_for_platform(PlatformKind::Windows, true, true));
    assert!(word_modifier_for_platform(PlatformKind::Macos, false, true));

    assert!(text_input_modifiers_allowed_for_platform(
        PlatformKind::Windows,
        true,
        true,
        false,
    ));
    assert!(!text_input_modifiers_allowed_for_platform(
        PlatformKind::Windows,
        true,
        false,
        false,
    ));
    assert!(text_input_modifiers_allowed_for_platform(
        PlatformKind::Macos,
        false,
        true,
        false,
    ));
}

#[test]
fn windows_executable_resolution_uses_pathext_without_shelling_out() {
    let root = std::env::temp_dir().join(format!(
        "rriter-platform-executable-{}-{}",
        std::process::id(),
        TEMP_FILE_COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("ruff.EXE"), b"").unwrap();
    let search_path = std::env::join_paths([&root]).unwrap();
    let resolved = process::resolve_executable_with(
        OsStr::new("ruff"),
        Some(search_path.as_os_str()),
        Some(OsStr::new(".EXE;.CMD")),
        PlatformKind::Windows,
    );
    assert_eq!(resolved, Some(root.join("ruff.EXE")));
    let _ = fs::remove_dir_all(root);
}

#[cfg(unix)]
#[test]
fn managed_command_timeout_kills_the_process_group() {
    let mut command = command_for("sh").unwrap();
    command.args(["-c", "sleep 30 & wait"]);
    let started = std::time::Instant::now();
    let error = run_command_output(&mut command, Duration::from_millis(80)).unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::TimedOut);
    assert!(started.elapsed() < Duration::from_secs(3));
}

#[cfg(unix)]
#[test]
fn managed_child_exit_reaps_background_descendants() {
    let root = std::env::temp_dir().join(format!(
        "rriter-platform-process-tree-{}-{}",
        std::process::id(),
        TEMP_FILE_COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&root).unwrap();
    let marker = root.join("orphan-marker");

    let mut command = command_for("sh").unwrap();
    command
        .arg("-c")
        .arg(r#"(sleep 0.2; printf orphan > "$1") & exit 0"#)
        .arg("rriter-process-test")
        .arg(&marker)
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let mut child = ManagedChild::spawn(&mut command).unwrap();
    assert!(child.wait_timeout(Duration::from_secs(1)).unwrap().is_some());

    std::thread::sleep(Duration::from_millis(350));
    assert!(!marker.exists());
    let _ = fs::remove_dir_all(root);
}

#[cfg(unix)]
#[test]
fn managed_command_collects_stdout_stderr_and_status() {
    let mut command = command_for("sh").unwrap();
    command.args(["-c", "printf out; printf err >&2; exit 7"]);
    let output = run_command_output(&mut command, Duration::from_secs(2)).unwrap();
    assert_eq!(output.status.code(), Some(7));
    assert_eq!(output.stdout, b"out");
    assert_eq!(output.stderr, b"err");
}
