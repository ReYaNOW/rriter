use super::*;
use std::collections::HashMap;
use std::time::Instant;

fn test_roots(name: &str) -> (PathBuf, PathBuf) {
    let base = std::env::temp_dir().join(format!(
        "rriter-tool-installer-{name}-{}",
        crate::platform::next_operation_id()
    ));
    (base.join("data"), base.join("cache"))
}

fn command_env(command: &Command) -> HashMap<String, String> {
    command
        .get_envs()
        .filter_map(|(name, value)| {
            Some((
                name.to_string_lossy().into_owned(),
                value?.to_string_lossy().into_owned(),
            ))
        })
        .collect()
}

#[test]
fn managed_layout_uses_generation_and_platform_specific_names() {
    let windows = ToolInstallLayout::with_roots(
        PlatformKind::Windows,
        ToolKind::Uv,
        "generation-a",
        PathBuf::from(r"C:\Users\Test\AppData\Local\RRiter"),
        PathBuf::from(r"C:\Users\Test\AppData\Local\RRiter\Cache"),
    );
    assert_eq!(
        windows.executable(),
        windows.generation_root.join("bin").join("uv.exe")
    );
    assert!(windows.generation_root.ends_with("managed/uv/generation-a"));

    for platform in [PlatformKind::Linux, PlatformKind::Macos] {
        let layout = ToolInstallLayout::with_roots(
            platform,
            ToolKind::Ruff,
            "generation-b",
            PathBuf::from("/Users/test/Library/Application Support/RRiter"),
            PathBuf::from("/Users/test/Library/Caches/RRiter"),
        );
        assert_eq!(layout.executable(), layout.bin.join("ruff"));
        assert!(
            layout
                .generation_root
                .ends_with("managed/ruff/generation-b")
        );
    }
}

#[test]
fn managed_executable_names_cover_all_supported_platforms() {
    for (kind, windows_name, unix_name) in [
        (ToolKind::Uv, "uv.exe", "uv"),
        (ToolKind::Ruff, "ruff.exe", "ruff"),
        (ToolKind::Ty, "ty.exe", "ty"),
    ] {
        assert_eq!(
            tool_executable_name(kind, PlatformKind::Windows),
            windows_name
        );
        assert_eq!(tool_executable_name(kind, PlatformKind::Linux), unix_name);
        assert_eq!(tool_executable_name(kind, PlatformKind::Macos), unix_name);
    }
    assert_eq!(
        tool_executable_name(ToolKind::Git, PlatformKind::Windows),
        ""
    );
    assert_eq!(installer_url(PlatformKind::Other), None);
}

#[test]
fn transactional_pruning_keeps_current_and_previous_generation() {
    let (data, cache) = test_roots("prune");
    let old = ToolInstallLayout::with_roots(
        PlatformKind::Linux,
        ToolKind::Ty,
        "old",
        data.clone(),
        cache.clone(),
    );
    let stale = ToolInstallLayout::with_roots(
        PlatformKind::Linux,
        ToolKind::Ty,
        "stale",
        data.clone(),
        cache.clone(),
    );
    let current =
        ToolInstallLayout::with_roots(PlatformKind::Linux, ToolKind::Ty, "current", data, cache);
    for layout in [&old, &stale, &current] {
        layout.create().unwrap();
        fs::write(layout.executable(), b"test").unwrap();
    }
    let (tx, _rx) = crate::ui_waker::UiWaker::counting().sync_channel(INSTALL_EVENT_CAPACITY);
    let reporter = ToolInstallReporter {
        tx,
        dropped_lines: Arc::new(AtomicUsize::new(0)),
    };
    current.prune_stale_generations(Some(&old.executable()), &reporter);
    assert!(old.generation_root.exists());
    assert!(current.generation_root.exists());
    assert!(!stale.generation_root.exists());
    let _ = fs::remove_dir_all(current.managed_root.parent().unwrap().parent().unwrap());
}

#[test]
fn failed_generation_cleanup_does_not_touch_previous_install() {
    let (data, cache) = test_roots("cleanup");
    let old = ToolInstallLayout::with_roots(
        PlatformKind::Windows,
        ToolKind::Ruff,
        "old",
        data.clone(),
        cache.clone(),
    );
    let failed =
        ToolInstallLayout::with_roots(PlatformKind::Windows, ToolKind::Ruff, "failed", data, cache);
    old.create().unwrap();
    failed.create().unwrap();
    fs::write(old.executable(), b"working").unwrap();
    fs::write(failed.executable(), b"partial").unwrap();
    failed.remove_generation().unwrap();
    assert!(old.executable().exists());
    assert!(!failed.generation_root.exists());
    let _ = fs::remove_dir_all(old.managed_root.parent().unwrap().parent().unwrap());
}

#[test]
fn uv_installer_plan_is_native_for_all_supported_platforms() {
    let windows_script = Path::new(r"C:\Temp\install uv.ps1");
    assert_eq!(
        installer_url(PlatformKind::Windows),
        Some(UV_INSTALL_URL_WINDOWS)
    );
    assert_eq!(installer_script_extension(PlatformKind::Windows), "ps1");
    assert_eq!(
        uv_installer_arguments(PlatformKind::Windows, windows_script),
        vec![
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-File",
            r"C:\Temp\install uv.ps1",
        ]
        .into_iter()
        .map(OsString::from)
        .collect::<Vec<_>>()
    );

    for platform in [PlatformKind::Linux, PlatformKind::Macos] {
        let script = Path::new("/tmp/install uv.sh");
        assert_eq!(installer_url(platform), Some(UV_INSTALL_URL_UNIX));
        assert_eq!(installer_script_extension(platform), "sh");
        assert_eq!(
            uv_installer_arguments(platform, script),
            vec![OsString::from("/tmp/install uv.sh")]
        );
    }
}

#[test]
fn only_uv_ruff_and_ty_offer_managed_installation() {
    assert!(ToolKind::Uv.supports_managed_install());
    assert!(ToolKind::Ruff.supports_managed_install());
    assert!(ToolKind::Ty.supports_managed_install());
    assert!(!ToolKind::Git.supports_managed_install());
    assert!(!ToolKind::Python.supports_managed_install());
    assert!(!ToolKind::Shell.supports_managed_install());
    assert_eq!(ToolKind::Ruff.managed_package(), Some("ruff"));
    assert_eq!(ToolKind::Ty.managed_package(), Some("ty"));
    assert_eq!(managed_package_spec(ToolKind::Ruff).unwrap(), "ruff@latest");
    assert_eq!(managed_package_spec(ToolKind::Ty).unwrap(), "ty@latest");
    assert!(managed_package_spec(ToolKind::Uv).is_err());
}

#[test]
fn uv_environment_is_isolated_and_never_updates_shell_profile() {
    let layout = ToolInstallLayout::with_roots(
        PlatformKind::Linux,
        ToolKind::Ruff,
        "generation",
        PathBuf::from("/data/rriter"),
        PathBuf::from("/cache/rriter"),
    );
    let mut tool = Command::new("uv");
    apply_uv_tool_environment(&mut tool, &layout);
    let tool_env = command_env(&tool);
    assert_eq!(
        tool_env.get("UV_TOOL_BIN_DIR"),
        Some(&layout.bin.to_string_lossy().into_owned())
    );
    assert_eq!(
        tool_env.get("UV_TOOL_DIR"),
        Some(&layout.environments.to_string_lossy().into_owned())
    );
    assert_eq!(
        tool_env.get("UV_CACHE_DIR"),
        Some(&layout.cache.to_string_lossy().into_owned())
    );
    assert_eq!(
        tool_env.get("UV_PYTHON_INSTALL_DIR"),
        Some(&layout.python_installations.to_string_lossy().into_owned())
    );
    assert_eq!(
        tool_env.get("UV_PYTHON_BIN_DIR"),
        Some(&layout.python_bin.to_string_lossy().into_owned())
    );
    assert_eq!(
        tool_env.get("UV_PYTHON_CACHE_DIR"),
        Some(&layout.python_cache.to_string_lossy().into_owned())
    );
    assert_eq!(
        tool_env.get("UV_PYTHON_INSTALL_BIN").map(String::as_str),
        Some("false")
    );
    assert_eq!(
        tool_env
            .get("UV_PYTHON_INSTALL_REGISTRY")
            .map(String::as_str),
        Some("false")
    );
    assert_eq!(
        tool_env.get("UV_NO_MODIFY_PATH").map(String::as_str),
        Some("1")
    );
    assert_eq!(
        tool_env.get("UV_SYSTEM_CERTS").map(String::as_str),
        Some("true")
    );
    assert_eq!(
        tool_env.get("UV_NO_PROGRESS").map(String::as_str),
        Some("1")
    );

    let mut installer = Command::new("sh");
    apply_uv_installer_environment(&mut installer, &layout);
    let installer_env = command_env(&installer);
    assert_eq!(
        installer_env.get("UV_UNMANAGED_INSTALL"),
        Some(&layout.bin.to_string_lossy().into_owned())
    );
    assert_eq!(
        installer_env.get("UV_NO_MODIFY_PATH").map(String::as_str),
        Some("1")
    );
    assert!(
        installer
            .get_envs()
            .any(|(name, value)| { name == OsStr::new("UV_INSTALL_DIR") && value.is_none() })
    );
}

#[test]
fn installer_log_is_byte_bounded_truncated_and_copyable() {
    let mut installer = ToolInstaller::default();
    installer.target = Some(ToolKind::Ruff);
    installer.phase = ToolInstallPhase::InstallingTool;
    installer.detail = "test".to_string();
    for index in 0..INSTALL_LOG_LIMIT + 10 {
        installer.push_log(ToolInstallLogKind::Output, format!("line-{index}"));
    }
    assert!(installer.logs.len() <= INSTALL_LOG_LIMIT + 1);
    assert!(installer.logs[0].text.contains("начало журнала удалено"));
    let full = installer.full_log();
    assert!(full.contains("Ruff: Установка инструмента"));
    assert!(full.contains(&format!("line-{}", INSTALL_LOG_LIMIT + 9)));

    installer.push_log(
        ToolInstallLogKind::Output,
        "я".repeat(INSTALL_LINE_BYTES_LIMIT),
    );
    assert!(
        installer
            .logs
            .last()
            .unwrap()
            .text
            .ends_with("[сообщение обрезано]")
    );
    assert!(installer.log_bytes <= INSTALL_LOG_BYTES_LIMIT + TRUNCATED_LOG_MARKER.len());
    assert_eq!(installer.logs[0].text, TRUNCATED_LOG_MARKER);
}

#[test]
fn log_scroll_follows_tail_until_user_scrolls_away() {
    let mut installer = ToolInstaller::default();
    installer.logs = (0..100)
        .map(|index| ToolInstallLogLine {
            kind: ToolInstallLogKind::Output,
            text: format!("line-{index}"),
        })
        .collect();
    let max = log_max_scroll(installer.logs.len(), 700.0, 1.0);
    assert!(max > 0.0);
    assert!(installer.update_log_scroll(1.0, max));
    assert_eq!(installer.log_scroll.target, max);
    installer.scroll_log_by(-100.0, max);
    assert!(!installer.follow_log);
    let target = installer.log_scroll.target;
    installer.logs.push(ToolInstallLogLine {
        kind: ToolInstallLogKind::Output,
        text: "new".to_string(),
    });
    let larger_max = log_max_scroll(installer.logs.len(), 700.0, 1.0);
    installer.update_log_scroll(0.016, larger_max);
    assert_eq!(installer.log_scroll.target, target);
}

#[test]
fn bounded_reporter_drops_output_instead_of_blocking_the_installer() {
    let (tx, rx) = crate::ui_waker::UiWaker::counting().sync_channel(1);
    let dropped_lines = Arc::new(AtomicUsize::new(0));
    let reporter = ToolInstallReporter {
        tx,
        dropped_lines: Arc::clone(&dropped_lines),
    };
    reporter.line(ToolInstallLogKind::Output, "first");
    reporter.line(ToolInstallLogKind::Output, "second");
    assert_eq!(dropped_lines.load(Ordering::Acquire), 1);
    assert!(matches!(rx.recv().unwrap(), ToolInstallEvent::Line(_)));

    reporter.line(ToolInstallLogKind::Output, "third");
    let ToolInstallEvent::Line(marker) = rx.recv().unwrap() else {
        panic!("expected a dropped-line marker");
    };
    assert!(marker.text.contains("пропущено строк вывода: 1"));
    assert_eq!(dropped_lines.load(Ordering::Acquire), 1);
}

#[test]
fn terminal_events_update_state_without_starting_network_work() {
    let mut installer = ToolInstaller::default();
    installer.target = Some(ToolKind::Ty);
    let (tx, rx) = crate::ui_waker::UiWaker::counting().sync_channel(INSTALL_EVENT_CAPACITY);
    installer.rx = Some(rx);
    tx.send(ToolInstallEvent::Phase(
        ToolInstallPhase::InstallingTool,
        "installing".to_string(),
    ))
    .unwrap();
    tx.send(ToolInstallEvent::Line(ToolInstallLogLine {
        kind: ToolInstallLogKind::Output,
        text: "progress".to_string(),
    }))
    .unwrap();
    tx.send(ToolInstallEvent::Done(Ok(ToolInstallOutcome {
        paths: vec![(ToolKind::Ty, PathBuf::from("/managed/bin/ty"))],
    })))
    .unwrap();
    installer.worker = Some(std::thread::spawn(|| {}));
    let outcome = installer.poll().unwrap();
    assert_eq!(installer.phase(), ToolInstallPhase::Succeeded);
    assert_eq!(outcome.paths[0].0, ToolKind::Ty);
    assert!(installer.full_log().contains("progress"));
    assert!(installer.worker.is_none());
}

#[test]
fn late_cancel_does_not_discard_a_committed_successful_generation() {
    let outcome = ToolInstallOutcome {
        paths: vec![(ToolKind::Uv, PathBuf::from("/managed/bin/uv"))],
    };
    let committed = Ok::<_, String>(outcome.clone());
    assert!(
        !generated_layouts_require_cleanup(&committed),
        "a validated generation is committed even if cancellation arrives late"
    );
    let ToolInstallEvent::Done(Ok(done)) = terminal_install_event(committed, true) else {
        panic!("successful install must win over a late cancellation request");
    };
    assert_eq!(done, outcome);

    assert!(matches!(
        terminal_install_event(Err(INSTALL_CANCELLED_MESSAGE.to_string()), true),
        ToolInstallEvent::Cancelled
    ));
    assert!(matches!(
        terminal_install_event(Err("failed".to_string()), false),
        ToolInstallEvent::Done(Err(error)) if error == "failed"
    ));
}

#[test]
fn download_progress_reports_known_and_unknown_lengths() {
    assert_eq!(download_progress_line(1024, None), "Загрузка uv: 1 КиБ");
    assert_eq!(
        download_progress_line(1536, Some(4096)),
        "Загрузка uv: 2/4 КиБ (37%)"
    );
    assert_eq!(
        download_progress_line(8192, Some(4096)),
        "Загрузка uv: 8/4 КиБ (100%)"
    );
}

#[test]
fn cancelled_download_stops_before_network_or_file_creation() {
    let (data, cache) = test_roots("cancel-download");
    let destination = cache.join("installer.sh");
    let (tx, _rx) = crate::ui_waker::UiWaker::counting().sync_channel(INSTALL_EVENT_CAPACITY);
    let reporter = ToolInstallReporter {
        tx,
        dropped_lines: Arc::new(AtomicUsize::new(0)),
    };
    let cancel = AtomicBool::new(true);
    let error =
        download_installer(PlatformKind::Linux, &destination, &cancel, &reporter).unwrap_err();
    assert!(error.contains("отменена"));
    assert!(!destination.exists());
    let _ = fs::remove_dir_all(data.parent().unwrap());
}

#[test]
fn unsupported_tool_is_rejected_before_worker_or_network_is_started() {
    let mut installer = ToolInstaller::default();
    let error = installer.start(ToolKind::Git, &crate::ui_waker::UiWaker::counting()).unwrap_err();
    assert!(error.contains("нельзя установить"));
    assert!(!installer.is_running());
    assert!(installer.worker.is_none());
}

#[test]
fn cancellation_only_overrides_the_exact_cancelled_error() {
    let ToolInstallEvent::Done(Err(error)) =
        terminal_install_event(Err("проверка версии завершилась ошибкой".to_string()), true)
    else {
        panic!("a late cancel must not hide an unrelated failure");
    };
    assert_eq!(error, "проверка версии завершилась ошибкой");

    assert!(matches!(
        terminal_install_event(Err(INSTALL_CANCELLED_MESSAGE.to_string()), false),
        ToolInstallEvent::Done(Err(error)) if error == INSTALL_CANCELLED_MESSAGE
    ));
}

#[test]
fn successful_generation_is_not_removed_by_late_cancellation() {
    let (data, cache) = test_roots("late-success");
    let layout =
        ToolInstallLayout::with_roots(PlatformKind::Linux, ToolKind::Uv, "committed", data, cache);
    layout.create().unwrap();
    fs::write(layout.executable(), b"validated").unwrap();

    let outcome = Ok::<_, String>(ToolInstallOutcome {
        paths: vec![(ToolKind::Uv, layout.executable())],
    });
    if generated_layouts_require_cleanup(&outcome) {
        layout.remove_generation().unwrap();
    }

    assert!(layout.executable().is_file());
    let _ = fs::remove_dir_all(layout.managed_root.parent().unwrap().parent().unwrap());
    let _ = fs::remove_dir_all(layout.cache.parent().unwrap());
}

#[test]
fn installer_log_geometry_is_pixel_stable_at_fractional_scales() {
    for scale in [1.0, 1.25, 1.5, 1.75, 2.0] {
        let values = [
            log_modal_height(803.0, scale),
            log_viewport_height(803.0, scale),
            log_line_height(scale),
            log_max_scroll(137, 803.0, scale),
        ];
        for value in values {
            assert_eq!(
                value.fract(),
                0.0,
                "geometry must be snapped at scale {scale}"
            );
        }
        assert!(values[0] > values[1]);
        assert!(values[2] >= 1.0);
    }
}

#[test]
fn async_cancel_waiter_observes_cancellation_without_network_timeout() {
    let cancel = Arc::new(AtomicBool::new(false));
    let cancel_for_thread = Arc::clone(&cancel);
    let setter = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(40));
        cancel_for_thread.store(true, Ordering::Release);
    });
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .unwrap();
    let started = Instant::now();
    runtime.block_on(wait_for_install_cancel(cancel.as_ref()));
    setter.join().unwrap();
    assert!(started.elapsed() < Duration::from_secs(1));
}

#[cfg(unix)]
#[test]
fn managed_ruff_install_uses_isolated_uv_environment_and_validates_executable() {
    use std::os::unix::fs::PermissionsExt;

    let (data, cache) = test_roots("fake uv with spaces");
    let layout = ToolInstallLayout::with_roots(
        PlatformKind::Linux,
        ToolKind::Ruff,
        "generation",
        data,
        cache,
    );
    layout.create().unwrap();

    let fake_uv = layout.generation_root.join("fake uv");
    fs::write(
        &fake_uv,
        r#"#!/bin/sh
set -eu
test "$1" = "--color"
test "$2" = "never"
test "$3" = "tool"
test "$4" = "install"
test "$5" = "--force"
test "$6" = "ruff@latest"
test -n "$UV_TOOL_BIN_DIR"
test -n "$UV_TOOL_DIR"
test -n "$UV_CACHE_DIR"
mkdir -p "$UV_TOOL_BIN_DIR" "$UV_TOOL_DIR" "$UV_CACHE_DIR"
cat > "$UV_TOOL_BIN_DIR/ruff" <<'EOF'
#!/bin/sh
echo 'ruff 0.test'
EOF
chmod +x "$UV_TOOL_BIN_DIR/ruff"
echo 'installed fake ruff'
"#,
    )
    .unwrap();
    let mut permissions = fs::metadata(&fake_uv).unwrap().permissions();
    permissions.set_mode(0o700);
    fs::set_permissions(&fake_uv, permissions).unwrap();

    let (tx, rx) = crate::ui_waker::UiWaker::counting().sync_channel(INSTALL_EVENT_CAPACITY);
    let reporter = ToolInstallReporter {
        tx,
        dropped_lines: Arc::new(AtomicUsize::new(0)),
    };
    let cancel = AtomicBool::new(false);
    install_uv_tool(ToolKind::Ruff, &fake_uv, &layout, &cancel, &reporter).unwrap();

    assert!(layout.executable().is_file());
    assert!(layout.environments.is_dir());
    assert!(layout.cache.is_dir());
    let log = rx
        .try_iter()
        .filter_map(|event| match event {
            ToolInstallEvent::Line(line) => Some(line.text),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n");
    assert!(log.contains("installed fake ruff"));
    assert!(log.contains("ruff 0.test"));

    let _ = fs::remove_dir_all(layout.managed_root.parent().unwrap().parent().unwrap());
    let _ = fs::remove_dir_all(layout.cache.parent().unwrap());
}

#[test]
fn tool_log_scrollbar_supports_smooth_click_drag_and_release() {
    let mut installer = ToolInstaller::default();
    assert!(installer.begin_log_scroll_drag(50.0, 0.0, 100.0, 100.0, 500.0, 20.0));
    assert!(installer.log_scroll_is_dragging());
    assert_eq!(installer.log_scroll.current, 0.0);
    assert!(installer.log_scroll.target > 0.0);
    let first_target = installer.log_scroll.target;
    assert!(installer.drag_log_scroll(90.0, 0.0, 100.0, 100.0, 500.0, 20.0));
    assert_eq!(installer.log_scroll.current, 0.0);
    assert!(installer.log_scroll.target > first_target);
    let release_target = installer.log_scroll.target;
    installer.end_log_scroll_drag();
    assert!(!installer.log_scroll_is_dragging());
    assert_eq!(installer.log_scroll.current, 0.0);
    assert_eq!(installer.log_scroll.target, release_target);
}

#[test]
fn dart_version_parser_accepts_stdout_and_stderr() {
    assert_eq!(
        parse_dart_version_output(b"Dart SDK version: 3.9.0 (stable)\n", b"").unwrap(),
        "Dart SDK version: 3.9.0 (stable)"
    );
    assert_eq!(
        parse_dart_version_output(b"", b"Dart SDK version: 3.9.1 (stable)\r\n").unwrap(),
        "Dart SDK version: 3.9.1 (stable)"
    );
    assert_eq!(
        parse_dart_version_output(
            b"warning: ignored preamble\nDart SDK version: 3.9.2 (stable)\n",
            b"",
        )
        .unwrap(),
        "Dart SDK version: 3.9.2 (stable)"
    );
}

#[test]
fn dart_version_parser_rejects_empty_and_unknown_output() {
    assert!(parse_dart_version_output(b"", b"").is_err());
    assert!(parse_dart_version_output(b"not dart", b"warning").is_err());
    assert!(parse_dart_version_output(&[0xff, 0xfe], b"").is_err());
}

#[test]
fn dart_tool_state_ignores_stale_probe_generation() {
    let mut state = DartToolState::default();
    state.status = DartToolStatus::Checking;
    state.generation = 7;
    let (tx, rx) = crate::ui_waker::UiWaker::counting().sync_channel(1);
    tx.send(DartProbeResult {
        generation: 6,
        result: Ok("Dart SDK version: stale".to_string()),
    })
    .unwrap();
    state.rx = Some(rx);

    assert!(!state.poll());
    assert_eq!(state.status(), DartToolStatus::Checking);
    assert!(state.version().is_none());
}

#[test]
fn dart_tool_state_transitions_from_checking_to_ready() {
    let mut state = DartToolState::default();
    state.status = DartToolStatus::Checking;
    state.generation = 3;
    let (tx, rx) = crate::ui_waker::UiWaker::counting().sync_channel(1);
    tx.send(DartProbeResult {
        generation: 3,
        result: Ok("Dart SDK version: 3.9.0 (stable)".to_string()),
    })
    .unwrap();
    state.rx = Some(rx);

    assert!(state.poll());
    assert_eq!(state.status(), DartToolStatus::Ready);
    assert_eq!(state.version(), Some("Dart SDK version: 3.9.0 (stable)"));
    assert!(state.error().is_none());
}

#[test]
fn dart_tool_state_preserves_probe_error_for_settings_ui() {
    let mut state = DartToolState::default();
    state.status = DartToolStatus::Checking;
    state.generation = 4;
    let (tx, rx) = crate::ui_waker::UiWaker::counting().sync_channel(1);
    tx.send(DartProbeResult {
        generation: 4,
        result: Err("invalid executable".to_string()),
    })
    .unwrap();
    state.rx = Some(rx);

    assert!(state.poll());
    assert_eq!(state.status(), DartToolStatus::Error);
    assert_eq!(state.error(), Some("invalid executable"));
    assert!(state.version().is_none());
}

#[test]
fn cancelling_dart_probe_sets_shared_cancel_flag() {
    let cancel = Arc::new(AtomicBool::new(false));
    let mut state = DartToolState::default();
    state.status = DartToolStatus::Checking;
    state.cancel = Some(Arc::clone(&cancel));

    state.cancel_probe();

    assert!(cancel.load(Ordering::Acquire));
    assert!(state.cancel.is_none());
    assert!(state.rx.is_none());
}

#[test]
fn dart_tool_status_labels_cover_user_visible_states() {
    for status in [
        DartToolStatus::NotFound,
        DartToolStatus::Checking,
        DartToolStatus::Ready,
        DartToolStatus::Installing,
        DartToolStatus::Updating,
        DartToolStatus::Cancelling,
        DartToolStatus::Error,
    ] {
        assert!(!status.label().is_empty());
    }
}

#[test]
fn dart_restart_adapter_targets_only_dart_server() {
    let source = include_str!("tool_installer.rs");
    let start = source.find("fn restart_dart_server").unwrap();
    let tail = &source[start..];
    let end = tail.find("fn trigger_tool_install").unwrap();
    let body = &tail[..end];

    assert!(body.contains("restart_server(\"dart\")"));
    assert!(!body.contains("restart_python"));
    assert!(!body.contains("restart_server(\"ruff\")"));
    assert!(!body.contains("restart_server(\"ty\")"));
}

fn poll_until_finish(installer: &mut ToolInstaller) -> ToolInstallFinish {
    let started = Instant::now();
    while started.elapsed() < Duration::from_secs(5) {
        if let Some(finish) = installer.poll_finish() {
            return finish;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    panic!("installer did not finish");
}

#[test]
fn pdfium_kind_stays_out_of_settings_and_config_paths() {
    assert!(!ToolKind::ALL.contains(&ToolKind::Pdfium));
    assert_eq!(ToolKind::from_index(ToolKind::Pdfium.index()), None);
    assert_eq!(ToolKind::Pdfium.config_key(), "pdfium");
    assert!(!ToolKind::Pdfium.supports_managed_install());
    let mut paths = crate::platform::ToolPaths::default();
    paths.set(ToolKind::Pdfium, Some(PathBuf::from("/x/libpdfium.so")));
    assert_eq!(paths.get(ToolKind::Pdfium), None);
    assert_eq!(paths.iter().count(), ToolKind::ALL.len());
    assert!(resolve_tool_kind(ToolKind::Pdfium).path.is_none());
}

#[test]
fn pdfium_install_fails_at_once_in_tests_without_touching_the_network() {
    let mut installer = ToolInstaller::default();
    installer
        .start_pdfium_install(&crate::ui_waker::UiWaker::counting())
        .unwrap();
    assert_eq!(installer.target(), Some(ToolKind::Pdfium));
    assert!(!installer.is_log_open(), "the PDF tab shows progress itself");
    assert_eq!(
        poll_until_finish(&mut installer),
        ToolInstallFinish::Failed("загрузка отключена в тестах".to_string())
    );
    assert_eq!(installer.phase(), ToolInstallPhase::Failed);
    assert!(!installer.is_running());
    assert!(installer.worker.is_none());
}

#[test]
fn pdfium_install_is_refused_while_another_tool_installs() {
    let mut installer = ToolInstaller::default();
    let (_tx, rx) = crate::ui_waker::UiWaker::counting().sync_channel(INSTALL_EVENT_CAPACITY);
    installer.target = Some(ToolKind::Uv);
    installer.rx = Some(rx);
    assert!(installer.is_running());
    assert_eq!(
        installer.start_pdfium_install(&crate::ui_waker::UiWaker::counting()),
        Err("Другая установка уже выполняется".to_string())
    );
    assert_eq!(installer.target(), Some(ToolKind::Uv));
}

#[test]
fn pdfium_cancel_and_failure_are_reported_as_terminal_finishes() {
    let mut installer = ToolInstaller::default();
    installer.target = Some(ToolKind::Pdfium);
    let (tx, rx) = crate::ui_waker::UiWaker::counting().sync_channel(INSTALL_EVENT_CAPACITY);
    installer.rx = Some(rx);
    tx.send(terminal_install_event(
        Err(INSTALL_CANCELLED_MESSAGE.to_string()),
        true,
    ))
    .unwrap();
    assert_eq!(installer.poll_finish(), Some(ToolInstallFinish::Cancelled));
    assert_eq!(installer.phase(), ToolInstallPhase::Cancelled);
    assert!(installer.poll_finish().is_none());

    let (tx, rx) = crate::ui_waker::UiWaker::counting().sync_channel(INSTALL_EVENT_CAPACITY);
    installer.rx = Some(rx);
    installer.phase = ToolInstallPhase::Idle;
    tx.send(ToolInstallEvent::Done(Err("архив повреждён".to_string())))
        .unwrap();
    assert_eq!(
        installer.poll_finish(),
        Some(ToolInstallFinish::Failed("архив повреждён".to_string()))
    );
    // `poll` keeps its old contract: only a success is returned.
    assert!(installer.poll().is_none());
}

#[test]
fn pdf_engine_button_flow_installs_once_and_returns_to_retry() {
    use crate::app::pdf_tab::PdfEngineState;
    let (_context, mut app) =
        crate::platform::offscreen_gl::test_support::offscreen_test_app(64, 64, 1.0);
    // Not installable (explicit path or unsupported platform): the click does nothing.
    app.pdf_engine = PdfEngineState::Missing {
        message: "fixed".to_string(),
        installable: false,
    };
    app.install_pdf_engine();
    assert!(matches!(&app.pdf_engine, PdfEngineState::Missing { message, installable: false } if message == "fixed"));
    assert!(app.tool_installer.target().is_none());

    app.pdf_engine = PdfEngineState::Missing {
        message: "not found".to_string(),
        installable: true,
    };
    app.install_pdf_engine();
    assert!(matches!(&app.pdf_engine, PdfEngineState::Installing { prev, .. } if prev == "not found"));
    // A second click while installing is ignored.
    app.install_pdf_engine();
    assert!(matches!(&app.pdf_engine, PdfEngineState::Installing { prev, .. } if prev == "not found"));

    let started = Instant::now();
    while matches!(app.pdf_engine, PdfEngineState::Installing { .. }) {
        assert!(started.elapsed() < Duration::from_secs(5), "install did not finish");
        app.poll_tool_installer();
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(matches!(
        &app.pdf_engine,
        PdfEngineState::Missing { message, installable: true } if message == "загрузка отключена в тестах"
    ));
    // The engine is not a configured tool: nothing was stored for it.
    assert_eq!(app.tool_paths.get(ToolKind::Pdfium), None);
}

#[test]
fn pdf_engine_button_reports_a_running_install_of_another_tool() {
    use crate::app::pdf_tab::PdfEngineState;
    let (_context, mut app) =
        crate::platform::offscreen_gl::test_support::offscreen_test_app(64, 64, 1.0);
    let (_tx, rx) = crate::ui_waker::UiWaker::counting().sync_channel(INSTALL_EVENT_CAPACITY);
    app.tool_installer.target = Some(ToolKind::Ruff);
    app.tool_installer.rx = Some(rx);
    app.pdf_engine = PdfEngineState::Missing {
        message: "not found".to_string(),
        installable: true,
    };
    app.install_pdf_engine();
    assert!(matches!(
        &app.pdf_engine,
        PdfEngineState::Missing { message, installable: true } if message == "идёт установка другого инструмента"
    ));
    assert_eq!(app.tool_installer.target(), Some(ToolKind::Ruff));
}

#[test]
fn pdf_engine_install_finishes_map_to_engine_states() {
    use crate::app::pdf_tab::PdfEngineState;
    let installing = || PdfEngineState::Installing {
        prev: "prev".to_string(),
        progress: String::new(),
    };
    let (_context, mut app) =
        crate::platform::offscreen_gl::test_support::offscreen_test_app(64, 64, 1.0);

    app.pdf_engine = installing();
    assert!(app.sync_pdf_engine_install(Some(ToolInstallFinish::Cancelled)));
    assert!(matches!(&app.pdf_engine, PdfEngineState::Missing { message, installable: true } if message == "prev"));

    app.pdf_engine = installing();
    assert!(app.sync_pdf_engine_install(Some(ToolInstallFinish::Failed("сеть недоступна".to_string()))));
    assert!(matches!(&app.pdf_engine, PdfEngineState::Missing { message, installable: true } if message == "сеть недоступна"));

    // Outside `Installing` a stray finish changes nothing.
    assert!(!app.sync_pdf_engine_install(Some(ToolInstallFinish::Cancelled)));

    // Progress follows the last installer log line.
    app.pdf_engine = installing();
    app.tool_installer.note("Загрузка PDF-движка: 64/3800 КиБ (1%)");
    assert!(app.sync_pdf_engine_install(None));
    assert!(matches!(&app.pdf_engine, PdfEngineState::Installing { progress, .. } if progress.contains("64/3800")));
    assert!(!app.sync_pdf_engine_install(None));

    // A finished install whose library is still not located must not loop into a download.
    let located = crate::app::pdf_tab::engine_after_install(
        PdfEngineState::Missing {
            message: "not found".to_string(),
            installable: true,
        },
        Path::new("/managed/pdfium/chromium-8066/libpdfium.so"),
    );
    assert!(matches!(
        located,
        PdfEngineState::Missing { message, installable: true }
            if message == "библиотека установлена, но не найдена: /managed/pdfium/chromium-8066/libpdfium.so"
    ));
    assert!(matches!(
        crate::app::pdf_tab::engine_after_install(PdfEngineState::Starting, Path::new("/x")),
        PdfEngineState::Starting
    ));
}

fn block_on_test<F: std::future::Future>(future: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(future)
}

#[test]
fn download_watchdog_lets_a_slow_but_steady_transfer_finish() {
    // Each step is far below the stall limit even though the sum exceeds it.
    let watchdog = DownloadWatchdog::new(Duration::from_millis(200), Duration::from_secs(60));
    let steps = block_on_test(async {
        let mut steps = 0;
        for _ in 0..6 {
            watchdog
                .step(tokio::time::sleep(Duration::from_millis(60)))
                .await
                .unwrap();
            steps += 1;
        }
        steps
    });
    assert_eq!(steps, 6);
}

#[test]
fn download_watchdog_fails_a_stalled_step() {
    let watchdog = DownloadWatchdog::new(Duration::from_millis(50), Duration::from_secs(60));
    let started = Instant::now();
    let error = block_on_test(watchdog.step(std::future::pending::<()>())).unwrap_err();
    assert!(error.contains("нет данных"), "{error}");
    assert!(started.elapsed() < Duration::from_secs(5));
}

#[test]
fn download_watchdog_enforces_the_total_ceiling() {
    let watchdog = DownloadWatchdog::new(Duration::from_secs(60), Duration::from_millis(50));
    let error = block_on_test(watchdog.step(std::future::pending::<()>())).unwrap_err();
    assert!(error.contains("предельное время"), "{error}");
    std::thread::sleep(Duration::from_millis(60));
    let error = block_on_test(watchdog.step(async {})).unwrap_err();
    assert!(error.contains("предельное время"), "{error}");
}

#[test]
fn download_timeouts_are_bounded_and_not_a_whole_request_cap() {
    assert!(DOWNLOAD_STALL_TIMEOUT >= Duration::from_secs(10));
    assert!(DOWNLOAD_STALL_TIMEOUT < DOWNLOAD_TOTAL_LIMIT);
    // 64 MiB at the total ceiling must still be possible on a slow (< 31 KB/s) link.
    assert!(DOWNLOAD_TOTAL_LIMIT >= Duration::from_secs(30 * 60));
    assert!(STALE_OP_DIR_MIN_AGE > DOWNLOAD_TOTAL_LIMIT);
}

#[test]
fn operation_dir_names_match_only_pid_counter_ids() {
    assert!(is_operation_dir_name(&crate::platform::next_operation_id()));
    assert!(is_operation_dir_name("123-4"));
    for name in ["chromium-8066", "123", "123-", "-4", "1-2-3", "a-1", "", "other"] {
        assert!(!is_operation_dir_name(name), "{name}");
    }
}

#[test]
fn stale_pdfium_ops_are_removed_and_everything_else_is_kept() {
    let (data, _cache) = test_roots("stale-pdfium");
    let managed = data.join("tools").join("managed").join("pdfium");
    let keep = managed.join("900-1");
    for stale in ["100-1", "100-2"] {
        fs::create_dir_all(managed.join(stale)).unwrap();
        fs::write(managed.join(stale).join("archive.tgz"), b"partial").unwrap();
    }
    fs::create_dir_all(&keep).unwrap();
    fs::create_dir_all(managed.join("chromium-8066")).unwrap();
    fs::write(managed.join("chromium-8066").join("libpdfium.so"), b"lib").unwrap();
    fs::create_dir_all(managed.join("unrelated")).unwrap();
    fs::write(managed.join("100-3"), b"a file, not a directory").unwrap();

    // Fresh directories stay: they may belong to a concurrent install.
    assert_eq!(
        prune_stale_pdfium_ops(&managed, &keep, Duration::from_secs(3600), 32),
        (0, 0)
    );
    assert!(managed.join("100-1").exists());

    assert_eq!(prune_stale_pdfium_ops(&managed, &keep, Duration::ZERO, 32), (2, 0));
    assert!(!managed.join("100-1").exists());
    assert!(!managed.join("100-2").exists());
    assert!(keep.exists(), "the running install's own directory stays");
    assert!(managed.join("chromium-8066").join("libpdfium.so").exists());
    assert!(managed.join("unrelated").exists());
    assert!(managed.join("100-3").is_file());
    let _ = fs::remove_dir_all(data.parent().unwrap());
}

#[test]
fn stale_pdfium_op_cleanup_is_bounded_and_tolerates_a_missing_root() {
    let (data, _cache) = test_roots("stale-pdfium-limit");
    let managed = data.join("pdfium");
    assert_eq!(
        prune_stale_pdfium_ops(&managed, &managed.join("keep"), Duration::ZERO, 32),
        (0, 0)
    );
    for index in 0..5 {
        fs::create_dir_all(managed.join(format!("200-{index}"))).unwrap();
    }
    assert_eq!(
        prune_stale_pdfium_ops(&managed, &managed.join("keep"), Duration::ZERO, 3),
        (3, 0)
    );
    let left = fs::read_dir(&managed).unwrap().count();
    assert_eq!(left, 2);
    let _ = fs::remove_dir_all(data.parent().unwrap());
}

#[test]
fn rust_analyzer_gzip_install_verifies_extracts_and_cleans_failed_or_cancelled_attempts() {
    use std::io::Write;

    let (data, _) = test_roots("rust-analyzer-gzip");
    let root = data.join("managed");
    fs::create_dir_all(&root).unwrap();
    let archive = data.join("rust-analyzer.gz");
    let executable = b"#!/bin/sh\necho fake\n";
    let file = fs::File::create(&archive).unwrap();
    let mut encoder = flate2::write::GzEncoder::new(file, flate2::Compression::default());
    encoder.write_all(executable).unwrap();
    encoder.finish().unwrap();
    let sha256 = sha256_file_hex(&archive).unwrap();
    let destination = root.join("2026-09-28").join("rust-analyzer");

    let installed = install_rust_analyzer_from_gz(
        &archive,
        &sha256,
        &destination,
        &AtomicBool::new(false),
        &mut |_, _| {},
    )
    .unwrap();
    assert_eq!(installed, destination);
    assert_eq!(fs::read(&destination).unwrap(), executable);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_ne!(fs::metadata(&destination).unwrap().permissions().mode() & 0o111, 0);
    }

    let wrong_hash_destination = root.join("wrong-hash").join("rust-analyzer");
    assert!(install_rust_analyzer_from_gz(
        &archive,
        &"0".repeat(64),
        &wrong_hash_destination,
        &AtomicBool::new(false),
        &mut |_, _| {},
    )
    .is_err());
    assert!(!wrong_hash_destination.exists());
    assert!(!wrong_hash_destination.parent().unwrap().exists());

    for (name, bytes) in [
        ("garbage", b"not a gzip stream".to_vec()),
        ("truncated", fs::read(&archive).unwrap()[..fs::metadata(&archive).unwrap().len() as usize - 4].to_vec()),
    ] {
        let bad_archive = data.join(format!("{name}.gz"));
        fs::write(&bad_archive, bytes).unwrap();
        let bad_destination = root.join(name).join("rust-analyzer");
        let bad_hash = sha256_file_hex(&bad_archive).unwrap();
        assert!(install_rust_analyzer_from_gz(
            &bad_archive,
            &bad_hash,
            &bad_destination,
            &AtomicBool::new(false),
            &mut |_, _| {},
        )
        .is_err());
        assert!(!bad_destination.exists());
        assert!(!fs::read_dir(bad_destination.parent().unwrap()).is_ok_and(|mut entries| entries.next().is_some()));
    }

    let cancelled_destination = root.join("cancelled").join("rust-analyzer");
    assert_eq!(
        install_rust_analyzer_from_gz(
            &archive,
            &sha256,
            &cancelled_destination,
            &AtomicBool::new(true),
            &mut |_, _| {},
        )
        .unwrap_err(),
        INSTALL_CANCELLED_MESSAGE
    );
    assert!(!cancelled_destination.exists());
    assert!(!cancelled_destination.parent().unwrap().exists());

    let late_cancel_destination = root.join("late-cancel").join("rust-analyzer");
    let late_cancel = AtomicBool::new(false);
    assert_eq!(
        install_rust_analyzer_from_gz(
            &archive,
            &sha256,
            &late_cancel_destination,
            &late_cancel,
            &mut |phase, _| {
                if phase == ToolInstallPhase::Extracting {
                    late_cancel.store(true, Ordering::Release);
                }
            },
        )
        .unwrap_err(),
        INSTALL_CANCELLED_MESSAGE
    );
    assert!(!late_cancel_destination.exists());
    assert_eq!(fs::read_dir(late_cancel_destination.parent().unwrap()).unwrap().count(), 0);
    let _ = fs::remove_dir_all(data.parent().unwrap());
}

#[test]
fn rust_analyzer_archive_constants_match_supported_platform_policy() {
    for archive in crate::lsp::RUST_ANALYZER_ARCHIVES {
        assert_eq!(archive.sha256.len(), 64);
        assert!(archive.sha256.bytes().all(|byte| byte.is_ascii_hexdigit()));
    }
    #[cfg(not(windows))]
    assert!(crate::lsp::rust_analyzer_archive_for_platform().is_some());
    #[cfg(windows)]
    assert!(crate::lsp::rust_analyzer_archive_for_platform().is_none());
}
