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
