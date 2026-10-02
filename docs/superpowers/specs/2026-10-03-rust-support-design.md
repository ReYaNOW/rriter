# Rust-поддержка на уровне Python: общий per-root слой LSP и rust-analyzer

Дата: 2026-10-03. Статус: решения brainstorming (Q1–Q9, подход O2, D1–D7) утверждены; находки консультанта (Dart-jobs и единая остановка корня, политика версий диагностик, Cargo-корень через `cargo locate-project`, двухуровневая резолюция без процессов в рендере, владение настройками и агрегация статуса, «занят» вместо «индексации») внесены; ждёт ревью пользователя.

## 1. Цель и границы

Rust-файлы получают тот же набор IDE-возможностей, что Python: live-диагностику и Problems, hover, definition, references, completion, signature help, inlay hints, code actions, formatting, статус сервера и строку в Settings с managed-установкой инструмента. Сервер — rust-analyzer (далее RA). Шаблон интеграции — Dart (сервер на корень проекта).

Сначала общее per-root ядро выносится из `dart_workspace.rs` в новый модуль и Dart переводится на него без изменения поведения; затем Rust добавляется как второй потребитель этого ядра.

Вне объёма:
* run/debug/test-раннеры (у Python их нет);
* установка rustup/cargo (как интерпретатор Python — не ставим);
* `.rs` вне Cargo-корня — только подсветка, без LSP;
* не-Cargo проекты (rust-script, одиночный `build.rs`), nested workspaces внутри workspace;
* свой раннер `cargo clippy` по образцу `ruff_workspace` — диагностика приходит только от RA;
* настройки RA кроме enable и check/clippy (features, target, proc-macro).

## 2. Ключевые решения

| # | Решение | Причина |
|---|---------|---------|
| K1 | Общий per-root слой `src/lsp/rooted_language.rs`, Dart и Rust — его потребители | Два per-root языка — уже дубль; третий ляжет бесплатно |
| K2 | Диспатч по расширению через одну функцию `language_for_ext` | Сейчас шесть строковых `match ext`/`else if ext == "dart"` в `lsp_manager.rs:573-593, 596-635, 638-690, 846-883` |
| K3 | Диагностика Rust — только `publishDiagnostics` от RA (`checkOnSave`) | RA сам гоняет `cargo check`/`clippy` по всему workspace; своя подсистема не нужна |
| K4 | Процесс RA на Cargo-корень, ленивый старт, остановка при закрытии последнего `.rs` корня | Так работают ty (`lsp_manager.rs:236-248`) и Dart (`dart_workspace.rs:173-201`); RA — 0,5–2 ГБ |
| K5 | `ToolKind::RustAnalyzer`, порядок: override → `rustup which` → PATH → managed | Как Dart: override/PATH/managed; rustup — штатный источник RA |
| K6 | Managed-установка — standalone-бинарь RA из GitHub releases, закреплённый тег + sha256, константы в коде | Шаблон pdfium (`tool_installer.rs:654-672`, проверка sha256 в `tool_installer_download.rs:814-857`); новый json-манифест запрещён AGENTS.md §3 |
| K7 | `RustSettings { enabled, check_command }` рядом с `DartSettings`, ключ `rust` в конфиге | Тот же механизм persist/UI (`state_persistence.rs:617-625, 714-757`) |
| K8 | Hover RA — общий путь нормализации плюс подсветка код-блоков `rust` деревом tree-sitter Rust | Общий путь сейчас подсвечивает код Python-парсером (`hover.rs:101-110`) |
| K9 | Характеризационные тесты Dart до переноса; Rust-тесты на `scripts/fake_lsp_server.py` через `RRITER_RUST_ANALYZER_PATH` | Так тестируется ty (`src/headless/tests.rs:23`); реальный RA индексирует минуты |
| K10 | Cargo-корень уточняет `cargo locate-project --workspace`, fallback — ближайший `Cargo.toml` | Правила `members`/`exclude`/`package.workspace` знает только Cargo; `toml`-crate в зависимостях нет (`Cargo.toml` — только `tree-sitter-toml-ng`) |
| K11 | Остановка корня — один метод `stop_root`; «можно ли» решает потребитель | Сегодня Dart гасит процесс в двух местах (`dart_workspace.rs:184-199, 292-307`) с разными условиями |
| K12 | Процессы (`cargo`, `rustup which`) — только в update, с таймаутом и кешем на корень; `resolve_tool_kind` остаётся файловым | Его зовёт рендер (`settings_tool_rows.rs:577`) |

## 3. Текущее состояние (факты)

* Серверы: `pub enum LspServerKind { Ruff, Ty, Dart }` (`protocol.rs:201-205`), `LspServerDef { kind, program, override_env, args, language_id, extensions }` (`protocol.rs:288-296`), константы `RUFF_SERVER`/`TY_SERVER`/`DART_SERVER` (`protocol.rs:298-324`). `restart_attempt_limit` = 4 для всех.
* `LspManager` (`lsp_manager.rs:21-60`): слоты `python`, `ty_process`; Dart — `open_dart_files: HashMap<PathKey, OpenDartFile>`, `dart_workspaces: HashMap<PathKey, DartWorkspaceState>`, `closed_dart_documents`, `dart_live_diagnostics: HashMap<PathBuf, (i32, Arc<[Diagnostic]>)>`, `dart_workspace_diagnostics`, `dart_status`.
* `DartWorkspaceState { root, process: Option<LspProcess>, generation, due_at, job: Option<DartAnalyzerJob> }` (`dart_workspace.rs:25-31`); `OpenDartFile { path, root, text, version }` (`:12-17`); корень — `dart_root_for_path` по `pubspec.yaml`/`analysis_options.yaml` с остановкой на границе workspace (`:63-95`).
* Жизненный цикл Dart: `open_dart_document` (`:114-150`), `change_dart_document` (`:150-173`), `close_dart_document` (`:173-201`, гасит процесс, когда у корня нет открытых файлов и нет job), `dart_process_for_path_mut` → `ensure_dart_process` ленивый старт (`:249-259, 398-419`), `poll_dart_processes` (`:262`), `mark_dart_missing` (`:310`), `reconfigure_dart_workspaces` (`:320`), `set_dart_workspace_analysis_enabled` (`:351`), `schedule_configured_dart_projects` (`:380`), analyzer-job `run_dart_workspace_check` (`:552`).
* `initialize` строится в `make_initialize_for_server(server, id, workspaces)` (`protocol_json_rpc_encoding.rs:140-264`): capabilities — две ветки (`Dart` / `Ruff | Ty`), `initializationOptions` — только для Dart (`:249-255`).
* Server→client: `workspace/configuration` отвечается через `configuration_response_for(server, item)` (`:783-805`), `client/registerCapability` → `result: null`, неизвестные запросы → `-32601`; `$/progress` и `window/workDoneProgress/create` обработчиков нет (`rg` пусто) — RA их шлёт, значит `window/workDoneProgress/create` сегодня получит `-32601` (допустимо по протоколу, RA продолжает работу без токена), `$/progress` — уведомление, игнорируется.
* Статусы: `LspServerStatus { Starting, Running, Crashed, Missing, Disabled }` (`lsp_process.rs:154-160`); показ — `lsp_ui.rs:370-376` и строка Dart в `settings_tool_rows.rs` (`dart_status_text`).
* Слияние диагностик: `diagnostic_slices_for_abs_path -> [&[Diagnostic]; 4]` = ruff (live, иначе workspace), ty, dart (live, иначе workspace), legacy (`lsp_diagnostics_store.rs:481-505`); `instant_merged_diagnostics_for_abs_path` (`:691-735`).
* Инструменты: `ToolKind { Git, Ruff, Ty, Uv, Python, Shell, Dart, Pdfium }`, `ALL: [Self; 7]` (без Pdfium), `ToolPaths { paths: [Option<PathBuf>; 7] }` (`integration.rs:15-25, 29, 134`); `ManagedToolInstallPlan { UvBootstrap, UvPackage(&str), DartSdkArchive, PdfiumArchive }`; `supports_managed_install` истинно только для uv-планов — у Dart кнопки установки нет, `DartSdkArchive` реализации не имеет. Managed Dart ищется в `data_dir()/tools/managed/dart/<gen>/dart-sdk/bin/dart` (`integration.rs:536-554`).
* Hover: `looks_like_dart_hover` → Dart-подсветка; `:param `/`looks_like_python_hover` → Python; иначе общий `normalize_hover_text` с подсветкой код-спанов Python-парсером (`hover.rs:11-24, 101-110`).
* Тесты: unit в `dart_workspace.rs` (`dart_root_*`, парсер machine-вывода), headless `ui_tests_settings_general.rs` (toggles Dart), `ui_tests_settings_tools.rs` (SDK refresh/restart), LSP-тесты ty через `scripts/fake_lsp_server.py` (`src/headless/tests.rs:23`).

## 4. Модули и файлы

Новые:
* `src/lsp/rooted_language.rs` — enum `RootedLanguage`, `RootedWorkspaces`, общий жизненный цикл (раздел 5).
* `src/lsp/rust_workspace.rs` — Rust-специфика: поиск Cargo-корня, `RUST_ANALYZER_SERVER`, `initializationOptions`, разбор `experimental/serverStatus`, константы релиза RA (тег, имена архивов по платформе, sha256).
* `src/app/rust_settings.rs` — `RustSettings`, `RustCheckCommand`.
* Тесты рядом с логикой (`#[cfg(test)] mod tests` в тех же файлах); headless — новый `src/headless/ui_tests_rust_lsp.rs`.

Изменяемые:
* `src/lsp/protocol.rs` — `LspServerKind::RustAnalyzer`, `name()`, `restart_attempt_limit`.
* `src/lsp/protocol/protocol_json_rpc_encoding.rs` — ветка capabilities для RA, `initializationOptions` из `RootedLanguage::initialization_options`, разбор уведомления `experimental/serverStatus` → новый `LspEvent::ServerStatus`.
* `src/lsp/lsp_manager.rs` — поля `dart: RootedWorkspaces`, `rust: RootedWorkspaces`, `rust_roots`, `rust_pending_docs`, `rust_cargo`; `language_for_ext`; `stop_rooted_root`, `rooted_root_may_stop`; удаление шести строковых армов; обобщение `request_ty_*`.
* `src/lsp/lsp_process.rs` — `start_with_executable` принимает `init_options: Option<&Value>`.
* `src/lsp/dart_workspace.rs` — остаётся analyzer-job, closing labels, Flutter-флаг, парсер `dart analyze`; общее уходит в `rooted_language.rs`.
* `src/lsp/lsp_diagnostics_store.rs` — слайс Rust в слиянии.
* `src/lsp/hover.rs` — подсветка код-блоков по тегу fence (`rust` → tree-sitter Rust).
* `src/platform/integration.rs` — `ToolKind::RustAnalyzer` (`ALL` 7 → 8, `ToolPaths` 7 → 8, `config_key`, `override_env`), `ManagedToolInstallPlan::RustAnalyzerArchive`, `resolve_rust_analyzer`.
* `src/app/tool_installer.rs`, `src/app/tool_installer_download.rs` — `start_rust_analyzer_install`.
* `src/render_view/settings_tool_rows.rs`, `src/render_view/settings_ui.rs`, `src/app/ui_handlers/ui_settings.rs`, `src/ui_system.rs` — строка RA и переключатели Rust.
* `src/state_persistence.rs` — ключ `rust`.
* `src/app/app_state.rs` (или где лежит `dart_settings`) — поле `rust_settings`.
* `PROJECT_GUIDE.md` §4 — новые файлы.

## 5. Общий per-root слой (`rooted_language.rs`)

```rust
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum RootedLanguage { Dart, Rust }

impl RootedLanguage {
    pub fn server_def(self) -> &'static LspServerDef;           // DART_SERVER | RUST_ANALYZER_SERVER
    pub fn extensions(self) -> &'static [&'static str];          // ["dart"] | ["rs"]
    pub fn root_for_path(self, path: &Path, workspaces: &[PathBuf], locate: &dyn Fn(&Path) -> Option<PathBuf>) -> Option<PathBuf>;
    pub fn tool_kind(self) -> ToolKind;                          // Dart | RustAnalyzer
}

pub struct OpenRootedFile { pub path: PathBuf, pub root: PathBuf, pub text: Arc<str>, pub version: i32 }
pub struct RootedWorkspaceState { pub root: PathBuf, pub process: Option<LspProcess> }

pub struct RootedWorkspaces {
    pub lang: RootedLanguage,
    pub open_files: HashMap<PathKey, OpenRootedFile>,
    pub roots: HashMap<PathKey, RootedWorkspaceState>,
    pub closed_documents: Vec<PathBuf>,
    pub live_diagnostics: HashMap<PathBuf, LiveDiagnostics>,      // раздел 6, политика версий
    pub init_options: Option<serde_json::Value>,                  // владелец настроек сервера — этот слой
    pub status: LspServerStatus,                                  // агрегат по корням, пересчёт в poll_processes
    pub health: Option<ServerHealth>,                             // только RA; Dart — None
    pub busy: bool,                                               // RA: !quiescent; Dart — false
    pub enabled: bool,
    pub missing: bool,                                            // инструмент не найден — не стартовать повторно
}
```

`RootedSettings` из brainstorming не нужен: настройки сервера живут в `init_options` этого слоя. Потребитель строит значение сам (`rust_workspace::initialization_options(check_command)`, для Dart — сегодняшняя константа из `protocol_json_rpc_encoding.rs:249-255`) и отдаёт через `set_init_options(value)`: значение изменилось → `restart_all()`. `make_initialize_for_server` получает `init_options: Option<&Value>` параметром и подставляет в `initializationOptions`; ответ на `workspace/configuration` для RA берётся из `rust.init_options` тем же `LspManager` (`configuration_response_for` получает ссылку на значение, а не строит его заново) — один источник для initialize и configuration.

`status` — один на язык, агрегат по `roots`, пересчитывается в конце `poll_processes` и при любом изменении `roots`: `!enabled` → `Disabled`; `missing` → `Missing`; хотя бы один процесс `Crashed` → `Crashed`; иначе хотя бы один `Starting` → `Starting`; иначе есть процессы → `Running`; процессов нет → `Disabled`. Текущий `dart_status` ведёт себя так же для одного корня, поэтому Dart-тесты не меняются.

Записи `live_diagnostics` несут `root: PathKey` корня-источника: остановка корня чистит ровно его записи (раздел 6). Поздних событий от остановленного процесса нет — `shutdown` закрывает его канал, `poll_processes` их не увидит.

Методы `RootedWorkspaces` — перенос из `dart_workspace.rs` с заменой «Dart» на `self.lang`. Слой ведёт только документы и процессы; решение «можно ли гасить корень» принимает потребитель в `LspManager`, но гасит всегда один метод:
* `open_document(path, text, version, root) -> PathKey` — корень передаёт вызывающий (`LspManager::notify_open` считает его через `lang.root_for_path`; `None` → документ не регистрируется, без LSP). Создаёт запись `roots` (`entry().or_insert`), лениво стартует процесс (`ensure_process`), шлёт `didOpen`. Планирование Dart analyzer-job при открытии (`dart_workspace.rs:146`) остаётся в Dart-ветке `notify_open` после этого вызова.
* `change_document(path, text, version)`.
* `close_document(path) -> Option<PathKey>` — `didClose`, удаление live-диагностики пути, запись в `closed_documents`; возвращает ключ корня, если у него не осталось открытых файлов (корень «простаивает»). Процесс сам не гасит.
* `stop_root(root_key) -> bool` — единственное место остановки: `process.take().shutdown()`, удаление записи `roots`, удаление `live_diagnostics` с этим `root`, пересчёт `status`; возвращает, изменились ли диагностики. Вызывается только из `LspManager::stop_rooted_root(lang, root)` (раздел 6, пересборка сводки).
* `root_has_open_files(root_key) -> bool`, `process_for_path_mut(path) -> Option<&mut LspProcess>` (ленивый старт как `dart_process_for_path_mut`), `poll_processes(events)` (в конце — пересчёт `status`), `mark_missing()`, `reconfigure(workspaces)`, `document_version(path)`, `drain_closed_documents()`, `set_enabled(bool)` (false → `stop_root` для всех корней), `restart_all()` (стоп всех корней, затем `ensure_process` для корней открытых файлов и повторный `didOpen` их документов — для смены настроек RA и после установки), `set_init_options(value)`.
* Старт процесса: `LspProcess::start_with_executable(def, vec![root], executable, init_options, ui_waker)` — исполняемый файл выдаёт `LspManager` через `lang.tool_kind()` (Dart — как сегодня `dart_executable_for_root`; Rust — из `rust_roots[..].executable`, раздел 7); `None`/`Err` → `mark_missing()`.

Решение об остановке в `LspManager`:
* `notify_close`: `if let Some(root) = rooted.close_document(path) && self.rooted_root_may_stop(lang, &root) { self.stop_rooted_root(lang, &root) }`; `rooted_root_may_stop`: Rust → `true`; Dart → `dart_jobs.get(root).is_none_or(|s| s.job.is_none())` (сегодня `dart_workspace.rs:184-199`).
* Dart poll после завершения job (`dart_workspace.rs:292-307`): `if !self.dart.root_has_open_files(&key) { self.stop_rooted_root(Dart, &key) }` — вместо прямого `process.take()`.
* Результат job применяется только при совпадении `generation` и включённом анализе (`:484-499`) — без изменений.

Dart-специфика остаётся в `dart_workspace.rs` картой `dart_jobs: HashMap<PathKey, DartAnalyzerState { generation, due_at, job }>` по тому же ключу корня, плюс Flutter-флаг. Карты независимы по назначению: `roots` содержит только корни с процессом (или попыткой старта), `dart_jobs` — любые корни, для которых планировался `dart analyze`, в том числе без открытых документов (`schedule_configured_dart_projects`, `:381-397`, сегодня создаёт `DartWorkspaceState` с `process: None` — таким корням запись в `roots` не нужна). `stop_root` карту `dart_jobs` не трогает: `generation` должна пережить остановку, чтобы отбросить поздний результат; отмена job и сброс `due_at` остаются в `set_dart_workspace_analysis_enabled(false)` (`:352-379`). Fallback-корень Dart — каталог файла (`:71-77`) — сохраняется в `RootedLanguage::Dart.root_for_path` (для Dart всегда `Some`). Публичные методы `LspManager`, которые вызывает приложение (`notify_saved`, `refresh_workspace_diagnostics`, `notify_analysis_configuration_changed`, `drain_closed_dart_documents`, `set_dart_workspace_analysis_enabled`), сохраняют сигнатуры.

`LspManager`: поля `open_dart_files`, `dart_workspaces`, `closed_dart_documents`, `dart_live_diagnostics`, `dart_status` заменяются на `dart: RootedWorkspaces`; добавляется `rust: RootedWorkspaces`. `dart_workspace_diagnostics` (результат `dart analyze`) остаётся.

Диспатч:

```rust
pub enum LangRoute { Python, Rooted(RootedLanguage) }
pub fn language_for_ext(ext: &str) -> Option<LangRoute> {
    match ext { "py" | "pyi" => Some(LangRoute::Python), "dart" => Some(LangRoute::Rooted(RootedLanguage::Dart)),
                "rs" => Some(LangRoute::Rooted(RootedLanguage::Rust)), _ => None }
}
fn rooted_mut(&mut self, lang: RootedLanguage) -> &mut RootedWorkspaces  // dart | rust
```

`ide_process_for_document`, `action_process_for_document`, `notify_open`, `notify_change`, `notify_close`, `notify_saved`, `refresh_workspace_diagnostics` идут через `language_for_ext`; строковых сравнений расширений в `lsp_manager.rs` не остаётся (проверка: `rg -n '"dart"|"py"' src/lsp/lsp_manager.rs` → только внутри `language_for_ext`). `request_ty_completion/signature_help/inlay_hints` переименовываются в `request_ide_*` с гейтом `language_for_ext(ext).is_some()`; вызывающие (`rg -n request_ty_ src`) обновляются.

Инвариант: поля, меняющиеся вместе (open_files ↔ roots ↔ live_diagnostics ↔ status), меняет только `RootedWorkspaces`; `LspManager` не трогает их напрямую; остановка корня — только `stop_root` через `stop_rooted_root` (проверка: `rg -n 'process.take\(\)' src/lsp` → только `rooted_language.rs`).

## 6. Rust-специфика (`rust_workspace.rs`)

* `RUST_ANALYZER_SERVER: LspServerDef { kind: RustAnalyzer, program: "rust-analyzer", override_env: "RRITER_RUST_ANALYZER_PATH", args: &[], language_id: "rust", extensions: &["rs"] }`.
* Корень: `cargo_root_for_path(path, workspaces, locate: &dyn Fn(&Path) -> Option<PathBuf>) -> Option<PathBuf>`, где `locate(dir)` в продакшене — вызов cargo ниже, в unit-тестах — замыкание. Шаг 1 — ближайший `Cargo.toml` вверх от каталога файла, не выше границы workspace-папки редактора (как `nearest_marker` с `stop`); нет → `None`, LSP для файла нет. Шаг 2 — `locate(dir)`: если `cargo` найден (резолюция из кеша, раздел 7), вызов `cargo locate-project --workspace --message-format plain` в каталоге этого `Cargo.toml` (`run_command_output`, таймаут 2 с) — в фоновом job вместе с `rustup which` (раздел 7, «Фоновая резолюция корня»), не в update: ответ — путь `Cargo.toml` корня workspace по правилам самого Cargo (`members`/`exclude`/`package.workspace`, вложенные workspace), его каталог и есть корень, даже если он выше границы папки редактора — иначе RA увидит кусок workspace и диагностика будет неполной. Ошибка/таймаут/нет cargo → остаётся результат шага 1. Разбор TOML своими силами не делаем (`toml`-crate в зависимостях нет — проверить в плане; строковый поиск `[workspace]` не учитывает `exclude` и `package.workspace`). Результат кешируется в `LspManager.rust_roots: HashMap<PathKey /* каталог Cargo.toml шага 1 */, RustRootResolution>` — один job на crate, не на файл; `reconfigure` и `refresh` резолюции очищают кеш. Ключи `roots`/`root_cache` — `PathKey` канонизированного пути: два файла одного workspace из разных папок редактора попадают в один корень и один процесс. Тесты (без cargo, `cargo = None`): member внутри workspace с `[workspace]` в родительском `Cargo.toml` → шаг 1 даёт member (документировано: без cargo — crate, не workspace); одиночный crate → свой каталог; `.rs` без `Cargo.toml` → `None`; `Cargo.toml` выше границы папки редактора на шаге 1 не учитывается. С cargo (тест на `fake` cargo-скрипт через `RRITER_CARGO_PATH` или unit с подменой команды) — member → корень workspace.
* `initialization_options(settings)`: `{"checkOnSave": true, "check": {"command": "check" | "clippy"}}`; при `enabled == false` процесс не стартует вовсе.
* Capabilities для RA — ветка `Ruff | Ty` минус `didChangeWatchedFiles` (RA сам следит за файлами через собственный watcher при `files.watcher = "server"`, что и есть его дефолт без клиентской поддержки), плюс `"window": {"workDoneProgress": true}` не объявляем — тогда RA не создаёт progress-токены и `$/progress` не идёт.
* `experimental/serverStatus` (уведомление `{health: "ok"|"warning"|"error", quiescent: bool, message?}`): объявляется в capabilities `"experimental": {"serverStatusNotification": true}`; парсится в `LspEvent::ServerStatus { server, quiescent: bool, health: ServerHealth /* Ok | Warning | Error */, message: Option<String> }` (неизвестное значение `health` → `Warning`); `poll_processes` пишет `rust.busy = !quiescent`, `rust.health = Some(health)`. Показ: статус-бар — только `LspServerStatus`, без спецтекста; строка Settings — « · занят», пока `busy` (любая фоновая работа RA, не только индексация — отдельного понятия «индексация» нет), и текст `message` при `Warning`/`Error` (например, «не удалось загрузить workspace»); `Error` дополнительно пишется в лог-панель LSP один раз на смену сообщения.
* Диагностика. Запись: `pub struct LiveDiagnostics { pub version: i32, pub root: PathKey, pub items: Arc<[Diagnostic]> }` — общая для Dart и Rust (`dart_live_diagnostics: HashMap<PathBuf, (i32, Arc<[Diagnostic]>)>` переезжает в `dart.live_diagnostics` с этим типом; `root` — корень, чей процесс прислал событие, его знает `poll_processes`). Политика версий в обработчике `LspEvent::Diagnostics` (`lsp_diagnostics_store.rs:189-238`): признак `is_open_file` берётся из `rooted(lang).open_files` для `Rooted(lang)` и из `open_python_files` для Python (сегодня non-Dart всегда смотрит в `open_python_files`); для Rust — RA публикует и для неоткрытых файлов после `cargo check`, и с `version: None`:
  * путь открыт: `version: None` → принять, `stored_version` = текущая версия документа (как Dart, `version.or(current).unwrap_or(0)`); `version: Some(v)` → принять, если `v >= current`, иначе отбросить как устаревшую;
  * путь не открыт: принять всегда, `stored_version = 0` (Dart такие публикации отвергает через `version_is_current` — для Dart поведение не меняется, условие параметризуется языком: `accepts_unopened(lang)` = `Rust → true, Dart → false`).
  Пустой `items` для пути = удаление записи (RA так снимает диагностику). Чтение: `diagnostic_slices_for_abs_path` и `diagnostic_arc_slices_for_abs_path` расширяются до `[ruff, ty, dart, rust, legacy]` (`:481-505, :595-616`), legacy-fallback требует пустоты всех четырёх; `instant_merged_diagnostics_for_abs_path` (`:736`) добавляет rust после dart; `diagnostic_paths` (`:645`) включает `rust.live_diagnostics.keys()`; `clear_diagnostics_for_path` (`:384`) удаляет и rust-запись. Сводка и подсчёты идут через слайсы — без изменений. `result_id` остаётся только у Ty pull (`:227`). Очистка при остановке корня / `set_enabled(false)` / `restart_all`: `RootedWorkspaces::stop_root(root)` удаляет записи с этим `root` и возвращает `true`, если что-то удалил; единственный вызывающий — `LspManager::stop_rooted_root(lang, root)`, который после этого делает `mark_diagnostics_changed(); rebuild_diagnostic_summary(); prune_diag_text_pool()` (как сегодняшняя остановка Dart, `lsp_manager.rs:436-440`). Problems получает rust-диагностику через общий store без изменений в панели.
* Запросы hover/definition/references/completion/signature help/inlay hints/code actions/formatting — общие, через `ide_process_for_document`. `workspace/configuration` от RA — `configuration_response_for(RustAnalyzer, item)` возвращает тот же объект, что `initialization_options` (RA запрашивает секцию `rust-analyzer`).
* Смена `check_command` → `rust.restart_all()`; `enabled` → `rust.set_enabled`.

## 7. Инструмент и установка

* `ToolKind::RustAnalyzer`: `config_key = "rust_analyzer"`, `override_env = "RRITER_RUST_ANALYZER_PATH"`, в `ALL` (8 элементов) и `ToolPaths` (8 слотов); `managed_install_plan = Some(RustAnalyzerArchive)`; `supports_managed_install` расширяется на `RustAnalyzerArchive` (кнопка «Установить»/«Обновить» в строке, как у uv-планов).
* Резолюция — два уровня, потому что общий `resolve_tool_kind` синхронный и вызывается из рендера (`draw_settings_tool_row`, `settings_tool_rows.rs:577`; кеш `TOOL_RESOLUTION_CACHE: RwLock<[Option<ToolResolution>; 7]>` заполняется при первом обращении, `integration.rs:248-261`), а процессы в нём недопустимы:
  * `resolve_rust_analyzer() -> ToolResolution` (в общем кеше, только файловые проверки, как у остальных инструментов): 1) `configured_tool_path(RustAnalyzer)`; 2) env override; 3) `resolve_executable("rust-analyzer")` по PATH; 4) managed: `data_dir()/tools/managed/rust-analyzer/<tag>/rust-analyzer[.exe]`, новейший тег первым (как `managed_dart_executable_in`). Пригодность — `is_file` + бит исполнения, как `is_usable_dart_executable` (`integration.rs:457-469`; версия не запускается). Нет результата → `Missing` в строке Settings и кнопка «Установить». Пункт 3 на rustup-машине находит прокси `~/.cargo/bin/rust-analyzer`, который существует и без установленного компонента и падает при запуске с «not installed for the toolchain».
  * **Фоновая резолюция корня.** Процессы `cargo`/`rustup` в update недопустимы (таймаут 2–3 с подвесил бы кадр), поэтому открытие `.rs` идёт через job по образцу Dart analyzer-job (`ui_waker.spawn_one_shot`, результат забирает poll):
    ```rust
    pub struct RustRootResolution { pub root: PathBuf, pub executable: Result<PathBuf, RustToolError> }
    pub enum RustToolError { NotFound, ComponentMissing, Timeout }
    // LspManager:
    rust_roots: HashMap<PathKey /* каталог Cargo.toml шага 1 */, RustRootEntry>,   // Pending(job) | Ready(RustRootResolution)
    rust_pending_docs: Vec<OpenRootedFile>,                                        // документы, чей crate ещё Pending
    ```
    `notify_open(.rs)`: шаг 1 (файловый подъём до `Cargo.toml`) синхронно — `None` → без LSP; есть `Ready` → `rust.open_document(path, …, root)` сразу; иначе документ в `rust_pending_docs`, при отсутствии job — старт job для этого crate: в worker `cargo locate-project` (раздел 6), затем `rustup which rust-analyzer` в найденном корне (toolchain зависит от каталога: `rust-toolchain.toml`, `rustup override`), `run_command_output`, таймаут 3 с, env `RUSTUP_AUTO_INSTALL=0`, чтобы rustup не начал качать toolchain. Выбор исполняемого файла в job: `resolution.path` задан override'ом/настройкой или лежит вне каталога `rustup` (не прокси) → он; иначе (прокси или пусто) и `rustup` найден → ответ `rustup which`; ошибка → `ComponentMissing` (подсказка в строке Settings: «rustup component add rust-analyzer»); нет ни того ни другого → `NotFound`. Пока job идёт, `rust.status = Starting` (документ «ждёт»), hover/completion по нему возвращают `None`. Poll получает результат → `Ready`, все `rust_pending_docs` этого crate открываются через `rust.open_document` с их текущим текстом и версией (`notify_change` на pending-документе обновляет запись в `rust_pending_docs`, `notify_close` удаляет её); `executable: Err` → `rust.mark_missing()`. Один job на crate, кеш — до `refresh`. `ToolKind::Cargo` не вводится.
  * `cargo` для `locate-project` (раздел 6) и для подсказки в строке — `resolve_executable("cargo")`, результат в `LspManager.rust_cargo: Option<PathBuf>`, обновляется вместе с `refresh_tool_resolutions()`.
  * Сброс: `refresh` резолюции (кнопка в Settings, смена override, конец установки) → `refresh_tool_resolutions()`, очистка `rust_roots` (идущие job'ы отбрасываются по generation, как результаты Dart-job), пересчёт `rust_cargo`, и если есть открытые `.rs` — `rust.restart_all()` через новую резолюцию (открытые документы снова проходят путь `notify_open`).
* Установка `start_rust_analyzer_install`: URL `https://github.com/rust-lang/rust-analyzer/releases/download/<TAG>/rust-analyzer-<triple>.gz`, тройки для linux x86_64/aarch64, macOS x86_64/aarch64, macOS x86_64/aarch64 (Windows: в релизах RA лежит `.zip`, zip-crate в зависимостях нет — managed-установка на Windows отключена, строка Settings подсказывает `rustup component add rust-analyzer`; отдельная карточка Backlog, решение пользователя 03.10.2026); sha256 на каждую платформу — константы в `rust_workspace.rs` (`RUST_ANALYZER_RELEASE_TAG`, `RUST_ANALYZER_ARCHIVES: [(triple, archive, sha256)]`), значения заполняет план по данным из релиза на момент реализации (задача плана с `ds-web`); фазы Downloading → Verifying → Extracting → Done через существующий `ToolInstallPhase`, без авторетраев, отмена через общий `cancel`. Распакованный файл получает `chmod +x` на Unix (как делает установка uv). После установки — `refresh` резолюции (сброс выше).
* Строка Settings: статус как `dart_status_text` — `источник: версия · LSP: статус [· занят] [· сообщение health]`; `rust_cargo.is_none()` → « · cargo не найден: установите rustup»; `RustToolError::ComponentMissing` у любого корня → « · rustup component add rust-analyzer». Всё читается из полей `LspManager`, в кадре процессов нет.

## 8. Настройки

```rust
pub enum RustCheckCommand { Check, Clippy }   // config_value "check" | "clippy", from_config_value с дефолтом Clippy
pub struct RustSettings { pub enabled: bool /* true */, pub check_command: RustCheckCommand /* Clippy */ }
```

Persist — ключ `"rust": {"enabled", "check_command"}` рядом с `"dart"` в `state_persistence.rs`; чтение терпимо к отсутствию ключей. UI — два ряда в разделе Dart/языков `settings_ui.rs`: toggle «Rust: поддержка rust-analyzer» (`UiId::SettingsRustToggleEnabled`), переключатель «Проверка при сохранении: cargo check / clippy» (`UiId::SettingsRustToggleCheckCommand`); обработчики в `ui_settings.rs` по образцу `SettingsDartToggleWorkspaceAnalysis` (`:314-320`): меняют поле, зовут `lsp.set_rust_enabled`/`lsp.set_rust_check_command`, `save_current_config()`.

## 9. Hover

`hover.rs`: порядок решений в точке входа (`:11-24`) становится явным: 1) если текст содержит fence-блок с тегом (` ```rust `, ` ```dart `, ` ```python `/` ```py `), каждый тегированный блок подсвечивается деревом своего языка через `get_ts_config`, а эвристики `looks_like_dart_hover`/`looks_like_python_hover` к таким блокам не применяются; 2) блоки без тега и текст без fence идут по сегодняшнему пути (эвристики → Dart/Python, иначе общий `normalize_hover_text` с Python-подсветкой). Тег сильнее эвристики, потому что он — явное утверждение сервера, а эвристика — догадка. Для RA этого достаточно: его hover — markdown с ` ```rust ` блоками сигнатур и docs. Тест: markdown с блоком ` ```rust ` и строкой `:param ` в docs → блок подсвечен как Rust.

## 10. Ошибки и устойчивость

* RA не найден: `rust.mark_missing()` → `status = Missing`, повторных стартов нет до `refresh` резолюции или установки; строка Settings показывает «не найден» и кнопку «Установить».
* Падение процесса: `restart_attempt_limit` = 4 (как у остальных), затем `Crashed`.
* `.rs` без Cargo-корня: `root_for_path = None` → документ не регистрируется, LSP-запросы по нему возвращают `None` без логов.
* Остановка процесса при закрытии последнего файла корня (`stop_rooted_root`) очищает `live_diagnostics` этого корня и пересобирает сводку Problems (RA после рестарта опубликует заново); второй открытый корень не затрагивается.
* `rustup which` не нашёл компонент → `Missing` для корня, подсказка в строке Settings, повторной попытки до `refresh` нет; таймаут cargo/rustup → fallback (шаг 1 корня / `Missing`), в лог LSP одна строка.
* Смена toolchain (`rust-toolchain.toml`, `rustup override`) подхватывается только по `refresh` или перезапуску RA — автоматически не отслеживается (вне объёма).
* Неизвестные RA-запросы (`window/workDoneProgress/create` и прочие) получают существующий `-32601`; уведомления игнорируются — цикла нет.

## 11. Тесты

* Характеризация Dart (до переноса, задача-страховка): unit на `open/change/close` с фейковым `LspProcess` недоступны (процесс реальный) — поэтому фиксируются текущие тесты `dart_root_*`, парсер machine-вывода, headless `ui_tests_settings_general` (toggles), `ui_tests_settings_tools` (refresh/restart), плюс новый headless-тест жизненного цикла Dart на `fake_lsp_server.py` через `RRITER_DART_PATH`: открыть `.dart` под `pubspec.yaml` → статус `Running`; закрыть → `Disabled`. Все они должны быть зелёными после переноса.
* Unit Rust: `cargo_root_for_path` (5 случаев из раздела 6, `locate` — замыкание), `initialization_options` для обеих настроек, разбор `experimental/serverStatus` (включая неизвестный `health`), `language_for_ext` на все расширения, `diagnostic_slices_for_abs_path` с rust-слайсом и legacy-fallback, политика версий: открытый путь + `None` → принято с текущей версией, открытый + меньшая версия → отброшено, неоткрытый путь → принято (Rust) / отброшено (Dart), пустой `items` → запись удалена; выбор исполняемого файла в job: override → без rustup, прокси + `rustup which` успех/ошибка (команды подменяются замыканиями, как `locate`); pending-документы: открытие двух файлов одного crate до результата job → один job, после результата оба `didOpen` с актуальной версией, закрытый до результата — не открывается; агрегация `status` по двум корням (`Running`+`Starting` → `Starting`, `Crashed` побеждает).
* Headless Rust на fake-сервере через `RRITER_RUST_ANALYZER_PATH` (override — путь к скрипту, ветка rustup не задействована): открытие `.rs` под `Cargo.toml` → `Running`; диагностика от сервера попадает в Problems, в том числе для неоткрытого файла корня и с `version: null`; `.rs` без `Cargo.toml` → статус не меняется; два корня: закрытие последней вкладки первого → его строка Problems исчезает, второй остаётся `Running`; закрытие последней вкладки → `Disabled`; toggle в Settings выключает (Problems пустеет) и включает; смена check/clippy → процесс перезапущен (fake-сервер фиксирует `initializationOptions` в лог-файл, тест читает); строка Settings показывает «не найден» при override на несуществующий путь. Ожидание — `tests_support::wait_until`.
* Headless Dart (характеризация + после переноса): закрытие последней `.dart`-вкладки во время analyzer-job → процесс жив до конца job, затем `Disabled`; результат job после отключения анализа отброшен.
* Fake-сервер (`scripts/fake_lsp_server.py`) расширяется ответами на `initialize` для RA-capabilities и уведомлением `experimental/serverStatus`, если это нужно сценариям; расширение — отдельная задача до тестовых.
* Регрессия Python: существующие LSP-тесты ty остаются зелёными (переименование `request_ty_*`).

## 12. Производительность

Нового per-frame кода нет: статусы и диагностики читаются из полей, заполненных в update; `resolve_tool_kind` в рендере остаётся файловым, процессов не запускает. `cargo locate-project` и `rustup which` — один фоновый job на crate при первом открытии (кеш `rust_roots`), с таймаутами, в update и рендере процессов нет; `refresh` сбрасывает кеш. Ограничение памяти RA — остановка при закрытии последнего `.rs` корня (K4).
