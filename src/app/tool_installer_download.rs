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
    if kind == ToolKind::RustAnalyzer {
        return Err("установка rust-analyzer появится в следующем шаге".to_string());
    }
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
        .connect_timeout(DOWNLOAD_CONNECT_TIMEOUT)
        .build()
        .map_err(|error| format!("Не удалось создать HTTP-клиент: {error}"))?;
    let watchdog = DownloadWatchdog::new(DOWNLOAD_STALL_TIMEOUT, DOWNLOAD_TOTAL_LIMIT);

    let response = tokio::select! {
        biased;
        () = wait_for_install_cancel(cancel) => {
            return Err(INSTALL_CANCELLED_MESSAGE.to_string());
        }
        response = watchdog.step(client.get(url).send()) => response,
    }?
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
            chunk = watchdog.step(response.chunk()) => chunk,
        }?
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

/// Bounds a download by inactivity instead of total time: every awaited step (response
/// headers, each body chunk) must finish within `stall`, and the whole download within
/// `total`. A slow but steady link therefore completes, a dead one fails after `stall`.
struct DownloadWatchdog {
    stall: Duration,
    total: Duration,
    deadline: tokio::time::Instant,
}

impl DownloadWatchdog {
    fn new(stall: Duration, total: Duration) -> Self {
        Self {
            stall,
            total,
            deadline: tokio::time::Instant::now() + total,
        }
    }

    async fn step<T>(&self, future: impl std::future::Future<Output = T>) -> Result<T, String> {
        let remaining = self
            .deadline
            .saturating_duration_since(tokio::time::Instant::now());
        let total_exceeded = || {
            format!(
                "Загрузка превысила предельное время ({} мин)",
                self.total.as_secs().div_ceil(60)
            )
        };
        if remaining.is_zero() {
            return Err(total_exceeded());
        }
        let limit = self.stall.min(remaining);
        tokio::time::timeout(limit, future).await.map_err(|_| {
            if remaining < self.stall {
                total_exceeded()
            } else {
                format!("Загрузка остановилась: нет данных более {:?}", self.stall)
            }
        })
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

const MAX_PDFIUM_ARCHIVE_BYTES: u64 = 64 * 1024 * 1024;
const MAX_PDFIUM_LIB_BYTES: u64 = 256 * 1024 * 1024;
const PDFIUM_VERSION_DIR_PREFIX: &str = "chromium-";

/// Everything the worker needs from the manifest; the worker never touches the manifest itself.
struct PdfiumInstallPlan {
    url: String,
    archive: String,
    sha256: String,
    lib_member: String,
    slug: String,
}

fn install_pdfium(
    plan: &PdfiumInstallPlan,
    cancel: &AtomicBool,
    reporter: &ToolInstallReporter,
) -> Result<ToolInstallOutcome, String> {
    // Tests drive the state machine without a network: the run fails at once.
    if cfg!(test) {
        return Err("загрузка отключена в тестах".to_string());
    }
    if Path::new(&plan.archive).file_name() != Some(OsStr::new(&plan.archive)) {
        return Err("недопустимое имя архива PDF-движка".to_string());
    }
    // The archive lives in a generation directory next to the versioned one and is
    // always removed, whatever the outcome.
    let layout = ToolInstallLayout::current(ToolKind::Pdfium);
    // A crash or kill skips the cleanup below; sweep what earlier runs left behind.
    let (removed, failed) = prune_stale_pdfium_ops(
        &layout.managed_root,
        &layout.generation_root,
        STALE_OP_DIR_MIN_AGE,
        MAX_STALE_OP_DIRS_PER_RUN,
    );
    if removed > 0 || failed > 0 {
        reporter.line(
            ToolInstallLogKind::Info,
            format!(
                "Удалены остатки прерванных загрузок PDF-движка: {removed}, не удалось: {failed}"
            ),
        );
    }
    let result = (|| {
        fs::create_dir_all(&layout.generation_root)
            .map_err(|error| format!("Не удалось создать каталог загрузки: {error}"))?;
        check_cancelled(cancel)?;
        reporter.phase(ToolInstallPhase::Downloading, "Загрузка PDF-движка");
        let archive_path = layout.generation_root.join(&plan.archive);
        download_pdfium_archive(&plan.url, &archive_path, cancel, reporter)?;
        let installed = install_from_archive_with(
            &archive_path,
            &plan.sha256,
            &plan.lib_member,
            &layout.managed_root,
            &plan.slug,
            cancel,
            &mut |phase, detail| reporter.phase(phase, detail),
        )?;
        Ok(ToolInstallOutcome {
            paths: vec![(ToolKind::Pdfium, installed)],
        })
    })();
    if let Err(error) = layout.remove_generation() {
        reporter.line(
            ToolInstallLogKind::Info,
            format!("Не удалось удалить временный каталог загрузки: {error}"),
        );
    }
    result
}

fn download_pdfium_archive(
    url: &str,
    destination: &Path,
    cancel: &AtomicBool,
    reporter: &ToolInstallReporter,
) -> Result<(), String> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| format!("Не удалось запустить сетевой runtime: {error}"))?;
    runtime.block_on(download_pdfium_archive_async(
        url,
        destination,
        cancel,
        reporter,
    ))
}

async fn download_pdfium_archive_async(
    url: &str,
    destination: &Path,
    cancel: &AtomicBool,
    reporter: &ToolInstallReporter,
) -> Result<(), String> {
    check_cancelled(cancel)?;
    let client = crate::platform::async_http_client_builder()
        .connect_timeout(DOWNLOAD_CONNECT_TIMEOUT)
        .build()
        .map_err(|error| format!("Не удалось создать HTTP-клиент: {error}"))?;
    let watchdog = DownloadWatchdog::new(DOWNLOAD_STALL_TIMEOUT, DOWNLOAD_TOTAL_LIMIT);
    let mut response = tokio::select! {
        biased;
        () = wait_for_install_cancel(cancel) => {
            return Err(INSTALL_CANCELLED_MESSAGE.to_string());
        }
        response = watchdog.step(client.get(url).send()) => response,
    }?
    .and_then(reqwest::Response::error_for_status)
    .map_err(|error| format!("Не удалось загрузить PDF-движок: {error}"))?;
    let content_length = response.content_length();
    if content_length.is_some_and(|length| length > MAX_PDFIUM_ARCHIVE_BYTES) {
        return Err("Архив PDF-движка превышает допустимый размер".to_string());
    }
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(destination)
        .map_err(|error| format!("Не удалось создать файл архива: {error}"))?;
    let mut downloaded = 0u64;
    let mut last_progress = 0u64;
    loop {
        let chunk = tokio::select! {
            biased;
            () = wait_for_install_cancel(cancel) => {
                return Err(INSTALL_CANCELLED_MESSAGE.to_string());
            }
            chunk = watchdog.step(response.chunk()) => chunk,
        }?
        .map_err(|error| format!("Ошибка чтения архива PDF-движка: {error}"))?;
        let Some(chunk) = chunk else {
            break;
        };
        downloaded = downloaded.saturating_add(chunk.len() as u64);
        if downloaded > MAX_PDFIUM_ARCHIVE_BYTES {
            return Err("Архив PDF-движка превышает допустимый размер".to_string());
        }
        output
            .write_all(&chunk)
            .map_err(|error| format!("Не удалось сохранить архив PDF-движка: {error}"))?;
        if last_progress == 0
            || downloaded.saturating_sub(last_progress) >= 64 * 1024
            || content_length == Some(downloaded)
        {
            reporter.line(
                ToolInstallLogKind::Info,
                pdfium_progress_line(downloaded, content_length),
            );
            last_progress = downloaded;
        }
    }
    output
        .flush()
        .map_err(|error| format!("Не удалось сохранить архив PDF-движка: {error}"))?;
    if downloaded == 0 {
        return Err("Получен пустой архив PDF-движка".to_string());
    }
    Ok(())
}

fn pdfium_progress_line(downloaded: u64, total: Option<u64>) -> String {
    let downloaded_kib = downloaded.div_ceil(1024);
    if let Some(total) = total.filter(|total| *total > 0) {
        let percent = (u128::from(downloaded) * 100 / u128::from(total)).min(100);
        format!(
            "Загрузка PDF-движка: {downloaded_kib}/{} КиБ ({percent}%)",
            total.div_ceil(1024)
        )
    } else {
        format!("Загрузка PDF-движка: {downloaded_kib} КиБ")
    }
}

/// Network-free half of the PDF engine install: checks the whole archive against
/// `expected_sha256`, reads only the entry named exactly `lib_member` and atomically
/// writes it to `managed_root/<version_slug>/<file name>`. Older `chromium-*` version
/// directories are removed afterwards. The caller owns (and removes) the generation
/// directory the archive was downloaded into; `prune_stale_generations` is not used
/// because it would delete the versioned directory too.
fn install_from_archive_with(
    archive: &Path,
    expected_sha256: &str,
    lib_member: &str,
    managed_root: &Path,
    version_slug: &str,
    cancel: &AtomicBool,
    on_phase: &mut dyn FnMut(ToolInstallPhase, &str),
) -> Result<PathBuf, String> {
    let member = normalized_archive_member(Path::new(lib_member))
        .ok_or_else(|| format!("недопустимый путь в архиве: {lib_member}"))?;
    let file_name = member
        .file_name()
        .ok_or_else(|| format!("недопустимый путь в архиве: {lib_member}"))?
        .to_owned();
    let mut slug_parts = Path::new(version_slug).components();
    if !matches!(
        (slug_parts.next(), slug_parts.next()),
        (Some(std::path::Component::Normal(_)), None)
    ) {
        return Err(format!("недопустимая версия PDF-движка: {version_slug}"));
    }
    check_cancelled(cancel)?;
    on_phase(ToolInstallPhase::Verifying, "Проверка контрольной суммы архива");
    let actual = sha256_file_hex(archive)
        .map_err(|error| format!("Не удалось прочитать архив PDF-движка: {error}"))?;
    if !actual.eq_ignore_ascii_case(expected_sha256.trim()) {
        return Err("архив повреждён или версия не совпадает".to_string());
    }
    check_cancelled(cancel)?;
    on_phase(ToolInstallPhase::Extracting, &format!("Распаковка {lib_member}"));
    let bytes = read_archive_member(archive, &member, lib_member, cancel)?;
    check_cancelled(cancel)?;
    let target = managed_root.join(version_slug).join(file_name);
    crate::platform::atomic_write(&target, &bytes)
        .map_err(|error| format!("Не удалось сохранить {}: {error}", target.display()))?;
    prune_old_pdfium_versions(managed_root, version_slug);
    Ok(target)
}

/// Lexical archive path without `.` parts; `None` for `..`, absolute and prefixed paths.
fn normalized_archive_member(path: &Path) -> Option<PathBuf> {
    use std::path::Component;
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::Normal(part) => normalized.push(part),
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => return None,
        }
    }
    (!normalized.as_os_str().is_empty()).then_some(normalized)
}

fn sha256_file_hex(path: &Path) -> io::Result<String> {
    use io::Read;
    use sha2::{Digest, Sha256};
    let mut file = fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

/// Reads the single entry `member` of a `.tgz`; nothing else is extracted, so archive
/// entries with `..` or absolute names can never reach the file system.
fn read_archive_member(
    archive: &Path,
    member: &Path,
    lib_member: &str,
    cancel: &AtomicBool,
) -> Result<Vec<u8>, String> {
    use io::Read;
    let read_error = |error: io::Error| format!("Не удалось прочитать архив PDF-движка: {error}");
    let file = fs::File::open(archive).map_err(read_error)?;
    let mut tar = tar::Archive::new(flate2::read::GzDecoder::new(io::BufReader::new(file)));
    for entry in tar.entries().map_err(read_error)? {
        check_cancelled(cancel)?;
        let entry = entry.map_err(read_error)?;
        let matches = entry
            .path()
            .ok()
            .and_then(|path| normalized_archive_member(&path))
            .is_some_and(|path| path == member);
        if !matches {
            continue;
        }
        if !entry.header().entry_type().is_file() {
            return Err(format!("{lib_member} в архиве не является файлом"));
        }
        if entry.size() > MAX_PDFIUM_LIB_BYTES {
            return Err(format!("{lib_member} превышает допустимый размер"));
        }
        let mut bytes = Vec::with_capacity(usize::try_from(entry.size()).unwrap_or(0));
        entry
            .take(MAX_PDFIUM_LIB_BYTES)
            .read_to_end(&mut bytes)
            .map_err(read_error)?;
        if bytes.is_empty() {
            return Err(format!("{lib_member} в архиве пуст"));
        }
        return Ok(bytes);
    }
    Err(format!("в архиве нет {lib_member}"))
}

/// Best effort: a locked or busy old version stays and is retried on the next install.
fn prune_old_pdfium_versions(managed_root: &Path, keep_slug: &str) {
    let Ok(entries) = fs::read_dir(managed_root) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        if name.starts_with(PDFIUM_VERSION_DIR_PREFIX)
            && name != keep_slug
            && entry.file_type().is_ok_and(|kind| kind.is_dir())
        {
            let _ = fs::remove_dir_all(entry.path());
        }
    }
}

/// `<pid>-<counter>`, the shape of `platform::next_operation_id`.
fn is_operation_dir_name(name: &str) -> bool {
    let mut parts = name.split('-');
    let digits = |part: Option<&str>| {
        part.is_some_and(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()))
    };
    digits(parts.next()) && digits(parts.next()) && parts.next().is_none()
}

/// Removes staging directories (`managed_root/<op_id>`) left by an install that crashed
/// before its own cleanup. Only real directories directly inside `managed_root` whose name
/// is an operation id are touched (never `chromium-*` versions, files or symlinks), not
/// `keep`, and only when unmodified for `min_age` so a concurrent install keeps its
/// directory. At most `limit` directories are removed per call. Returns (removed, failed).
fn prune_stale_pdfium_ops(
    managed_root: &Path,
    keep: &Path,
    min_age: Duration,
    limit: usize,
) -> (usize, usize) {
    let Ok(entries) = fs::read_dir(managed_root) else {
        return (0, 0);
    };
    let (mut removed, mut failed) = (0, 0);
    for entry in entries.flatten() {
        if removed + failed >= limit {
            break;
        }
        let path = entry.path();
        let name = entry.file_name();
        let is_stale = path != keep
            && name.to_str().is_some_and(is_operation_dir_name)
            && entry.file_type().is_ok_and(|kind| kind.is_dir())
            && entry
                .metadata()
                .and_then(|meta| meta.modified())
                .ok()
                .and_then(|modified| modified.elapsed().ok())
                .is_some_and(|age| age >= min_age);
        if !is_stale {
            continue;
        }
        match fs::remove_dir_all(&path) {
            Ok(()) => removed += 1,
            Err(_) => failed += 1,
        }
    }
    (removed, failed)
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


#[cfg(test)]
mod pdfium_archive_tests {
    use super::*;

    struct Fixture {
        root: PathBuf,
        archive: PathBuf,
        sha256: String,
        managed_root: PathBuf,
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    /// Raw header names, so `../evil` and absolute names can be stored (the builder API refuses them).
    fn append_raw(
        builder: &mut tar::Builder<flate2::write::GzEncoder<Vec<u8>>>,
        name: &str,
        kind: tar::EntryType,
        data: &[u8],
    ) {
        let mut header = tar::Header::new_gnu();
        header.as_old_mut().name[..name.len()].copy_from_slice(name.as_bytes());
        header.set_entry_type(kind);
        header.set_size(data.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        builder.append(&header, data).unwrap();
    }

    fn fixture(name: &str) -> Fixture {
        let root = std::env::temp_dir().join(format!(
            "rriter-pdfium-archive-{name}-{}",
            crate::platform::next_operation_id()
        ));
        let managed_root = root.join("managed");
        fs::create_dir_all(managed_root.join("chromium-7000")).unwrap();
        fs::write(managed_root.join("chromium-7000/libpdfium.so"), b"old").unwrap();
        fs::create_dir_all(managed_root.join("other")).unwrap();
        fs::write(managed_root.join("other/keep"), b"keep").unwrap();
        let mut builder = tar::Builder::new(flate2::write::GzEncoder::new(
            Vec::new(),
            flate2::Compression::fast(),
        ));
        append_raw(&mut builder, "./lib/libpdfium.so", tar::EntryType::Regular, b"lib");
        append_raw(&mut builder, "../evil", tar::EntryType::Regular, b"evil");
        append_raw(&mut builder, "/abs/evil", tar::EntryType::Regular, b"evil");
        append_raw(&mut builder, "bin/other", tar::EntryType::Regular, b"other");
        append_raw(&mut builder, "lib/dir", tar::EntryType::Directory, b"");
        let bytes = builder.into_inner().unwrap().finish().unwrap();
        let archive = root.join("pdfium.tgz");
        fs::write(&archive, &bytes).unwrap();
        let sha256 = sha256_file_hex(&archive).unwrap();
        Fixture {
            root,
            archive,
            sha256,
            managed_root,
        }
    }

    /// Test-only shorthand: no cancellation, no phase reporting.
    fn install_from_archive(
        archive: &Path,
        expected_sha256: &str,
        lib_member: &str,
        managed_root: &Path,
        version_slug: &str,
    ) -> Result<PathBuf, String> {
        install_from_archive_with(
            archive,
            expected_sha256,
            lib_member,
            managed_root,
            version_slug,
            &AtomicBool::new(false),
            &mut |_, _| {},
        )
    }

    fn install(fixture: &Fixture, sha256: &str, member: &str) -> Result<PathBuf, String> {
        install_from_archive(
            &fixture.archive,
            sha256,
            member,
            &fixture.managed_root,
            "chromium-8066",
        )
    }

    fn names(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(dir)
            .unwrap()
            .flatten()
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    #[test]
    fn sha256_helper_matches_the_known_vector() {
        let dir = fixture("vector");
        let file = dir.root.join("abc");
        fs::write(&file, b"abc").unwrap();
        assert_eq!(
            sha256_file_hex(&file).unwrap(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn valid_archive_installs_the_library_and_prunes_only_old_versions() {
        let fixture = fixture("ok");
        let installed = install(&fixture, &fixture.sha256, "lib/libpdfium.so").unwrap();
        assert_eq!(installed, fixture.managed_root.join("chromium-8066/libpdfium.so"));
        assert_eq!(fs::read(&installed).unwrap(), b"lib");
        assert_eq!(names(&fixture.managed_root), ["chromium-8066", "other"]);
        assert!(fixture.managed_root.join("other/keep").is_file());

        // A repeated install replaces the existing file atomically.
        fs::write(&installed, b"stale").unwrap();
        let upper = fixture.sha256.to_ascii_uppercase();
        assert_eq!(install(&fixture, &upper, "lib/libpdfium.so").unwrap(), installed);
        assert_eq!(fs::read(&installed).unwrap(), b"lib");
        assert_eq!(names(&fixture.managed_root.join("chromium-8066")), ["libpdfium.so"]);
    }

    #[test]
    fn wrong_hash_is_rejected_before_anything_is_written() {
        let fixture = fixture("hash");
        let wrong = "0".repeat(64);
        let error = install(&fixture, &wrong, "lib/libpdfium.so").unwrap_err();
        assert!(error.starts_with("архив повреждён"), "{error}");
        assert_eq!(names(&fixture.managed_root), ["chromium-7000", "other"]);
    }

    #[test]
    fn traversal_and_absolute_members_are_rejected_and_never_extracted() {
        let fixture = fixture("traversal");
        for member in ["../evil", "/abs/evil", "lib/../../evil", "", "."] {
            let error = install(&fixture, &fixture.sha256, member).unwrap_err();
            assert!(error.contains("недопустимый путь"), "{member}: {error}");
        }
        // A valid member next to hostile entries reads only its own entry.
        install(&fixture, &fixture.sha256, "lib/libpdfium.so").unwrap();
        assert!(!fixture.root.join("evil").exists());
        assert!(!Path::new("/abs/evil").exists());
        assert_eq!(names(&fixture.root), ["managed", "pdfium.tgz"]);
        assert_eq!(names(&fixture.managed_root), ["chromium-8066", "other"]);
    }

    #[test]
    fn missing_or_non_file_member_is_an_error_without_side_effects() {
        let fixture = fixture("missing");
        assert_eq!(
            install(&fixture, &fixture.sha256, "lib/none").unwrap_err(),
            "в архиве нет lib/none"
        );
        let error = install(&fixture, &fixture.sha256, "lib/dir").unwrap_err();
        assert!(error.contains("не является файлом"), "{error}");
        assert_eq!(names(&fixture.managed_root), ["chromium-7000", "other"]);
    }

    #[test]
    fn invalid_version_slug_is_rejected() {
        let fixture = fixture("slug");
        for slug in ["", "..", "a/b", "/abs"] {
            let error = install_from_archive(
                &fixture.archive,
                &fixture.sha256,
                "lib/libpdfium.so",
                &fixture.managed_root,
                slug,
            )
            .unwrap_err();
            assert!(error.contains("недопустимая версия"), "{slug}: {error}");
        }
        assert_eq!(names(&fixture.managed_root), ["chromium-7000", "other"]);
    }

    #[test]
    fn cancel_before_extraction_leaves_the_managed_directory_untouched() {
        let fixture = fixture("cancel");
        let cancel = AtomicBool::new(true);
        let error = install_from_archive_with(
            &fixture.archive,
            &fixture.sha256,
            "lib/libpdfium.so",
            &fixture.managed_root,
            "chromium-8066",
            &cancel,
            &mut |_, _| {},
        )
        .unwrap_err();
        assert_eq!(error, INSTALL_CANCELLED_MESSAGE);
        assert_eq!(names(&fixture.managed_root), ["chromium-7000", "other"]);
    }

    #[test]
    fn cancel_between_verify_and_extract_stops_before_the_write() {
        let fixture = fixture("cancel-mid");
        let cancel = AtomicBool::new(false);
        let mut phases = Vec::new();
        let error = install_from_archive_with(
            &fixture.archive,
            &fixture.sha256,
            "lib/libpdfium.so",
            &fixture.managed_root,
            "chromium-8066",
            &cancel,
            &mut |phase, _| {
                phases.push(phase);
                if phase == ToolInstallPhase::Extracting {
                    cancel.store(true, Ordering::Release);
                }
            },
        )
        .unwrap_err();
        assert_eq!(error, INSTALL_CANCELLED_MESSAGE);
        assert_eq!(phases, [ToolInstallPhase::Verifying, ToolInstallPhase::Extracting]);
        assert_eq!(names(&fixture.managed_root), ["chromium-7000", "other"]);
    }
}
