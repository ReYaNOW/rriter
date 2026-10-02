# Rust-поддержка на уровне Python: общий per-root слой LSP и rust-analyzer

Дата: 2026-10-03. Статус: спека утверждена по brainstorming (решения Q1–Q9, подход O2, разделы D1–D7), ждёт ревью консультанта и пользователя.

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
* `src/lsp/lsp_manager.rs` — поля `dart: RootedWorkspaces`, `rust: RootedWorkspaces`; `language_for_ext`; удаление шести строковых армов; обобщение `request_ty_*`.
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
    pub fn root_for_path(self, path: &Path, workspaces: &[PathBuf]) -> Option<PathBuf>;
    pub fn initialization_options(self, settings: &RootedSettings) -> Option<serde_json::Value>;
    pub fn tool_kind(self) -> ToolKind;                          // Dart | RustAnalyzer
}

pub struct OpenRootedFile { pub path: PathBuf, pub root: PathBuf, pub text: Arc<str>, pub version: i32 }
pub struct RootedWorkspaceState { pub root: PathBuf, pub process: Option<LspProcess> }

pub struct RootedWorkspaces {
    pub lang: RootedLanguage,
    pub open_files: HashMap<PathKey, OpenRootedFile>,
    pub roots: HashMap<PathKey, RootedWorkspaceState>,
    pub closed_documents: Vec<PathBuf>,
    pub live_diagnostics: HashMap<PathBuf, (i32, Arc<[Diagnostic]>)>,
    pub status: LspServerStatus,
    pub enabled: bool,
    pub missing: bool,                                            // инструмент не найден — не стартовать повторно
}
```

Методы `RootedWorkspaces` — перенос из `dart_workspace.rs` с заменой «Dart» на `self.lang`:
* `open_document(path, text, version, workspaces, ui_waker)` — находит корень через `lang.root_for_path`; при `None` документ не регистрируется (без LSP). Создаёт `RootedWorkspaceState`, лениво стартует процесс (`ensure_process`), шлёт `didOpen`.
* `change_document(path, text, version)`, `close_document(path)` — как `close_dart_document`: `didClose`, удаление live-диагностики, запись в `closed_documents`; если у корня больше нет открытых файлов и `can_stop_root(root)` (хук потребителя: Dart запрещает при активном job) — `process.take().shutdown()`, `status = Disabled`.
* `process_for_path_mut(path) -> Option<&mut LspProcess>` (ленивый старт как `dart_process_for_path_mut`), `poll_processes(events)`, `mark_missing()`, `reconfigure(workspaces)`, `document_version(path)`, `drain_closed_documents()`, `set_enabled(bool)` (false → shutdown всех процессов, очистка `live_diagnostics`), `restart_all()` (для смены настроек RA).
* Старт процесса: `LspProcess::start_with_executable(def, vec![root], executable_override, ui_waker)` — исполняемый файл выдаёт потребитель через `lang.tool_kind()` и текущую резолюцию (`resolve_tool_executable`), как сегодня `dart_executable_for_root`.

Dart-специфика остаётся в `dart_workspace.rs` картой `dart_jobs: HashMap<PathKey, DartAnalyzerState { generation, due_at, job }>` по тому же ключу корня, плюс Flutter-флаг в `DartRoot`; `can_stop_root` для Dart смотрит `dart_jobs[root].job.is_none()`. Публичные методы `LspManager`, которые вызывает приложение (`notify_saved`, `refresh_workspace_diagnostics`, `notify_analysis_configuration_changed`, `drain_closed_dart_documents`, `set_dart_workspace_analysis_enabled`), сохраняют сигнатуры.

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

Инвариант: поля, меняющиеся вместе (open_files ↔ roots ↔ live_diagnostics ↔ status), меняет только `RootedWorkspaces`; `LspManager` не трогает их напрямую.

## 6. Rust-специфика (`rust_workspace.rs`)

* `RUST_ANALYZER_SERVER: LspServerDef { kind: RustAnalyzer, program: "rust-analyzer", override_env: "RRITER_RUST_ANALYZER_PATH", args: &[], language_id: "rust", extensions: &["rs"] }`.
* Корень: `cargo_root_for_path(path, workspaces)`: от каталога файла вверх до ближайшего `Cargo.toml` (не выше границы workspace-папки, как `nearest_marker` с `stop`); затем продолжать подъём до той же границы и, если выше найден `Cargo.toml`, содержащий секцию `[workspace]` (поиск строки `[workspace]` в начале строки без разбора TOML), взять его. Нет `Cargo.toml` → `None`. Тесты: member внутри workspace → корень workspace; одиночный crate → свой каталог; `.rs` без `Cargo.toml` → `None`; `Cargo.toml` выше границы workspace не учитывается.
* `initialization_options(settings)`: `{"checkOnSave": true, "check": {"command": "check" | "clippy"}}`; при `enabled == false` процесс не стартует вовсе.
* Capabilities для RA — ветка `Ruff | Ty` минус `didChangeWatchedFiles` (RA сам следит за файлами через собственный watcher при `files.watcher = "server"`, что и есть его дефолт без клиентской поддержки), плюс `"window": {"workDoneProgress": true}` не объявляем — тогда RA не создаёт progress-токены и `$/progress` не идёт.
* `experimental/serverStatus` (уведомление `{health: "ok"|"warning"|"error", quiescent: bool, message?}`): объявляется в capabilities `"experimental": {"serverStatusNotification": true}`; парсится в `LspEvent::ServerStatus { server, quiescent, health_message: Option<String> }`; `LspManager` хранит `rust_indexing: bool` (= `!quiescent`) и показывает «индексация…» в статусе (`lsp_ui.rs`, строка Settings) вместе с `Running`; `health == "error"` → сообщение в лог-панель LSP.
* Диагностика: `LspEvent::Diagnostics { server: RustAnalyzer, path, version, items }` → `rust.live_diagnostics[path]` для любого пути (RA публикует и для неоткрытых файлов после `cargo check`); очищается при `Disabled`/остановке корня. В `diagnostic_slices_for_abs_path` массив расширяется до 5: `[ruff, ty, dart, rust, legacy]`; `instant_merged_diagnostics_for_abs_path` добавляет rust после dart. Problems получает их через общий store без изменений в панели.
* Запросы hover/definition/references/completion/signature help/inlay hints/code actions/formatting — общие, через `ide_process_for_document`. `workspace/configuration` от RA — `configuration_response_for(RustAnalyzer, item)` возвращает тот же объект, что `initialization_options` (RA запрашивает секцию `rust-analyzer`).
* Смена `check_command` → `rust.restart_all()`; `enabled` → `rust.set_enabled`.

## 7. Инструмент и установка

* `ToolKind::RustAnalyzer`: `config_key = "rust_analyzer"`, `override_env = "RRITER_RUST_ANALYZER_PATH"`, в `ALL` (8 элементов) и `ToolPaths` (8 слотов); `managed_install_plan = Some(RustAnalyzerArchive)`; `supports_managed_install` расширяется на `RustAnalyzerArchive` (кнопка «Установить»/«Обновить» в строке, как у uv-планов).
* `resolve_rust_analyzer() -> ToolResolution`: 1) `configured_tool_path(RustAnalyzer)`; 2) env override; 3) `rustup which rust-analyzer` (если `resolve_executable("rustup")` найден; команда через `run_command_output` с таймаутом 5 с, результат кешируется в общем resolution cache); 4) `resolve_executable("rust-analyzer")`; 5) managed: `data_dir()/tools/managed/rust-analyzer/<tag>/rust-analyzer[.exe]`, новейший тег первым (как `managed_dart_executable_in`). Проверка пригодности — `rust-analyzer --version` (как `is_usable_dart_executable`).
* Установка `start_rust_analyzer_install`: URL `https://github.com/rust-lang/rust-analyzer/releases/download/<TAG>/rust-analyzer-<triple>.gz`, тройки для linux x86_64/aarch64, macOS x86_64/aarch64, windows x86_64 (`.zip` на Windows — в релизах RA для Windows лежит `.zip`; распаковка одного члена, как `read_archive_member` у pdfium); sha256 на каждую платформу — константы в `rust_workspace.rs` (`RUST_ANALYZER_RELEASE_TAG`, `RUST_ANALYZER_ARCHIVES: [(triple, archive, sha256)]`), значения заполняет план по данным из релиза на момент реализации (задача плана с `ds-web`); фазы Downloading → Verifying → Extracting → Done через существующий `ToolInstallPhase`, без авторетраев, отмена через общий `cancel`. Распакованный файл получает `chmod +x` на Unix (как делает установка uv). После установки — `refresh` резолюции и, если открыты `.rs`, `rust.restart_all()`.
* Строка Settings: статус как `dart_status_text` — `источник: версия · LSP: статус [· индексация]`; если `resolve_executable("cargo")` пуст — добавляется « · cargo не найден: установите rustup». Проверка cargo — один вызов при обновлении резолюции, не в кадре.

## 8. Настройки

```rust
pub enum RustCheckCommand { Check, Clippy }   // config_value "check" | "clippy", from_config_value с дефолтом Clippy
pub struct RustSettings { pub enabled: bool /* true */, pub check_command: RustCheckCommand /* Clippy */ }
```

Persist — ключ `"rust": {"enabled", "check_command"}` рядом с `"dart"` в `state_persistence.rs`; чтение терпимо к отсутствию ключей. UI — два ряда в разделе Dart/языков `settings_ui.rs`: toggle «Rust: поддержка rust-analyzer» (`UiId::SettingsRustToggleEnabled`), переключатель «Проверка при сохранении: cargo check / clippy» (`UiId::SettingsRustToggleCheckCommand`); обработчики в `ui_settings.rs` по образцу `SettingsDartToggleWorkspaceAnalysis` (`:314-320`): меняют поле, зовут `lsp.set_rust_enabled`/`lsp.set_rust_check_command`, `save_current_config()`.

## 9. Hover

`hover.rs`: fence-блоки с тегом (` ```rust `, ` ```dart `, ` ```python `) подсвечиваются деревом соответствующего языка через `get_ts_config`; ветки `looks_like_dart_hover`/`looks_like_python_hover` остаются как есть (их эвристики — для серверов без тегов). Для RA этого достаточно: его hover — markdown с ` ```rust ` блоками сигнатур и docs. Без тега — текущее поведение.

## 10. Ошибки и устойчивость

* RA не найден: `rust.mark_missing()` → `status = Missing`, повторных стартов нет до `refresh` резолюции или установки; строка Settings показывает «не найден» и кнопку «Установить».
* Падение процесса: `restart_attempt_limit` = 4 (как у остальных), затем `Crashed`.
* `.rs` без Cargo-корня: `root_for_path = None` → документ не регистрируется, LSP-запросы по нему возвращают `None` без логов.
* Остановка процесса при закрытии последнего файла корня очищает `live_diagnostics` этого корня (RA после рестарта опубликует заново).
* Неизвестные RA-запросы (`window/workDoneProgress/create` и прочие) получают существующий `-32601`; уведомления игнорируются — цикла нет.

## 11. Тесты

* Характеризация Dart (до переноса, задача-страховка): unit на `open/change/close` с фейковым `LspProcess` недоступны (процесс реальный) — поэтому фиксируются текущие тесты `dart_root_*`, парсер machine-вывода, headless `ui_tests_settings_general` (toggles), `ui_tests_settings_tools` (refresh/restart), плюс новый headless-тест жизненного цикла Dart на `fake_lsp_server.py` через `RRITER_DART_PATH`: открыть `.dart` под `pubspec.yaml` → статус `Running`; закрыть → `Disabled`. Все они должны быть зелёными после переноса.
* Unit Rust: `cargo_root_for_path` (4 случая из раздела 6), `initialization_options` для обеих настроек, разбор `experimental/serverStatus`, `language_for_ext` на все расширения, `diagnostic_slices_for_abs_path` с rust-слайсом.
* Headless Rust на fake-сервере через `RRITER_RUST_ANALYZER_PATH`: открытие `.rs` под `Cargo.toml` → `Running`; диагностика от сервера попадает в Problems; `.rs` без `Cargo.toml` → статус не меняется; закрытие последней вкладки → `Disabled`; toggle в Settings выключает/включает; строка Settings показывает «не найден» при пустом override на несуществующий путь. Ожидание — `tests_support::wait_until`.
* Fake-сервер (`scripts/fake_lsp_server.py`) расширяется ответами на `initialize` для RA-capabilities и уведомлением `experimental/serverStatus`, если это нужно сценариям; расширение — отдельная задача до тестовых.
* Регрессия Python: существующие LSP-тесты ty остаются зелёными (переименование `request_ty_*`).

## 12. Производительность

Нового per-frame кода нет: статусы и диагностики читаются из полей, заполненных в update. `rustup which`/`cargo` резолюция — по событию (старт, refresh, установка), с таймаутом и кешем. Ограничение памяти RA — остановка при закрытии последнего `.rs` корня (K4).
