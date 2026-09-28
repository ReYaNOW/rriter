#[derive(Clone, Debug, PartialEq, Eq)]
struct ToolInstallLayout {
    kind: ToolKind,
    managed_root: PathBuf,
    generation_root: PathBuf,
    bin: PathBuf,
    environments: PathBuf,
    python_installations: PathBuf,
    python_bin: PathBuf,
    python_cache: PathBuf,
    cache: PathBuf,
    downloads: PathBuf,
    platform: PlatformKind,
}

impl ToolInstallLayout {
    fn current(kind: ToolKind) -> Self {
        Self::with_roots(
            CURRENT_PLATFORM,
            kind,
            crate::platform::next_operation_id(),
            crate::platform::data_dir(),
            crate::platform::cache_dir(),
        )
    }

    fn with_roots(
        platform: PlatformKind,
        kind: ToolKind,
        generation: impl Into<String>,
        data: PathBuf,
        cache: PathBuf,
    ) -> Self {
        let generation = generation.into();
        let tools_root = data.join("tools").join("managed");
        let managed_root = tools_root.join(kind.config_key());
        let generation_root = managed_root.join(&generation);
        Self {
            kind,
            bin: generation_root.join("bin"),
            environments: generation_root.join("envs"),
            python_installations: tools_root.join("python"),
            python_bin: tools_root.join("python-bin"),
            python_cache: cache.join("uv-python"),
            cache: cache.join("uv"),
            downloads: cache.join("tool-installer"),
            managed_root,
            generation_root,
            platform,
        }
    }

    fn executable(&self) -> PathBuf {
        self.bin
            .join(tool_executable_name(self.kind, self.platform))
    }

    fn create(&self) -> io::Result<()> {
        fs::create_dir_all(&self.bin)?;
        fs::create_dir_all(&self.environments)?;
        fs::create_dir_all(&self.python_installations)?;
        fs::create_dir_all(&self.python_bin)?;
        fs::create_dir_all(&self.python_cache)?;
        fs::create_dir_all(&self.cache)?;
        fs::create_dir_all(&self.downloads)?;
        Ok(())
    }

    fn remove_generation(&self) -> io::Result<()> {
        match fs::remove_dir_all(&self.generation_root) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error),
        }
    }

    fn prune_stale_generations(
        &self,
        previous_configured_path: Option<&Path>,
        reporter: &ToolInstallReporter,
    ) {
        let previous_root = previous_configured_path
            .and_then(|path| generation_root_for_path(&self.managed_root, path));
        let entries = match fs::read_dir(&self.managed_root) {
            Ok(entries) => entries,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return,
            Err(error) => {
                reporter.line(
                    ToolInstallLogKind::Info,
                    format!(
                        "Не удалось проверить старые поколения {}: {error}",
                        self.kind.label()
                    ),
                );
                return;
            }
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path == self.generation_root || previous_root.as_ref() == Some(&path) {
                continue;
            }
            if entry.file_type().is_ok_and(|kind| kind.is_dir())
                && let Err(error) = fs::remove_dir_all(&path)
            {
                reporter.line(
                    ToolInstallLogKind::Info,
                    format!(
                        "Старое поколение {} пока не удалено (возможно, используется): {error}",
                        self.kind.label()
                    ),
                );
            }
        }
    }
}

fn generation_root_for_path(managed_root: &Path, path: &Path) -> Option<PathBuf> {
    path.ancestors()
        .find(|ancestor| ancestor.parent() == Some(managed_root))
        .map(Path::to_path_buf)
}

fn install_tool(
    kind: ToolKind,
    existing_uv: Option<PathBuf>,
    cancel: &AtomicBool,
    reporter: &ToolInstallReporter,
) -> Result<ToolInstallOutcome, String> {
    let target_layout = ToolInstallLayout::current(kind);
    target_layout
        .create()
        .map_err(|error| format!("Не удалось создать каталоги инструментов: {error}"))?;
    let mut generated_layouts = vec![target_layout.clone()];

    let previous_target = crate::platform::configured_tool_path(kind)
        .or_else(|| crate::platform::resolve_tool_kind(kind).path);
    let previous_uv = crate::platform::configured_tool_path(ToolKind::Uv)
        .or_else(|| crate::platform::resolve_tool_kind(ToolKind::Uv).path);
    let result = (|| {
        check_cancelled(cancel)?;
        let mut installed_paths = Vec::new();
        let (uv_path, installed_uv_layout) = if kind == ToolKind::Uv {
            let path = install_uv(&target_layout, cancel, reporter)?;
            installed_paths.push((ToolKind::Uv, path.clone()));
            (path, Some(target_layout.clone()))
        } else if let Some(path) = existing_uv {
            reporter.line(
                ToolInstallLogKind::Info,
                format!("Используется uv: {}", path.display()),
            );
            (path, None)
        } else {
            reporter.line(
                ToolInstallLogKind::Info,
                "uv не найден — сначала будет установлена управляемая копия",
            );
            let uv_layout = ToolInstallLayout::current(ToolKind::Uv);
            uv_layout
                .create()
                .map_err(|error| format!("Не удалось создать каталог uv: {error}"))?;
            generated_layouts.push(uv_layout.clone());
            let path = install_uv(&uv_layout, cancel, reporter)?;
            installed_paths.push((ToolKind::Uv, path.clone()));
            (path, Some(uv_layout))
        };

        if kind != ToolKind::Uv {
            install_uv_tool(kind, &uv_path, &target_layout, cancel, reporter)?;
            installed_paths.push((kind, target_layout.executable()));
        }

        check_cancelled(cancel)?;
        target_layout.prune_stale_generations(previous_target.as_deref(), reporter);
        if let Some(uv_layout) = installed_uv_layout
            && kind != ToolKind::Uv
        {
            uv_layout.prune_stale_generations(previous_uv.as_deref(), reporter);
        }
        Ok(ToolInstallOutcome {
            paths: installed_paths,
        })
    })();

    if generated_layouts_require_cleanup(&result) {
        for layout in generated_layouts.iter().rev() {
            if let Err(error) = layout.remove_generation() {
                reporter.line(
                    ToolInstallLogKind::Info,
                    format!(
                        "Не удалось удалить незавершённое поколение {}: {error}",
                        layout.kind.label()
                    ),
                );
            }
        }
    }
    result
}

fn generated_layouts_require_cleanup<T>(result: &Result<T, String>) -> bool {
    // Cancellation is observed while downloading or while a managed child is
    // running. Once validation and pruning completed, the generation is the
    // committed result even if the UI receives a very late Cancel click.
    result.is_err()
}

fn install_uv(
    layout: &ToolInstallLayout,
    cancel: &AtomicBool,
    reporter: &ToolInstallReporter,
) -> Result<PathBuf, String> {
    reporter.phase(
        ToolInstallPhase::DownloadingUv,
        "Загрузка официального установщика Astral",
    );
    let script_path = layout.downloads.join(format!(
        "rriter-uv-install-{}.{}",
        crate::platform::next_operation_id(),
        installer_script_extension(layout.platform)
    ));
    let install_result = (|| {
        download_installer(layout.platform, &script_path, cancel, reporter)?;
        check_cancelled(cancel)?;

        reporter.phase(
            ToolInstallPhase::InstallingUv,
            "Установка uv в управляемый каталог RRiter",
        );
        run_uv_installer(layout, &script_path, cancel, reporter)
    })();
    let _ = fs::remove_file(&script_path);
    install_result?;

    let uv_path = layout.executable();
    validate_executable(ToolKind::Uv, &uv_path, layout, cancel, reporter)?;
    Ok(uv_path)
}

fn download_installer(
    platform: PlatformKind,
    destination: &Path,
    cancel: &AtomicBool,
    reporter: &ToolInstallReporter,
) -> Result<(), String> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| format!("Не удалось запустить сетевой runtime: {error}"))?;
    runtime.block_on(download_installer_async(
        platform,
        destination,
        cancel,
        reporter,
    ))
}

async fn download_installer_async(
    platform: PlatformKind,
    destination: &Path,
    cancel: &AtomicBool,
    reporter: &ToolInstallReporter,
) -> Result<(), String> {
    let url = installer_url(platform)
        .ok_or_else(|| "Установка uv не поддерживается на этой платформе".to_string())?;
    check_cancelled(cancel)?;
    let client = crate::platform::async_http_client_builder()
        .connect_timeout(Duration::from_secs(15))
        .timeout(DOWNLOAD_TIMEOUT)
        .build()
        .map_err(|error| format!("Не удалось создать HTTP-клиент: {error}"))?;

    let response = tokio::select! {
        biased;
        () = wait_for_install_cancel(cancel) => {
            return Err(INSTALL_CANCELLED_MESSAGE.to_string());
        }
        response = client.get(url).send() => response,
    }
    .and_then(reqwest::Response::error_for_status)
    .map_err(|error| format!("Не удалось загрузить установщик uv: {error}"))?;
    let mut response = response;
    let content_length = response.content_length();
    if content_length.is_some_and(|length| length > MAX_INSTALLER_BYTES as u64) {
        return Err("Установщик uv превышает допустимый размер".to_string());
    }

    let mut bytes = Vec::with_capacity(
        content_length
            .and_then(|length| usize::try_from(length).ok())
            .unwrap_or(32 * 1024)
            .min(MAX_INSTALLER_BYTES),
    );
    let mut last_progress_bytes = 0usize;
    loop {
        let chunk = tokio::select! {
            biased;
            () = wait_for_install_cancel(cancel) => {
                return Err(INSTALL_CANCELLED_MESSAGE.to_string());
            }
            chunk = response.chunk() => chunk,
        }
        .map_err(|error| format!("Ошибка чтения установщика uv: {error}"))?;
        let Some(chunk) = chunk else {
            break;
        };
        if bytes.len().saturating_add(chunk.len()) > MAX_INSTALLER_BYTES {
            return Err("Установщик uv превышает допустимый размер".to_string());
        }
        bytes.extend_from_slice(&chunk);
        if last_progress_bytes == 0
            || bytes.len().saturating_sub(last_progress_bytes) >= 64 * 1024
            || content_length == Some(bytes.len() as u64)
        {
            reporter.line(
                ToolInstallLogKind::Info,
                download_progress_line(bytes.len(), content_length),
            );
            last_progress_bytes = bytes.len();
        }
    }
    check_cancelled(cancel)?;
    if bytes.is_empty() {
        return Err("Получен пустой установщик uv".to_string());
    }

    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(destination)
        .map_err(|error| format!("Не удалось создать файл установщика uv: {error}"))?;
    output
        .write_all(&bytes)
        .and_then(|()| output.flush())
        .map_err(|error| format!("Не удалось сохранить установщик uv: {error}"))?;
    reporter.line(
        ToolInstallLogKind::Info,
        format!(
            "Установщик uv сохранён: {} КиБ",
            (bytes.len() + 1023) / 1024
        ),
    );
    Ok(())
}

async fn wait_for_install_cancel(cancel: &AtomicBool) {
    while !cancel.load(Ordering::Acquire) {
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

fn download_progress_line(downloaded: usize, total: Option<u64>) -> String {
    let downloaded_kib = (downloaded + 1023) / 1024;
    if let Some(total) = total.filter(|total| *total > 0) {
        let total_kib = (total + 1023) / 1024;
        let percent = ((downloaded as u128 * 100) / total as u128).min(100);
        format!("Загрузка uv: {downloaded_kib}/{total_kib} КиБ ({percent}%)")
    } else {
        format!("Загрузка uv: {downloaded_kib} КиБ")
    }
}

fn run_uv_installer(
    layout: &ToolInstallLayout,
    script_path: &Path,
    cancel: &AtomicBool,
    reporter: &ToolInstallReporter,
) -> Result<(), String> {
    let mut command = uv_installer_command(layout.platform, script_path)?;
    command.current_dir(&layout.generation_root);
    apply_uv_installer_environment(&mut command, layout);
    apply_proxy_environment(&mut command);
    run_logged_command(
        &mut command,
        UV_INSTALL_TIMEOUT,
        cancel,
        reporter,
        "установщик uv",
    )
}

fn apply_uv_installer_environment(command: &mut Command, layout: &ToolInstallLayout) {
    command
        .env_remove("UV_INSTALL_DIR")
        .env("UV_UNMANAGED_INSTALL", &layout.bin)
        .env("UV_NO_MODIFY_PATH", "1")
        .env("UV_SYSTEM_CERTS", "true")
        .env("UV_NO_PROGRESS", "1")
        .env("NO_COLOR", "1")
        .env("CLICOLOR", "0")
        .env("TERM", "dumb");
}

fn install_uv_tool(
    kind: ToolKind,
    uv_path: &Path,
    layout: &ToolInstallLayout,
    cancel: &AtomicBool,
    reporter: &ToolInstallReporter,
) -> Result<(), String> {
    let package_spec = managed_package_spec(kind)?;
    reporter.phase(
        ToolInstallPhase::InstallingTool,
        format!("Установка {package_spec} через uv"),
    );
    let mut command = Command::new(uv_path);
    command
        .args(["--color", "never", "tool", "install", "--force"])
        .arg(&package_spec)
        .current_dir(&layout.generation_root);
    apply_uv_tool_environment(&mut command, layout);
    apply_proxy_environment(&mut command);
    run_logged_command(
        &mut command,
        TOOL_INSTALL_TIMEOUT,
        cancel,
        reporter,
        &format!("uv tool install {package_spec}"),
    )?;
    validate_executable(kind, &layout.executable(), layout, cancel, reporter)
}

fn managed_package_spec(kind: ToolKind) -> Result<String, String> {
    kind.managed_package()
        .map(|package| format!("{package}@latest"))
        .ok_or_else(|| format!("{} нельзя установить через uv", kind.label()))
}

fn validate_executable(
    kind: ToolKind,
    path: &Path,
    layout: &ToolInstallLayout,
    cancel: &AtomicBool,
    reporter: &ToolInstallReporter,
) -> Result<(), String> {
    reporter.phase(
        ToolInstallPhase::Validating,
        format!("Проверка {} --version", kind.label()),
    );
    if !path.is_file() {
        return Err(format!(
            "{} не найден после установки: {}",
            kind.label(),
            path.display()
        ));
    }
    let mut command = Command::new(path);
    command
        .arg("--version")
        .current_dir(&layout.generation_root);
    apply_uv_tool_environment(&mut command, layout);
    let result = run_logged_command(
        &mut command,
        TOOL_VALIDATE_TIMEOUT,
        cancel,
        reporter,
        &format!("{} --version", kind.label()),
    );
    if result.is_ok() {
        reporter.line(
            ToolInstallLogKind::Success,
            format!("{} готов: {}", kind.label(), path.display()),
        );
    }
    result
}

fn run_logged_command(
    command: &mut Command,
    timeout: Duration,
    cancel: &AtomicBool,
    reporter: &ToolInstallReporter,
    name: &str,
) -> Result<(), String> {
    let status = run_command_streaming_cancelable(command, timeout, cancel, |stream, line| {
        if line.trim().is_empty() {
            return;
        }
        let kind = match stream {
            ProcessOutputStream::Stdout => ToolInstallLogKind::Output,
            // uv and its bootstrap installer use stderr for normal progress.
            // A non-zero exit still adds a dedicated error message below.
            ProcessOutputStream::Stderr => ToolInstallLogKind::Output,
        };
        reporter.line(kind, line);
    })
    .map_err(|error| match error.kind() {
        io::ErrorKind::Interrupted => INSTALL_CANCELLED_MESSAGE.to_string(),
        io::ErrorKind::TimedOut => format!("{name} превысил лимит времени"),
        _ => format!("Не удалось запустить {name}: {error}"),
    })?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{name} завершился с кодом {:?}", status.code()))
    }
}

fn uv_installer_command(platform: PlatformKind, script_path: &Path) -> Result<Command, String> {
    match platform {
        PlatformKind::Windows => {
            let shell = ["powershell.exe", "pwsh.exe"]
                .into_iter()
                .find_map(|candidate| resolve_executable(OsStr::new(candidate)))
                .ok_or_else(|| "PowerShell не найден".to_string())?;
            let mut command = Command::new(shell);
            command.args(uv_installer_arguments(platform, script_path));
            Ok(command)
        }
        PlatformKind::Linux | PlatformKind::Macos => {
            let shell = resolve_executable(OsStr::new("/bin/sh"))
                .ok_or_else(|| "/bin/sh не найден".to_string())?;
            let mut command = Command::new(shell);
            command.args(uv_installer_arguments(platform, script_path));
            Ok(command)
        }
        PlatformKind::Other => Err("Установка uv не поддерживается на этой платформе".to_string()),
    }
}

fn uv_installer_arguments(platform: PlatformKind, script_path: &Path) -> Vec<OsString> {
    match platform {
        PlatformKind::Windows => [
            OsString::from("-NoLogo"),
            OsString::from("-NoProfile"),
            OsString::from("-NonInteractive"),
            OsString::from("-ExecutionPolicy"),
            OsString::from("Bypass"),
            OsString::from("-File"),
            script_path.as_os_str().to_os_string(),
        ]
        .into_iter()
        .collect(),
        _ => vec![script_path.as_os_str().to_os_string()],
    }
}

fn apply_uv_tool_environment(command: &mut Command, layout: &ToolInstallLayout) {
    command
        .env("UV_TOOL_BIN_DIR", &layout.bin)
        .env("UV_TOOL_DIR", &layout.environments)
        .env("UV_CACHE_DIR", &layout.cache)
        .env("UV_PYTHON_INSTALL_DIR", &layout.python_installations)
        .env("UV_PYTHON_BIN_DIR", &layout.python_bin)
        .env("UV_PYTHON_CACHE_DIR", &layout.python_cache)
        .env("UV_PYTHON_INSTALL_BIN", "false")
        .env("UV_PYTHON_INSTALL_REGISTRY", "false")
        .env("UV_NO_MODIFY_PATH", "1")
        .env("UV_SYSTEM_CERTS", "true")
        .env("UV_NO_PROGRESS", "1")
        .env("NO_COLOR", "1")
        .env("CLICOLOR", "0")
        .env("TERM", "dumb");
}

fn apply_proxy_environment(command: &mut Command) {
    let Some(proxy) = crate::platform::system_proxy_config() else {
        return;
    };
    set_env_if_absent(command, "ALL_PROXY", proxy.all.as_deref());
    set_env_if_absent(command, "HTTP_PROXY", proxy.http.as_deref());
    set_env_if_absent(command, "HTTPS_PROXY", proxy.https.as_deref());
    set_env_if_absent(command, "NO_PROXY", proxy.bypass.as_deref());
}

fn set_env_if_absent(command: &mut Command, name: &str, value: Option<&str>) {
    if std::env::var_os(name).is_none()
        && let Some(value) = value.filter(|value| !value.is_empty())
    {
        command.env(name, value);
    }
}

fn check_cancelled(cancel: &AtomicBool) -> Result<(), String> {
    if cancel.load(Ordering::Acquire) {
        Err(INSTALL_CANCELLED_MESSAGE.to_string())
    } else {
        Ok(())
    }
}

fn installer_url(platform: PlatformKind) -> Option<&'static str> {
    match platform {
        PlatformKind::Windows => Some(UV_INSTALL_URL_WINDOWS),
        PlatformKind::Linux | PlatformKind::Macos => Some(UV_INSTALL_URL_UNIX),
        PlatformKind::Other => None,
    }
}

fn installer_script_extension(platform: PlatformKind) -> &'static str {
    if platform == PlatformKind::Windows {
        "ps1"
    } else {
        "sh"
    }
}

fn tool_executable_name(kind: ToolKind, platform: PlatformKind) -> &'static str {
    match (kind, platform) {
        (ToolKind::Uv, PlatformKind::Windows) => "uv.exe",
        (ToolKind::Ruff, PlatformKind::Windows) => "ruff.exe",
        (ToolKind::Ty, PlatformKind::Windows) => "ty.exe",
        (ToolKind::Uv, _) => "uv",
        (ToolKind::Ruff, _) => "ruff",
        (ToolKind::Ty, _) => "ty",
        _ => "",
    }
}

