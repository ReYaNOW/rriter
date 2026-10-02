# Rust-поддержка на уровне Python — план реализации

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `.rs`-файлы в Cargo-проекте получают диагностику, hover, definition, completion, inlay hints, code actions и formatting от rust-analyzer, строку в Settings с managed-установкой — через общий per-root слой LSP, на который сначала переводится Dart без изменения поведения.

**Architecture:** Этап A выносит жизненный цикл «документы ↔ процесс на корень ↔ live-диагностика ↔ статус» из `dart_workspace.rs` в `src/lsp/rooted_language.rs` (`RootedWorkspaces`), Dart становится его первым потребителем (analyzer-job остаётся Dart-специфичным), диспатч по расширению сводится к одной `language_for_ext`. Этап B добавляет Rust: `LspServerKind::RustAnalyzer`, фоновая резолюция корня (`cargo locate-project` + `rustup which` в worker), политика версий диагностик для публикаций по неоткрытым файлам, `ToolKind::RustAnalyzer` с установкой из GitHub releases, настройки `RustSettings`, подсветка fence-блоков в hover.

**Tech Stack:** Rust (nightly, `make test TEST_FILTER=…`), существующий LSP-стек (`LspProcess`, `LspManager`, `lsp_diagnostics_store.rs`), `scripts/fake_lsp_server.py` для headless-тестов, `python3 scripts/rriter_headless.py` для проб UI.

**Spec:** `docs/superpowers/specs/2026-10-03-rust-support-design.md` (разделы и `file:line` ниже ссылаются на неё; исполнитель читает спеку вместе с задачей).

## Как читать план

* Задачи — прозой с решениями, файлами и интерфейсами; полный код — только где сам код и есть решение (сигнатуры общего слоя, политика версий, константы релиза). Исполнитель пишет код сам по спеке.
* Каждая задача — один раздел работы; шаги внутри — коммитами, дерево собирается после каждого коммита. Коммиты делает исполнитель, ветка одна (`rust-support`), без push.
* Строка `Агенты:` задаёт класс implementer'а и нужен ли ревьюер задачи; таски без ревьюера проверяет финальное ревью ветки.
* Тесты: unit — `make test TEST_FILTER=<module path>`; headless — поведение сначала пробуется на prebuilt-бинаре (`python3 scripts/rriter_headless.py shot|dump`, `make fast` только если бинаря нет), `make codex_test` — один раз в конце ветки, у главной сессии. Implementer: не больше 3 прогонов узких тестов на задачу, потом отчёт с падениями.
* Артефакты между задачами (константы релиза, расширение fake-сервера) — в репозитории, не в scratchpad.

## Global Constraints

* Rust-файл вне Cargo-корня — только подсветка, без LSP (спека §1).
* Диагностика Rust — только `publishDiagnostics` от RA (`checkOnSave`), своего раннера `cargo check`/`clippy` нет (K3).
* Процесс RA на Cargo-корень, ленивый старт, остановка при закрытии последней `.rs`-вкладки корня (K4); остановка корня — только `RootedWorkspaces::stop_root` через `LspManager::stop_rooted_root` (K11; проверка `rg -n 'process.take\(\)' src/lsp/dart_workspace.rs src/lsp/rust_workspace.rs src/lsp/rooted_language.rs` → только `rooted_language.rs`; Python/Ty-пути в `lsp_manager.rs`/`lsp_manager_support.rs` не трогаем).
* В update и рендере процессов `cargo`/`rustup` нет: резолюция корня — фоновый job с таймаутами (`cargo locate-project` 2 с, `rustup which` 3 с, env `RUSTUP_AUTO_INSTALL=0`); `resolve_tool_kind` остаётся файловым (K12).
* `toml`-crate не добавляется (K10); новых json-манифестов нет — константы релиза в коде (K6); новых crate'ов без явной необходимости нет (для `.gz`/`.zip` — то, что уже есть в `Cargo.toml`, см. Task 10).
* Строковых сравнений расширений в `lsp_manager.rs` не остаётся: `rg -n '"dart"|"py"' src/lsp/lsp_manager.rs` → только внутри `language_for_ext` (K2).
* `window.workDoneProgress` не объявляем; `experimental.serverStatusNotification: true` объявляем; `didChangeWatchedFiles` для RA не объявляем (спека §6).
* Поведение Dart не меняется: характеризационные тесты Task 2 зелёные после Task 3 и Task 4 без правок тестов.
* Правила AGENTS.md: без `.unwrap()`/`.expect()` в продакшн-путях, без per-frame аллокаций и I/O в рендере, файлы ≤1500 строк (жёсткий предел 1600 — `rooted_language.rs`, `rust_workspace.rs`, `lsp_manager.rs` проверить `wc -l` в каждой задаче), без `#[allow]`, без rustfmt, новые файлы — в `PROJECT_GUIDE.md` §4 (Task 14). Тесты параллельны и per-process: всё, что пишется вне процесса, — с `std::process::id()` в имени.
* Текст UI — русский, как соседние строки Settings; идентификаторы — английские.

## Review Focus

Пять случаев, которые спека подразумевает, но тесты задач не покрывали, пока их сюда не внесли; тест на каждый добавлен задаче-владельцу (помечено «RF-n»):

* RF-1 (Task 7): `rust-toolchain.toml` указывает на неустановленный toolchain — `rustup which` без `RUSTUP_AUTO_INSTALL=0` начал бы качать toolchain; ожидание: job завершается `ComponentMissing`/`Timeout`, UI не ждёт, повтора до `refresh` нет.
* RF-2 (Task 8): RA публикует диагностику для файла из `~/.cargo/registry` (ошибка в зависимости) — путь вне корня; ожидание: запись принята (неоткрытый путь), помечена `root` корня-источника и исчезает при остановке этого корня, не остаётся «вечной» в Problems.
* RF-3 (Task 7): два editor-workspace-а указывают в один Cargo workspace (вложенные папки) — ожидание: один корень, один процесс (ключ `PathKey`), закрытие вкладок из одной папки не гасит процесс, пока открыты вкладки из другой.
* RF-4 (Task 3): переоткрытие `.dart` сразу после закрытия последней вкладки во время analyzer-job — ожидание: процесс не перезапускается (ещё жив), документ `didOpen` в живой процесс; после job при открытом документе процесс не гасится.
* RF-5 (Task 9/13): пользователь выключает Rust в Settings, пока job резолюции корня ещё идёт — ожидание: результат job отброшен (generation), процесс не стартует, Problems пустеет, повторное включение открывает документы заново.

---

## Этап A — общий per-root слой, Dart на нём

### Task 1: fake LSP server — режимы для RA и логирование `initializationOptions`

**Files:**
- Modify: `scripts/fake_lsp_server.py` (сейчас режим задаётся суффиксами в имени файла `sys.argv[0]`: `_crash`, `_long`, `_diagnostics`, `_nodefinition`, `_definitionerror`, `:33-51`; dispatch `:52-104` — `initialize`, `initialized`, `shutdown`, `exit`, hover, definition, inlayHint, `didOpen` только в `_diagnostics`).
- Test: `src/headless/tests.rs` — рядом с `install_fake_ty` (`:16-34`) появляется общий helper `install_fake_lsp(session, kind: ToolKind, basename) -> PathBuf`, `install_fake_ty` становится его вызовом с `ToolKind::Ty`.

**Interfaces:**
- Produces: суффиксы режима `_serverstatus` (после `initialized` сервер шлёт `experimental/serverStatus {"health":"ok","quiescent":false}`, через 300 мс — `{"health":"ok","quiescent":true}`), `_unopened` (вместе с `_diagnostics`: при `didOpen` файла `X.rs` публикует диагностику и для соседнего `sibling.rs` в том же каталоге, `version: null`), `_initlog` (пишет полученные `initializationOptions` одной JSON-строкой в файл `<argv0>.init.jsonl` — append; каждый старт — новая строка; тест читает последнюю строку). Ответ на `workspace/configuration`-запросы сервер не делает (он их не шлёт); на `window/workDoneProgress/create` не рассчитываем. Поддержка нескольких суффиксов в одном имени — уже есть (поиск подстроки).
- Produces: `install_fake_lsp(session, ToolKind, basename)`.

Шаги: добавить три режима по образцу существующих флагов (чтение `mode`, ветки в dispatch); `_unopened` публикует по URI `dirname(uri)/sibling.rs` с одним диагностическим элементом severity 1, текст `fake: unopened sibling`; `_initlog` — запись в `initialize`. Helper в `tests.rs`: копия скрипта под basename в каталоге сессии, `chmod +x`, запись пути в `ToolPaths` для `kind` (как `install_fake_ty` делает для Ty). Проверка — не cargo: `python3 scripts/fake_lsp_server.py` запускается руками через `printf` JSON-RPC кадров в stdin для режимов `_serverstatus`, `_initlog` и `_diagnostics_unopened` (один прогон на режим: для последнего — `didOpen` файла `a.rs` и проверка, что пришли две `publishDiagnostics`, вторая по `sibling.rs` с `"version": null`), результат — в отчёт. `install_fake_lsp` проверяется первым тестом Task 2. Коммит: `fake LSP server: serverStatus, unopened-sibling diagnostics, init-options log`.

Агенты: implementer `ds-low` · reviewer нет.

### Task 2: характеризация Dart перед переносом

**Files:**
- Create: `src/headless/ui_tests_dart_lsp.rs` (регистрация модуля там же, где `ui_tests_lsp_servers.rs`).
- Modify: `src/headless/tests.rs` — только `mod` строка.

**Interfaces:**
- Consumes: `install_fake_lsp(session, ToolKind::Dart, "dart")` из Task 1 (DART_SERVER зовёт `dart language-server`, `protocol.rs:32-39`; fake-сервер аргументы игнорирует — `:42-53`).
- Produces: тесты, которые Task 3 и Task 4 обязаны оставить зелёными без правок:
  * `headless_dart_lifecycle_starts_on_open_and_stops_on_last_close`: fixture-каталог с `pubspec.yaml` и `lib/main.dart` (в имени каталога `std::process::id()`); открыть файл → `wait_until` статуса Dart `Running` (поле `lsp.dart_status`; Dart-переключатель включён как в `ui_tests_settings_general.rs`); закрыть вкладку → статус `Disabled`.
  * `headless_dart_process_survives_close_during_analyzer_job`: workspace analysis включён; `dart analyze` реальный бинарь недоступен — job завершится быстро с ошибкой, поэтому тест фиксирует наблюдаемый инвариант по полям: закрыть последнюю вкладку, пока `dart_workspaces[root].job.is_some()` (job стартует через 1 с после открытия — ждать `wait_until` появления job, затем закрыть), процесс жив (`process.is_some()`), после завершения job — `process.is_none()`, статус `Disabled`. Если job стартует и завершается быстрее, чем тест успевает закрыть вкладку, — заменить на unit-тест в `dart_workspace.rs` с ручным `DartAnalyzerJob` (создать `OneShot` через `ui_waker.spawn_one_shot` с `thread::sleep(500 мс)`); решение принимает implementer, в отчёте — какой вариант и почему. Это RF-4: добавить переоткрытие файла во время job → `process` тот же (сравнить `Option::is_some` до/после и что `didOpen` ушёл — по `server_logs`/счётчику `.starts` fake-сервера: перезапуска нет, `starts == 1`).
  * `headless_dart_two_roots_are_independent`: два каталога с `pubspec.yaml`, по файлу в каждом → две записи `dart_workspaces` с процессами; закрыть первый → у первого `process.is_none()`, у второго `is_some()`, `dart_status` остаётся `Running`.
  * `headless_dart_live_diagnostics_reach_problems`: fake `dart_diagnostics` (суффикс) публикует при `didOpen` → `wait_until` появления строки `ProblemJump(1)` в `dump` (как `ui_tests_problems.rs`); закрыть вкладку → строки нет.
- Поля `LspManager`, которые читают тесты, доступны из `src/headless` (тот же crate; при `pub(super)` добавить `pub(crate)` геттеры `dart_root_state_for_test` только под `#[cfg(test)]`).

Не больше 3 прогонов `make test TEST_FILTER=ui_tests_dart_lsp`; поведение сначала пробовать на prebuilt-бинаре через `python3 scripts/rriter_headless.py` (`docs/headless.md`), fake-сервер подставлять через настройки ToolPaths в конфиге сессии. Коммит: `headless: Dart LSP lifecycle characterization`.

Агенты: implementer `ds-low-agentic` · reviewer нет.

### Task 3: `rooted_language.rs` — модуль, unit-тесты, параметр `init_options`

(Перенос Dart на этот модуль — Task 15, идёт сразу после Task 3; разделено по размеру.)

**Files:**
- Create: `src/lsp/rooted_language.rs` (спека §5 целиком).
- Modify: `src/lsp/lsp_process.rs:1112-1121` (`start_with_executable` получает `init_options: Option<&serde_json::Value>`), `src/lsp/protocol/protocol_json_rpc_encoding.rs:140-144, 249-255` (`make_initialize_for_server(server, id, workspaces, init_options)`; Dart-константа переезжает в `dart_workspace::DART_INIT_OPTIONS` и приходит параметром — в этой задаче вызывающие Dart-кода передают её явно, поведение байт-в-байт то же), `src/lsp/mod.rs` (`mod rooted_language`).
- Test: `#[cfg(test)] mod tests` в `rooted_language.rs`; `src/lsp/protocol_tests.rs:1084-1168` (существующие тесты сериализации `initialize`) — добавить проверку, что Dart-`initializationOptions` сериализуются как прежде через параметр и что `None` не добавляет ключ.

Поля `RootedWorkspaces` — приватные (кроме `lang`); чтение — геттерами `status()`, `health()`, `busy()`, `enabled()`, `missing()`, `init_options()`, `open_files()`, `roots()`, `live_diagnostics()`; мутации — только методами ниже плюс `insert_live_diagnostics(path, LiveDiagnostics)` и `remove_live_diagnostics(path) -> bool` (их зовёт обработчик store в Task 15/8). Причина: инвариант «меняет только владелец» без приватности держится на дисциплине, а не на компиляторе.

**Interfaces (Produces — Task 15, 4, 7, 8 опираются на эти имена; в листинге `pub` поля читать как «приватное поле + геттер того же имени»):**

```rust
// src/lsp/rooted_language.rs
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum RootedLanguage { Dart }                 // вариант Rust добавляет Task 7 (вместе с его server_def/extensions/tool_kind)

impl RootedLanguage {
    pub fn server_def(self) -> &'static LspServerDef;        // Dart → DART_SERVER
    pub fn extensions(self) -> &'static [&'static str];
    pub fn tool_kind(self) -> crate::platform::ToolKind;
}

pub struct OpenRootedFile { pub path: PathBuf, pub root: PathBuf, pub text: Arc<str>, pub version: i32 }
pub struct RootedWorkspaceState { pub root: PathBuf, pub process: Option<LspProcess> }
pub struct LiveDiagnostics { pub version: i32, pub root: PathKey, pub items: Arc<[Diagnostic]> }

pub struct RootedWorkspaces {
    pub lang: RootedLanguage,
    pub open_files: HashMap<PathKey, OpenRootedFile>,
    pub roots: HashMap<PathKey, RootedWorkspaceState>,
    pub closed_documents: Vec<PathBuf>,
    pub live_diagnostics: HashMap<PathBuf, LiveDiagnostics>,
    pub init_options: Option<serde_json::Value>,
    pub status: LspServerStatus,
    pub health: Option<ServerHealth>,                 // enum ServerHealth { Ok, Warning, Error } — объявить здесь, заполняет Task 7
    pub busy: bool,
    pub enabled: bool,
    pub missing: bool,
}

impl RootedWorkspaces {
    pub fn new(lang: RootedLanguage, enabled: bool) -> Self;
    pub fn open_document(&mut self, path: PathBuf, text: Arc<str>, version: i32, root: PathBuf, executable: Option<PathBuf>, ui_waker: &UiWaker) -> PathKey;
    pub fn change_document(&mut self, path: &Path, text: Arc<str>, version: i32);
    pub fn close_document(&mut self, path: &Path) -> Option<PathKey>;     // Some(root) — у корня не осталось открытых файлов
    pub fn stop_root(&mut self, root: &PathKey) -> bool;                  // true — удалил live-диагностику
    pub fn root_has_open_files(&self, root: &PathKey) -> bool;
    pub fn process_for_path_mut(&mut self, path: &Path, executable: impl FnOnce(&Path) -> Option<PathBuf>, ui_waker: &UiWaker) -> Option<&mut LspProcess>;
    pub fn poll_processes(&mut self, events: &mut Vec<LspEvent>, ...);   // форма — как сегодняшний poll_dart_processes; в конце recompute_status()
    pub fn mark_missing(&mut self);
    pub fn reconfigure(&mut self, workspaces: &[PathBuf]);
    pub fn document_version(&self, path: &Path) -> Option<i32>;
    pub fn drain_closed_documents(&mut self) -> Vec<PathBuf>;
    pub fn set_enabled(&mut self, enabled: bool) -> bool;                 // false → stop_root всех; возвращает «диагностика менялась»
    pub fn restart_all(&mut self, executable: impl Fn(&Path) -> Option<PathBuf>, ui_waker: &UiWaker) -> bool;
    pub fn set_init_options(&mut self, value: Option<serde_json::Value>) -> bool; // true — изменилось (вызывающий решает про restart_all)
    fn ensure_process(&mut self, root: &PathKey, executable: Option<PathBuf>, ui_waker: &UiWaker);
    fn recompute_status(&mut self, has_pending: bool);                    // правило из спеки §5; has_pending → Starting (Rust, Task 7; Dart передаёт false)
}
```

Unit-тесты Task 3 (без процесса: `ensure_process` не вызывается, если `executable` — `None`, это и есть путь `mark_missing`): `close_document` возвращает корень только для последнего файла корня; `stop_root` удаляет только записи своего `root` из `live_diagnostics`; `recompute_status(false)`: `Crashed` побеждает `Running`, `Starting` побеждает `Running`, нет корней → `Disabled`, `!enabled` → `Disabled`, `missing` → `Missing`; `recompute_status(true)` при пустых корнях → `Starting`; `set_init_options` возвращает `true` только при изменении. Прогоны: `make test TEST_FILTER=rooted_language`, `make test TEST_FILTER=protocol_tests`, ≤3. Коммиты: 1) модуль + unit; 2) параметр `init_options` в process/protocol. Коммит-сообщения: `lsp: RootedWorkspaces per-root layer`, `lsp: initializationOptions passed as a parameter`.

Агенты: implementer `ds-low` · reviewer нет (модуль без потребителей до Task 15; его проверяет ревью Task 15).

### Task 15: перенос Dart на `RootedWorkspaces` и store

**Files:**
- Modify: `src/lsp/dart_workspace.rs` (остаются: `dart_root_for_path` и `DartRoot`, analyzer-job и `dart_jobs`, closing labels, Flutter-флаг, парсер `dart analyze`, `DART_INIT_OPTIONS`), `src/lsp/lsp_manager.rs:21-70` (поля), `src/lsp/lsp_diagnostics_store.rs` (тип записи live-диагностики Dart, параметр `root`).
- Test: Task 2 зелёный без правок; существующие unit `dart_workspace.rs` и store-тесты Dart в `lsp_tests.rs` зелёные.

**Interfaces (Consumes):** всё из Task 3. **Produces:** `LspManager::{stop_rooted_root, rooted_root_may_stop, dart_status()}`, поле `dart: RootedWorkspaces`.

Решения, которые implementer не принимает сам:
* Исполняемый файл передаётся замыканием/значением от `LspManager` (Dart — как сегодня через `crate::platform::resolve_dart_for_workspace(Some(root))`, `integration.rs:325`; где именно `ensure_dart_process` берёт путь — посмотреть в `dart_workspace.rs:398-419` и перенести без изменений), слой сам резолюцию не делает.
* `stop_root` — единственный `process.take()` в `src/lsp` (Global Constraints); `LspManager::stop_rooted_root(lang, root)` после него: `mark_diagnostics_changed(); rebuild_diagnostic_summary(); prune_diag_text_pool()` только если вернулось `true`.
* `LspManager`: `open_dart_files`, `dart_workspaces` (часть с процессом), `closed_dart_documents`, `dart_live_diagnostics`, `dart_status` → `dart: RootedWorkspaces`; `dart_status` остаётся публичным геттером `dart_status()` → `self.dart.status`, чтобы UI не менять (`lsp_ui.rs:370-376`, `settings_tool_rows.rs`). `dart_workspace_diagnostics` и `dart_workspace_analysis_enabled` остаются. `dart_disabled`/`dart_unavailable` → `dart.enabled`/`dart.missing`.
* `DartWorkspaceState` сокращается до `DartAnalyzerState { generation, due_at, job }` в карте `dart_jobs: HashMap<PathKey, DartAnalyzerState>`; `schedule_dart_workspace_analysis` создаёт запись там; `can_stop_root` не вводится — решение в `LspManager::rooted_root_may_stop(lang, root)`: Dart → `dart_jobs.get(root).is_none_or(|s| s.job.is_none())`.
* Poll Dart после job (`dart_workspace.rs:292-307`): `if !self.dart.root_has_open_files(&key) { self.stop_rooted_root(RootedLanguage::Dart, &key) }`.
* В store (`lsp_diagnostics_store.rs:189-238, 481-505, 595-616, 645-658, 736-779, 384-402`) Dart-ветки читают `self.dart.live_diagnostics` и `.items`/`.version`; `is_open_file` для Dart — `self.dart.open_files.contains_key`. Обработчик `LspEvent::Diagnostics` получает дополнительный параметр `root: Option<&PathKey>` — `poll_processes` слоя знает, процесс какого корня прислал событие, и передаёт его (Python-серверы — `None`); он пишется в `LiveDiagnostics.root`. Политика версий Dart — без изменений (параметризация под Rust — Task 8).
* `rg -n 'process.take\(\)' src/lsp/dart_workspace.rs src/lsp/rooted_language.rs` → только `rooted_language.rs`; `wc -l src/lsp/lsp_manager.rs src/lsp/dart_workspace.rs src/lsp/rooted_language.rs` — ни один не выше 1500.

Коммиты внутри задачи: 1) `LspManager`/`dart_workspace.rs` на `RootedWorkspaces`; 2) store на `LiveDiagnostics` + параметр `root`. Прогоны: `make test TEST_FILTER=dart` (unit Dart + Task 2), затем `make test TEST_FILTER=lsp_tests`, не больше 3 всего. Коммит-сообщения: `Dart LSP: lifecycle moved to RootedWorkspaces`, `diagnostics store: LiveDiagnostics with source root`.

Агенты: implementer `ds-low-agentic` · reviewer `ds-high` — high: перенос владения состоянием между модулями с инвариантом «один stop», ревьюер решает, не изменилось ли поведение Dart в путях, которых тесты Task 2 не видят (job без открытых документов, `set_dart_workspace_analysis_enabled`, closing labels).

### Task 4: `language_for_ext` и диспатч без строковых сравнений

**Files:**
- Modify: `src/lsp/lsp_manager.rs:573-593` (`ide_process_for_document`, `action_process_for_document`), `:596-635` (`notify_open`), `:638-690` (`notify_change`), `:846-883` (`notify_close`), `notify_saved`, `refresh_workspace_diagnostics`, `is_python_ext` (`:83`) — заменяется на `language_for_ext`; `request_ty_completion` → `request_ide_completion`, `request_ty_signature_help` → `request_ide_signature_help`, `request_ty_inlay_hints` → `request_ide_inlay_hints`.
- Modify callers: `src/app/autocomplete/autocomplete_ty_flow_methods.rs:850, 852`, `src/app/autocomplete/autocomplete_detail_flow_methods.rs:616` (переименование); `rg -n 'request_ty_' src` после правки → пусто.
- Test: `#[cfg(test)]` в `lsp_manager.rs` или `lsp_tests.rs`: `language_for_ext` на `py`, `pyi`, `dart`, `rs`, `PY` (регистр — как сегодня: если сегодня `ext` уже приведён к нижнему регистру вызывающим, тест фиксирует это), `txt` → `None`.

**Interfaces:**

```rust
pub enum LangRoute { Python, Rooted(RootedLanguage) }
pub fn language_for_ext(ext: &str) -> Option<LangRoute>;    // "py"|"pyi" → Python; "dart" → Rooted(Dart); "rs" → Rooted(Rust) добавляет Task 7 (здесь — только Dart)
fn rooted_mut(&mut self, lang: RootedLanguage) -> &mut RootedWorkspaces;
fn rooted(&self, lang: RootedLanguage) -> &RootedWorkspaces;
```

Правило: все перечисленные методы начинаются с `let Some(route) = language_for_ext(ext) else { return … }` и `match route { LangRoute::Python => …, LangRoute::Rooted(lang) => { let rooted = self.rooted_mut(lang); … } }`; Dart-специфика внутри `Rooted` ветки (планирование job при открытии, closing labels) — через `if lang == RootedLanguage::Dart`. `request_ide_*` гейт — `language_for_ext(ext).is_some()` и процесс через `ide_process_for_document`: для Python поведение прежнее; для Dart это намеренное расширение (completion/signature help/inlay hints уходят в Dart-процесс, если вызывающий их запросит) — зафиксировать unit-тестом: `request_ide_completion` для `.dart` без процесса возвращает `None` без паники, для `.txt` — `None`. Проверка: `rg -n '"dart"|"py"' src/lsp/lsp_manager.rs` → только внутри `language_for_ext`. Python-ветки по существу не меняются (это рефакторинг диспатча); регрессия Python — существующие ty-тесты `ui_tests_lsp_servers.rs` и autocomplete-тесты (`make test TEST_FILTER=autocomplete`, один из трёх прогонов).

Прогоны: `make test TEST_FILTER=lsp` (lsp unit + headless ty/Dart), не больше 3. Коммит: `LspManager: language_for_ext dispatch, request_ide_* names`.

Агенты: implementer `ds-low-agentic` · reviewer нет (сквозной рефакторинг без изменения поведения; ловит финальное ревью и тесты Task 2 плюс ty-тесты).

## Этап B — Rust

### Task 5: протокол — `LspServerKind::RustAnalyzer`, capabilities, `experimental/serverStatus`

**Files:**
- Modify: `src/lsp/protocol.rs` (`LspServerKind` `:201-205`, `name()`, `restart_attempt_limit`; константа `RUST_ANALYZER_SERVER` рядом с `TY_SERVER`/`DART_SERVER` `:23-39`; `LspEvent` `:121-198` — новый вариант), `src/lsp/protocol/protocol_json_rpc_encoding.rs` (`make_initialize_for_server` `:140-264` — ветка capabilities для `RustAnalyzer`; `configuration_response_for` `:487-490` — получает `init_options: Option<&Value>` и для `RustAnalyzer` возвращает его клон (или `null`); `dispatch_frame_for_server` `:601-608` — разбор `experimental/serverStatus` `:707` рядом с `publishDiagnostics`).
- Test: `#[cfg(test)]` в `protocol_json_rpc_encoding.rs` (там уже есть тесты на `make_initialize_for_server`? — `rg -n 'fn .*initialize' src/lsp/protocol/*tests*`; если нет — завести рядом с существующими протокольными тестами).

**Interfaces (Produces):**

```rust
// protocol.rs
pub enum LspServerKind { Ruff, Ty, Dart, RustAnalyzer }      // name() → "rust-analyzer"; restart_attempt_limit → 4
pub(super) const RUST_ANALYZER_SERVER: LspServerDef = LspServerDef {
    kind: LspServerKind::RustAnalyzer, program: "rust-analyzer", override_env: "RRITER_RUST_ANALYZER_PATH",
    args: &[], language_id: "rust", extensions: &["rs"],
};
#[derive(Clone, Copy, PartialEq, Eq, Debug)] pub enum ServerHealth { Ok, Warning, Error }   // объявлен в rooted_language.rs (Task 3), здесь используется
LspEvent::ServerStatus { server: LspServerKind, quiescent: bool, health: ServerHealth, message: Option<String> }
```

Решения:
* Capabilities RA = ветка `Ruff | Ty` без `workspace.didChangeWatchedFiles`, без `window.workDoneProgress`, плюс `"experimental": {"serverStatusNotification": true}`. `initializationOptions` — из параметра `init_options` (введён в Task 3), для RA это объект секции `rust-analyzer` (`{"checkOnSave": true, "check": {"command": …}}`, строит Task 7).
* `experimental/serverStatus`: `health` строкой `"ok"|"warning"|"error"`, иное → `Warning`; отсутствие `quiescent` → `false`; `message` — опционально.
* `workspace/configuration` для RA: на каждый `item` отдаётся клон `init_options` (RA запрашивает секцию `rust-analyzer`), `init_options == None` → `null`.
* `RootedLanguage::Rust` здесь ещё не существует — Task 5 не трогает `rooted_language.rs`; все `match` по `LspServerKind` в репозитории (`rg -n 'LspServerKind::Dart' src` даст список) получают арм `RustAnalyzer` с тем же поведением, что `Ty`, если не сказано иное (статус-строки, логи `server_logs` по `name()`).

Unit-тесты: `make_initialize_for_server(RustAnalyzer, …, Some(&json!({"checkOnSave": true})))` → в JSON есть `experimental.serverStatusNotification == true`, нет `window.workDoneProgress`, нет `didChangeWatchedFiles`, `initializationOptions.checkOnSave == true`; разбор трёх вариантов `serverStatus` (ok/quiescent true; error с message; неизвестный health → Warning); `configuration_response_for(RustAnalyzer, item, Some(opts))` → `opts`.

Прогоны: `make test TEST_FILTER=protocol`, ≤3. Коммит: `LSP protocol: RustAnalyzer server kind, capabilities, experimental/serverStatus`.

Агенты: implementer `ds-low` · reviewer нет.

### Task 6: `ToolKind::RustAnalyzer` и файловая резолюция

**Files:**
- Modify: `src/platform/integration.rs` — `ToolKind` `:15-25` (+`RustAnalyzer` перед `Pdfium`), `ALL` 7 → 8, `index()` (`RustAnalyzer => 7`, `Pdfium => ToolKind::ALL.len()`), `config_key = "rust_analyzer"`, `override_env = "RRITER_RUST_ANALYZER_PATH"`, `managed_install_plan = Some(ManagedToolInstallPlan::RustAnalyzerArchive)` (новый вариант enum `:7-12`), `supports_managed_install` — добавить `RustAnalyzerArchive` в `matches!`; `ToolPaths { paths: [Option<PathBuf>; 7] }` `:132` → 8 (лучше `ToolKind::ALL.len()`, если тип это позволяет без `const`-проблем; иначе литерал 8); `TOOL_RESOLUTION_CACHE` `:208-212` — размер по `ALL.len()`; `resolve_tool_kind_uncached` `:263` — PATH-кандидаты `(ToolKind::RustAnalyzer, PlatformKind::Windows) => &["rust-analyzer.exe"]`, иначе `&["rust-analyzer"]`; после PATH — managed-каталог `data_dir()/tools/managed/rust-analyzer/<tag>/rust-analyzer[.exe]`, новейший `<tag>` первым (по образцу `managed_dart_executable_in`, `:536`), источник `ToolPathSource::Managed`.
- Modify: `src/state_persistence.rs` — если `ToolPaths` сериализуется по `config_key` в цикле по `ALL`, правок нет; если по явному списку — добавить `rust_analyzer` (проверить `rg -n 'config_key\(\)' src/state_persistence.rs`).
- Modify: `src/render_view/settings_tool_rows.rs:235` — цикл по `ALL` даст строку RA автоматически; подпись строки (имя инструмента) — там, где подписи остальных (`rg -n '"Dart"' src/render_view/settings_tool_rows.rs`), текст `rust-analyzer`. Статус-текст RA — Task 9; в этой задаче строка показывает путь/источник как у `Git`/`Ruff`.
- Test: существующие тесты `integration.rs`/`platform` на `ToolPaths` round-trip (`rg -n 'ToolPaths' src --glob '*tests*'`) расширить на `RustAnalyzer`; unit: `managed_rust_analyzer_executable_in(root)` выбирает новейший тег среди `2026-09-28`, `2026-08-01` (создать каталоги в temp с `std::process::id()`); `config_key`/`override_env`/`index` для нового варианта; приоритет резолюции — тест с временным каталогом: configured path побеждает PATH-кандидата, managed-файл берётся только при пустых configured/env/PATH (PATH подменить через существующий механизм тестов `resolve_executable`, если он есть — `rg -n 'fn resolve_executable' -A3 src/platform`; иначе тестировать `resolve_rust_analyzer_in(configured, env, path_hit, managed_root)` как чистую функцию над `Option`-входами).

**Interfaces (Produces):** `ToolKind::RustAnalyzer`, `ManagedToolInstallPlan::RustAnalyzerArchive`, `fn managed_rust_analyzer_executable_in(root: &Path, platform: PlatformKind) -> Option<PathBuf>`, `pub const MANAGED_RUST_ANALYZER_DIR: &str = "rust-analyzer"` (относительно `data_dir()/tools/managed/`).

Решение: кнопка «Установить» появится в строке сразу (`supports_managed_install`), но до Task 10 `ToolInstaller::start(RustAnalyzer)` должен возвращать `Err("установка rust-analyzer появится в следующем шаге")`, а не падать в uv-ветку — добавить явную ветку в `install_tool` (`tool_installer_download.rs:143`), Task 10 её заменит. Пригодность файла — `is_file` + бит исполнения на Unix, общий helper с `is_usable_dart_executable` (`:457-469`) → переименовать в `is_usable_executable` и вызывать из обоих мест.

Прогоны: `make test TEST_FILTER=platform`, ≤3. Коммит: `ToolKind::RustAnalyzer: config key, override env, PATH/managed resolution`.

Агенты: implementer `ds-low` · reviewer нет.

### Task 7: `rust_workspace.rs` — Cargo-корень, фоновая резолюция, `RootedLanguage::Rust`

(Интеграция в `LspManager` — Task 16, сразу после Task 7; разделено по размеру.)

**Files:**
- Create: `src/lsp/rust_workspace.rs`.
- Modify: `src/lsp/rooted_language.rs` (вариант `Rust`: `server_def → RUST_ANALYZER_SERVER`, `extensions → ["rs"]`, `tool_kind → ToolKind::RustAnalyzer`), `src/lsp/mod.rs`.
- Test: `#[cfg(test)]` в `rust_workspace.rs` (резолвер, job-тело с подменёнными командами, `initialization_options`, RF-1, RF-3-часть про `locate`).

### Task 16: Rust в `LspManager` — маршрутизация, pending-документы, статус, enable/refresh

**Files:**
- Modify: `src/lsp/lsp_manager.rs` (поля `rust: RootedWorkspaces`, `rust_roots`, `rust_pending_docs`, `rust_tools`, `rust_roots_generation`; `language_for_ext`: `"rs" → Rooted(Rust)`; `notify_open/change/close` ветки для Rust; `poll_rust_root_jobs`; обработка `LspEvent::ServerStatus`; `set_rust_enabled`, `set_rust_init_options`, `refresh_rust_resolution()`, `reopen_rust_documents`), при превышении 1500 строк — новый `src/lsp/lsp_manager_rust.rs`.
- Test: unit в `lsp_tests.rs` на pending/generation (список в конце раздела); поведение в headless — Task 13. Consumes всё из Task 7; Produces — методы `LspManager` из таблицы интерфейсов.

**Interfaces (Produces):**

```rust
// rust_workspace.rs
pub fn cargo_root_for_path(path: &Path, workspaces: &[PathBuf], locate: &dyn Fn(&Path) -> Option<PathBuf>) -> Option<PathBuf>;
pub fn initialization_options(check: RustCheckCommand) -> serde_json::Value;   // {"checkOnSave": true, "check": {"command": "check"|"clippy"}}
pub struct RustRootResolution { pub root: PathBuf, pub executable: Result<PathBuf, RustToolError>, pub version: Option<String> }
#[derive(Clone, Copy, PartialEq, Eq, Debug)] pub enum RustToolError { NotFound, ComponentMissing, Timeout }
pub(super) struct RustRootJob { generation: u64, rx: crate::ui_waker::OneShot<RustRootResolution> }
pub(super) enum RustRootEntry { Pending(RustRootJob), Ready(RustRootResolution) }
pub(super) struct RustTools { pub cargo: Option<PathBuf>, pub rustup: Option<PathBuf>, pub resolution: ToolResolution /* resolve_tool_kind(RustAnalyzer) */ }
pub(super) fn resolve_rust_root_blocking(crate_dir: &Path, tools: &RustTools) -> RustRootResolution;   // тело job: locate → rustup which → `rust-analyzer --version`
```

`RustCheckCommand` объявляется в Task 9 (`src/app/rust_settings.rs`); чтобы Task 7 не зависел от Task 9, `initialization_options` принимает `check_command: &str` (`"check"`/`"clippy"`), а `RustSettings` в Task 9 отдаёт строку через `config_value()`.

Решения (спека §6–§7, §10):
* `notify_open(.rs)`: шаг 1 — `nearest_cargo_toml_dir(path, workspaces)` синхронно (подъём до `Cargo.toml`, не выше границы папки редактора, которой принадлежит файл; файл вне всех папок — подъём до корня ФС не делаем, ограничение — 32 уровня); `None` → документ не регистрируется. `rust_roots.get(crate_dir)`: `Ready(res)` с `Ok(exe)` → `rust.open_document(path, text, version, res.root, Some(exe), ui_waker)`; `Ready` с `Err` → в `rust_pending_docs` не кладём, `rust.mark_missing()` (повторная попытка — только после `refresh_rust_resolution`); `Pending` → документ в `rust_pending_docs` (`OpenRootedFile` с `root = crate_dir` как временный ключ); отсутствует → старт job: `ui_waker.spawn_one_shot("rriter-rust-root", move || resolve_rust_root_blocking(&crate_dir, &tools))`, запись `Pending`, документ в `rust_pending_docs`; статус на время job — `Starting`: `recompute_status(has_pending: bool)` получает параметр, который `LspManager` передаёт как `!rust_pending_docs.is_empty()` (Dart — всегда `false`); сигнатура `recompute_status` в Task 3 сразу с этим параметром.
* Job (`resolve_rust_root_blocking`): 1) `cargo` есть → `run_command_output(cargo, ["locate-project", "--workspace", "--message-format", "plain"], cwd = crate_dir, timeout 2 s)`; успех → `root = parent(stdout.trim())`, иначе `root = crate_dir`. 2) исполняемый файл: `tools.resolution.path` задан и (`source != Path` или путь не внутри каталога `rustup`'а) → он; иначе `rustup` есть → `run_command_output(rustup, ["which", "rust-analyzer"], cwd = root, env RUSTUP_AUTO_INSTALL=0, timeout 3 s)`: успех → путь, непустой stderr/ненулевой код → `ComponentMissing`, таймаут → `Timeout`; ни `resolution.path`, ни `rustup` → `NotFound`. 3) `version`: `run_command_output(exe, ["--version"], timeout 2 s)` → первая строка stdout. `run_command_output` — из `platform` (сигнатуру и таймаут посмотреть по `rg -n 'pub fn run_command_output' src/platform`); если таймаута у него нет — обернуть в `platform::ManagedChild` с `wait_timeout`-циклом; процесс по таймауту убивается через process-tree API платформы (AGENTS.md §5).
* Poll: в `poll_rust_root_jobs()` (вызывается из общего poll `LspManager`) — `rx.poll()` как `poll_dart_workspace_diagnostics` (`dart_workspace.rs:269-310`); `Ready(res)`: если `generation != rust_roots_generation` → отбросить; иначе запись `Ready`, все `rust_pending_docs` с этим `crate_dir` → `rust.open_document(...)` с `res.root` и `res.executable.ok()`; `Err` → `rust.mark_missing()`, документы из pending удаляются; `Closed` → как `Err(NotFound)` + строка в лог.
* `notify_change` для pending-документа обновляет `text`/`version` в `rust_pending_docs`; `notify_close` удаляет его оттуда; иначе — через `rust.change_document/close_document` + `rooted_root_may_stop(Rust) == true` → `stop_rooted_root`.
* `LspEvent::ServerStatus { server: RustAnalyzer, .. }` → `rust.busy`, `rust.health`, `message` → при `Error` одна запись в `server_logs["rust-analyzer"]` на смену текста (хранить `last_health_message: Option<String>`).
* `set_rust_enabled(bool)`: `rust.set_enabled(b)`; `false` → `rust_roots_generation += 1` (RF-5: результат идущего job отбрасывается), `rust_pending_docs.clear()`, диагностика чистится в `set_enabled` → `stop_rooted_root` путь (сводка пересобирается); `true` → для всех открытых вкладок `.rs` (список передаёт вызывающий из `App`: `lsp.reopen_rust_documents(iter of (path, text, version))`) снова `notify_open`.
* `set_rust_init_options(value)`: `rust.set_init_options(Some(value))` вернул `true` → `rust.restart_all(executable_for_root, ui_waker)`, где `executable_for_root(root)` ищет `Ready` запись с этим `root` в `rust_roots`.
* `refresh_rust_resolution()`: `refresh_tool_resolutions()`; `rust_tools = RustTools { cargo: resolve_executable("cargo"), rustup: resolve_executable("rustup"), resolution: resolve_tool_kind(RustAnalyzer) }` (это update-путь, вызывается по кнопке/настройке/установке — не рендер); `rust_roots.clear()`, `rust_roots_generation += 1`, `rust.missing = false`; открытые `.rs` → заново `notify_open` (тот же `reopen_rust_documents`). Первичное заполнение `rust_tools` — лениво при первом `notify_open(.rs)`.
* `RF-1`: тест job с `rustup`-заглушкой (замыкание/скрипт, который спит 10 с) → `Timeout` за ≤3.5 с; `RF-3`: unit — два `crate_dir` в разных editor-папках, `locate` возвращает один и тот же корень → `rust.roots.len() == 1` после обоих `open_document`.

Unit-тесты (`rust_workspace.rs`): `cargo_root_for_path` 5 случаев из спеки §6 (`locate` — замыкание); `initialization_options("clippy")` → `check.command == "clippy"`; `resolve_rust_root_blocking` с подменёнными командами: вынести выполнение команд за трейт/замыкание `RunCmd = dyn Fn(&Path, &[&str], &Path) -> Result<String, RustToolError>` и в тесте подставлять ответы: override без rustup; прокси + `which` успех; прокси + `which` ошибка → `ComponentMissing`; таймаут → `Timeout`; RF-1, RF-3 выше; pending: два документа одного crate до результата → один job (поле `rust_roots.len() == 1`, `rust_pending_docs.len() == 2`), после результата оба в `rust.open_files`; документ, закрытый до результата, в `open_files` не попадает.

`wc -l` для `lsp_manager.rs` после задачи — если выше 1500, Rust-ветки `notify_*` и poll job'ов выносятся в `src/lsp/lsp_manager_rust.rs` (`impl LspManager`, `include!`-стиль не использовать — обычный `mod` с `impl`), файл — в `PROJECT_GUIDE.md` §4 (Task 14).

Разделение тестов: `cargo_root_for_path`, `initialization_options`, `resolve_rust_root_blocking` (все варианты, RF-1) — Task 7 (`make test TEST_FILTER=rust_workspace`, ≤3 прогона, коммит `rust_workspace: cargo root, background root resolution`); pending-документы, один job на crate, закрытие до результата, RF-3, RF-5 — Task 16 в `lsp_tests.rs` с `RunCmd`-замыканием, которое блокируется на `std::sync::mpsc` канале до сигнала теста (так RF-5 проверяется детерминированно: открыть `.rs` → job висит → `set_rust_enabled(false)` → отпустить канал → poll → `rust.open_files()` пуст, процесса нет) — `make test TEST_FILTER=lsp_tests`, ≤3; коммиты Task 16: 1) Rust-ветки и job; 2) статус/health/enable/refresh.

Агенты Task 7: implementer `ds-low` · reviewer нет (чистые функции с unit-тестами; потребитель — Task 16, его ревью покроет контракт).

Агенты Task 16: implementer `ds-low-agentic` · reviewer `ds-high` — high: конкурентность pending-документов и generation (документ меняется/закрывается, пока job идёт; `refresh` во время job), ревьюер решает, нет ли пути, где документ остаётся без `didOpen` или открывается в остановленный процесс.

### Task 8: store — rust-слайс и политика версий

**Files:**
- Modify: `src/lsp/lsp_diagnostics_store.rs` — обработчик `LspEvent::Diagnostics` `:189-238` (параметризация по языку), `diagnostic_slices_for_abs_path` `:481-505` → `[&[Diagnostic]; 5]`, `diagnostic_arc_slices_for_abs_path` `:595-616` → 5, `diagnostic_paths` `:645-658`, `instant_merged_diagnostics_for_abs_path` `:736-779`, `clear_diagnostics_for_path` `:384-402`; `src/lsp/lsp_manager.rs` — очистки при выключении (`:344-352`, `:436-440` — rust аналогично dart).
- Test: `src/lsp/lsp_tests.rs` (там уже тесты store с прямой вставкой `manager.diagnostics.insert`, `:469-1385`) — новые тесты через публичный путь: сформировать `LspEvent::Diagnostics { server: RustAnalyzer, … }` и прогнать через тот же обработчик, которым пользуется poll (имя — по `rg -n 'LspEvent::Diagnostics' src/lsp/lsp_diagnostics_store.rs`).

**Interfaces (Consumes):** `self.rust.live_diagnostics: HashMap<PathBuf, LiveDiagnostics>`, `self.rust.open_files`, `RootedLanguage` (Task 3/7). **Produces:** порядок слайсов `[ruff, ty, dart, rust, legacy]` — на него опирается `instant_merged_*` и всё, что индексирует массив (`rg -n 'slices\[' src/lsp`).

Политика (спека §6, дословно в код):
* `route` события по `server`: `Ruff/Ty → Python`, `Dart → Rooted(Dart)`, `RustAnalyzer → Rooted(Rust)`; `is_open_file` = `rooted(lang).open_files.contains_key(&key)` для rooted, `open_python_files` для Python.
* Rooted: `current = rooted(lang).document_version(path)`; открытый путь: `version None` → принять, `stored = current.unwrap_or(0)`; `Some(v)` → принять, если `current.is_none_or(|c| v >= c)`, иначе отбросить; неоткрытый путь: `accepts_unopened(lang)` (`Rust → true`, `Dart → false`) → принять со `stored = 0`, иначе отбросить. Для Dart результат тот же, что сегодняшнее `version_is_current && (is_dart && version.is_none() || should_accept…)` — проверить существующими Dart-тестами store.
* `items.is_empty()` для rooted → `live_diagnostics.remove(path)` (а не запись пустого слайса); `root` записи — из параметра события: `poll_processes` слоя знает корень процесса → `LspEvent::Diagnostics` при раздаче в store сопровождается `root: PathKey` (сигнатура обработчика получает `Option<&PathKey>`; Python → `None`).
* Legacy-fallback: `self.diagnostics` только если все четыре пусты. Очистка при `set_rust_enabled(false)` — через `stop_root` (Task 3/7), здесь только `clear_diagnostics_for_path` и общий `clear`.

Тесты: открытый `.rs` + `version None` → хранится текущая версия; открытый + `v < current` → отброшено; неоткрытый `.rs` → принято (`stored == 0`); неоткрытый `.dart` → отброшено; пустой `items` → записи нет, `diagnostic_paths` её не содержит; `diagnostic_slices_for_abs_path` возвращает rust в индексе 3 и legacy пуст при непустом rust; RF-2: путь `/home/x/.cargo/registry/src/foo-1.0/lib.rs` с `root = PathKey(ws)` → после `stop_root(ws)` записи нет.

Прогоны: `make test TEST_FILTER=lsp_tests`, ≤3. Коммит: `diagnostics store: rust live slice, rooted version policy, root-scoped clear`.

Агенты: implementer `ds-low` · reviewer `ds-high` — high: политика версий задевает Dart и Ty-пути в одном обработчике; ревьюер решает, эквивалентна ли новая формула старой для Dart во всех четырёх комбинациях (`open × version`).

### Task 9: `RustSettings`, persist, Settings UI, строка rust-analyzer

**Files:**
- Create: `src/app/rust_settings.rs` (образец `src/app/dart_settings.rs:5`).
- Modify: `src/app/app_state.rs:1236` (поле `rust_settings: RustSettings` рядом с `dart_settings`), `src/state_persistence.rs:618, 726` (ключ `"rust"`), `src/ui_system/ui_ids.rs:67` (`SettingsRustToggleEnabled`, `SettingsRustToggleCheckCommand`, `SettingsRustRestart`), `src/render_view/settings_tool_rows.rs:834-863` (две новые пары `(UiId, подпись)` в том же списке, что Dart-переключатели: «Rust: вкл/выкл», «Проверка: cargo check / clippy»), `src/app/ui_handlers/ui_settings.rs:314` (три обработчика по образцу `SettingsDartToggleWorkspaceAnalysis`), `src/render_view/settings_tool_rows.rs:559-577` (`draw_settings_tool_row`: параметр `rust_row: Option<&crate::lsp::RustRowInfo>`; для `kind == RustAnalyzer` статус-текст `rust_status_text(info)`), `src/lsp/lsp_manager.rs` (`pub fn rust_row_info(&self) -> RustRowInfo`), `src/app/mod.rs` (`pub use rust_settings::*` как у Dart), вызовы `draw_settings_tool_row` (`rg -n 'draw_settings_tool_row\(' src`).
- Test: unit в `rust_settings.rs` (`from_config_value` дефолт `Clippy` на мусор), round-trip persist в inline `mod tests` файла `src/state_persistence.rs` (`:804`; найти тест Dart-ключа и повторить для `"rust"`, включая чтение конфига без ключа `rust` → дефолты), headless — Task 13.

**Interfaces:**

```rust
// rust_settings.rs
#[derive(Clone, Copy, Debug, PartialEq, Eq)] pub enum RustCheckCommand { Check, Clippy }
impl RustCheckCommand { pub const fn config_value(self) -> &'static str; pub fn from_config_value(v: &str) -> Self /* дефолт Clippy */; }
#[derive(Clone, Debug, PartialEq, Eq)] pub struct RustSettings { pub enabled: bool, pub check_command: RustCheckCommand }   // Default: true, Clippy
// lsp (rust_workspace.rs или lsp_manager.rs)
pub struct RustRowInfo { pub status: LspServerStatus, pub busy: bool, pub health_message: Option<String>, pub cargo_missing: bool, pub component_missing: bool, pub version: Option<String>, pub roots: usize }
```

Решения:
* Обработчики: `SettingsRustToggleEnabled` → `rust_settings.enabled = !…; lsp.set_rust_enabled(enabled)` + при включении `lsp.reopen_rust_documents(открытые .rs-вкладки)`; `save_current_config()`. `SettingsRustToggleCheckCommand` → переключить `Check ↔ Clippy`, `lsp.set_rust_init_options(rust_workspace::initialization_options(cmd.config_value()))`, `save_current_config()`. `SettingsRustRestart` → `lsp.refresh_rust_resolution()`.
* Persist: `"rust": {"enabled": bool, "check_command": "check"|"clippy"}`; чтение терпимо к отсутствию.
* `rust_status_text(info)`: `rust-analyzer · {источник}: {version|«не найден»} · LSP: {статус как dart_status_text} [· занят] [· {health_message}] [· cargo не найден: установите rustup] [· rustup component add rust-analyzer]`; источник — `ToolResolution.source` как у Dart (`state.source()`), для RA — из `resolve_tool_kind(RustAnalyzer)` (файловый, в рендере допустим — так работают все строки). Текст формируется как `dart_status_text` — в рендере, `format!` одной строкой: это локальный стиль всех строк инструментов (AGENTS.md допускает `format!` при локальном стиле), ничего тяжелее соседей. Параметр `rust_row` добавляется к функции с уже стоящим `#[allow(clippy::too_many_arguments)]` (`:559-576`); новых `#[allow]` не добавлять.
* При старте приложения `lsp.set_rust_enabled(rust_settings.enabled)` и `set_rust_init_options(...)` вызываются там же, где сегодня применяются `dart_settings` к `LspManager` (`rg -n 'set_dart_workspace_analysis_enabled\(' src/app` — место инициализации).

Прогоны: `make test TEST_FILTER=rust_settings`, `make test TEST_FILTER=state_persistence`, ≤3. UI-снимок здесь не делается (prebuilt-бинарь ещё без этого кода); проверка строки и переключателей — headless в Task 13. Коммит: `Rust settings: enabled/check command, Settings toggles, rust-analyzer row`.

Агенты: implementer `ds-low-agentic` · reviewer нет.

### Task 10: константы релиза rust-analyzer (ds-web)

**Files:**
- Create: `docs/superpowers/plans/2026-10-03-rust-support-release.md` — таблица: тег, для каждой тройки (`x86_64-unknown-linux-gnu`, `aarch64-unknown-linux-gnu`, `x86_64-apple-darwin`, `aarch64-apple-darwin`, `x86_64-pc-windows-msvc`) имя asset'а и sha256 со страницы релиза `https://github.com/rust-lang/rust-analyzer/releases/tag/<tag>` (GitHub показывает sha256 у каждого asset'а; отдельного `.sha256` файла нет — факт 03.10.2026), плюс URL шаблон `https://github.com/rust-lang/rust-analyzer/releases/download/<tag>/<asset>`.

Решение: тег — последний релиз на момент выполнения задачи (на 03.10.2026 — `2026-09-28`); Windows-asset — `.zip`; записать также размер каждого asset'а (для проверки загрузки). Проверить, что sha256 скопированы целиком (64 hex), и что имена совпадают по шаблону `rust-analyzer-<triple>.gz`/`.zip`. Отчёт — путь файла. Внешних действий кроме чтения страниц нет.

Агенты: implementer `ds-web` · reviewer нет.

### Task 11: managed-установка rust-analyzer

**Files:**
- Modify: `src/lsp/rust_workspace.rs` (константы `RUST_ANALYZER_RELEASE_TAG: &str`, `RUST_ANALYZER_ARCHIVES: &[RustAnalyzerArchive { triple, asset, sha256 }]`, `fn rust_analyzer_archive_for_platform() -> Option<&'static RustAnalyzerArchive>` по `std::env::consts::{ARCH, OS}`), `src/app/tool_installer.rs:620-672` (`ToolInstaller::start(RustAnalyzer)` → новая ветка `start_rust_analyzer_install`, по образцу `start_pdfium_install` `:657`), `src/app/tool_installer_download.rs` (`install_rust_analyzer_from_gz(archive, expected_sha256, destination, cancel, on_phase)`: `sha256_file_hex` `:874` → `Verifying`; `flate2::read::GzDecoder` одного файла (не tar — asset для Linux/macOS это gzip одного бинаря) → `Extracting`; запись во временный файл рядом + `rename` (atomic sibling-temp, AGENTS.md); `PermissionsExt::set_mode(0o755)` на Unix; destination `data_dir()/tools/managed/rust-analyzer/<tag>/rust-analyzer`), фазы `Downloading → Verifying → Extracting → Succeeded/Failed/Cancelled` через `ToolInstallPhase` `:263`; загрузка — тем же async-клиентом, что `download_pdfium_archive_async` `:730` (переиспользовать, переименовав в `download_archive_async`, если внутри нет pdfium-специфики; иначе общий helper выделить).
- Modify: завершение установки → `refresh_rust_resolution()` в `LspManager` (где pdfium/uv сообщают приложению об успехе — `rg -n 'Succeeded' src/app/tool_installer*.rs src/app/events` — тот же хук).
- Test: `src/app/tool_installer_tests.rs` — `install_rust_analyzer_from_gz` на temp-файле: сгенерировать gzip из байтов `b"#!/bin/sh\necho fake\n"`, посчитать sha256 в тесте, успех → файл исполняемый и содержимое совпадает; неверный sha256 → `Err`, файла нет; отмена через `cancel` до записи → `Cancelled`, файла нет; `rust_analyzer_archive_for_platform()` → `#[cfg(not(windows))]` `Some` с 64-hex sha256, `#[cfg(windows)]` `None`. Хук успеха: `poll_tool_installer()` (`tool_installer.rs:1039-1080`) на `Installed` записывает путь инструмента и перезапускает только Ruff/Ty — добавить ветку `RustAnalyzer → lsp.refresh_rust_resolution()` там же.

**Interfaces (Consumes):** таблица Task 10 (значения переносятся в константы вручную, implementer сверяет 64 hex); `ToolKind::RustAnalyzer`, `ManagedToolInstallPlan::RustAnalyzerArchive` (Task 6).

Решение по Windows: zip-crate в зависимостях нет и в этой задаче не добавляется — на Windows `rust_analyzer_archive_for_platform()` возвращает `None`, кнопка не рисуется (`supports_managed_install` остаётся `true` по типу, но `draw_settings_tool_row` для `RustAnalyzer` проверяет `archive_for_platform().is_some()`), подсказка в строке «установите через rustup: rustup component add rust-analyzer». Решение пользователя (03.10.2026): zip-crate не добавлять, на Windows — через rustup; отдельная карточка Kanri «Windows: managed-установка rust-analyzer (.zip)» заводится в Task 14 (Backlog).

Прогоны: `make test TEST_FILTER=tool_installer`, ≤3. Без сети в тестах. Коммит: `tool installer: managed rust-analyzer from GitHub release (gz, sha256)`.

Агенты: implementer `ds-low-agentic` · reviewer `ds-high` — high: загрузка и запись исполняемого файла — security-путь (sha256 до записи, atomic replace, права), ревьюер решает, нет ли окна с частично записанным исполняемым файлом в managed-каталоге.

### Task 12: hover — подсветка fence-блоков по тегу

**Files:**
- Modify: `src/lsp/hover.rs:3-30` (`highlight_hover_text`: перед `looks_like_dart_hover` — `if let Some(result) = highlight_tagged_fences(&preprocessed) { return result; }`), `:95-115` (общий путь: тегированные блоки → дерево своего языка, остальное как сегодня через `push_python_ts_spans`); новый helper `fn fence_language(tag: &str) -> Option<&'static str>` (`rust → "rs"`, `python|py → "py"`, `dart → "dart"`, иначе `None`) и `fn push_ts_spans_for(lang_ext, text, …)` — обобщение `push_python_ts_spans` через `get_ts_config(ext)` (`src/languages/python.rs:10` показывает вызов для `"py"`; Rust-конфиг есть: `queries.rs:379`, `languages/rust.rs`).
- Test: тестовый модуль hover — `src/lsp/python_hover_tests.rs`, подключён через `#[path]` в `hover.rs:1245`; новые тесты туда: markdown с блоком ```` ```rust ```` `fn main() {}` и строкой `:param x:` в docs → спаны блока построены Rust-парсером (проверить наличие спана keyword на `fn`), а не Python; текст без тегов и с `:param ` → прежний Python-путь (существующие тесты не меняются); ```` ```dart ```` блок → Dart.

Решение: тег сильнее эвристики только для тегированных блоков; `highlight_tagged_fences` возвращает `Some` только если найден хотя бы один блок с известным тегом — тогда весь текст идёт общим путём `normalize_hover_text` с подсветкой по тегам, эвристики не вызываются. Режим hover-рендера (`HoverLineKindPublic`, inline-code ranges) — без изменений.

Прогоны: `make test TEST_FILTER=hover`, ≤3. Коммит: `hover: fenced code blocks highlighted by language tag`.

Агенты: implementer `ds-low` · reviewer нет.

### Task 13: headless-тесты Rust

**Files:**
- Create: `src/headless/ui_tests_rust_lsp.rs` (регистрация `mod` в `src/headless/tests.rs`).
- Modify: только `mod` строка; fixture-helper `rust_crate_session(name) -> (dir, file, session)` внутри нового файла: каталог `rriter-headless-<name>-<pid>` с `Cargo.toml` (`[package] name = "fixture" version = "0.1.0" edition = "2021"`) и `src/main.rs`; fake-сервер через `install_fake_lsp(session, ToolKind::RustAnalyzer, basename)` (Task 1) — override путь к скрипту, значит ветка `rustup which` не используется (путь не прокси); `cargo` в тестовой среде может быть на PATH — `locate-project` на fixture отработает (один crate → тот же каталог) или fallback даст тот же ответ, тест от этого не зависит.

Сценарии (каждый — отдельный `#[test]`, ожидание — `tests_support::wait_until`, никаких фиксированных `wait` кроме негативных проверок):
1. `headless_rust_starts_on_open_and_stops_on_last_close`: открыть `src/main.rs` → `lsp.rust_row_info().status == Running` (через `session.app.lsp`); закрыть вкладку → `Disabled`.
2. `headless_rust_diagnostics_reach_problems_including_unopened_file`: fake `rust_analyzer_diagnostics_unopened`; открыть `main.rs` → в Problems строки для `main.rs` и для `sibling.rs` (создать файл в fixture); закрыть вкладку → обе исчезают.
3. `headless_rust_outside_cargo_root_has_no_lsp`: `.rs` в каталоге без `Cargo.toml` → статус остаётся `Disabled`, `rust.open_files` пуст, fake-сервер не стартовал (`.starts` файла нет).
4. `headless_rust_two_roots_independent`: два fixture-crate'а; открыть по файлу → `rust_row_info().roots == 2`; закрыть первый → `roots == 1`, статус `Running`, в Problems только строки второго.
5. `headless_rust_settings_toggle_and_check_command_restart`: fake `rust_analyzer_initlog`; `click_ui("SettingsRustToggleEnabled")` → `Disabled`, Problems пусты (при `_diagnostics`); снова → `Running`; `click_ui("SettingsRustToggleCheckCommand")` → fake-сервер перезапущен (`.starts == 2`) и последняя строка `.init.jsonl` содержит `"command":"check"`.
6. `headless_rust_settings_row_shows_missing_tool`: override на несуществующий путь (`ToolPaths` для `RustAnalyzer` = `<dir>/missing-rust-analyzer`) → после открытия `.rs` строка Settings содержит «не найден» (текст — из `dump` Settings-страницы; как её открыть — по `ui_tests_settings_tools.rs`), статус `Missing`.
7. `headless_rust_server_status_busy_flag`: fake `rust_analyzer_serverstatus` → `rust_row_info().busy` сначала `true`, затем `false` (`wait_until` на оба).
8. (снят: RF-5 без управляемой задержки job в headless не воспроизводится; покрыт unit-тестом Task 16 с блокирующим `RunCmd`.)

Сценарий 5 использует basename `rust_analyzer_diagnostics_initlog` (оба режима сразу — Task 1 допускает сочетание суффиксов).

Разделение между агентами: сценарии 1–4 одному implementer'у, 5–7 другому, оба пишут в один файл? — нет: два файла `ui_tests_rust_lsp.rs` (1–4) и `ui_tests_rust_settings.rs` (5–8), fixture-helper — в первом, второй импортирует (`pub(super) fn`); второй агент стартует после первого (helper нужен), либо helper пишет первый коммитом раньше остального. Прогоны: по 3 на агента, `make test TEST_FILTER=ui_tests_rust`. Пробы на prebuilt-бинаре через `rriter_headless.py` допустимы только после `make fast` главной сессией перед этапом (бинарь после Task 12 должен содержать Rust-код) — SDD-сессия делает один `make fast` перед Task 13.

Коммиты: `headless: Rust LSP lifecycle and diagnostics`, `headless: Rust settings, status row, serverStatus`.

Агенты: implementer `ds-low-agentic` ×2 (по файлам, последовательно или второй после helper-коммита) · reviewer нет.

### Task 14: документация и индекс

**Files:**
- Modify: `PROJECT_GUIDE.md` §4 (`:1168` — формат `* \`path\` -> описание`): строки для `src/lsp/rooted_language.rs`, `src/lsp/rust_workspace.rs`, `src/app/rust_settings.rs`, `src/headless/ui_tests_dart_lsp.rs`, `src/headless/ui_tests_rust_lsp.rs`, `src/headless/ui_tests_rust_settings.rs`, (`src/lsp/lsp_manager_rust.rs`, если появился в Task 7); обновить описание `src/lsp/dart_workspace.rs` (жизненный цикл ушёл в `rooted_language.rs`). `docs/headless.md` — абзац про режимы fake-сервера из Task 1 (где описаны существующие суффиксы — `rg -n '_diagnostics' docs`). `docs/feature-backlog.md` — строка «Rust: rust-analyzer per Cargo root, managed install (Linux/macOS)» в списке принятых фич, если файл ведёт такой список.
- Run: `python3 gen_project_ai_map.py` (разрешено AGENTS.md §3) и закоммитить `PROJECT_AI_MAP.txt`, если он отслеживается.

Коммит: `docs: Rust support in project guide, headless fake-server modes`.

Агенты: implementer `ds-low` · reviewer нет.

---

## Таблица общих интерфейсов

| Интерфейс | Определяет | Использует |
|---|---|---|
| `install_fake_lsp(session, ToolKind, basename)`; суффиксы `_serverstatus`, `_unopened`, `_initlog` | Task 1 | Task 2, 13 |
| `RootedWorkspaces` и его методы (приватные поля + геттеры), `LiveDiagnostics`, `ServerHealth` | Task 3 | Task 15, 4, 7, 8 |
| `LspManager::{stop_rooted_root, rooted_root_may_stop, dart_status()}`, поле `dart`, параметр `root` обработчика `Diagnostics` | Task 15 | Task 4, 7, 8 |
| `start_with_executable(def, workspaces, executable, init_options, ui_waker)`, `make_initialize_for_server(server, id, workspaces, init_options)` | Task 3 | Task 5, 7 |
| `LangRoute`, `language_for_ext`, `rooted_mut`, `request_ide_*` | Task 4 | Task 7, callers в `src/app/autocomplete` |
| `LspServerKind::RustAnalyzer`, `RUST_ANALYZER_SERVER`, `LspEvent::ServerStatus`, `configuration_response_for(server, item, init_options)` | Task 5 | Task 7, 8 |
| `ToolKind::RustAnalyzer`, `ManagedToolInstallPlan::RustAnalyzerArchive`, `managed_rust_analyzer_executable_in`, `is_usable_executable` | Task 6 | Task 7, 9, 11 |
| `rust_workspace::{cargo_root_for_path, initialization_options(&str), RustRootResolution, RustToolError, RustTools}`, `LspManager::{set_rust_enabled, set_rust_init_options, refresh_rust_resolution, reopen_rust_documents, rust_row_info}` | Task 7 (`rust_row_info` — Task 9) | Task 8, 9, 11, 13 |
| Порядок слайсов `[ruff, ty, dart, rust, legacy]`, обработчик `Diagnostics` с `root: Option<&PathKey>` | Task 8 | Task 3 (poll слоя передаёт root), 13 |
| `RustSettings`, `RustCheckCommand::config_value`, `UiId::SettingsRust*`, `RustRowInfo` | Task 9 | Task 13 |
| `RUST_ANALYZER_RELEASE_TAG`, `RUST_ANALYZER_ARCHIVES`, `rust_analyzer_archive_for_platform` | Task 11 (значения — Task 10) | Task 6 (managed-каталог по тегу), Task 9 (кнопка) |

## Порядок и параллельность

* Task 1 → Task 2 → Task 3 → Task 15 → Task 4 последовательно (каждая строит на предыдущей; Task 2 — страховка Task 15; Task 15 вынесен из Task 3 по размеру и идёт сразу за ним).
* Кнопка «Установить» в строке rust-analyzer между Task 6 и Task 11 возвращает ошибку-заглушку — допустимо только потому, что ветка сливается в `master` целиком после Task 14; отдельно Task 6 не мержится.
* После Task 4: Task 5, Task 6, Task 10, Task 12 параллельны и независимы (разные файлы: protocol / platform+installer-заглушка / docs / hover).
* Task 7 после Task 5 и Task 6 (параллельно Task 10, 12); Task 16 после Task 7; Task 8 и Task 9 после Task 16 (поля `rust`), между собой параллельны (store vs settings/UI — файлы не пересекаются, кроме `lsp_manager.rs`: Task 8 правит только очистки `:344-352, :436-440`, Task 9 добавляет `rust_row_info` — в разных местах файла; при конфликте слияния решает главная сессия). Task 11 после Task 6, Task 10 и коммита Task 7 (константы ложатся в `rust_workspace.rs`, который правит Task 7; отдельный файл констант был бы меньше 200 строк — запрещено AGENTS.md).
* Task 13 после Task 9, 11, 12 и одного `make fast`; Task 14 последним.
* Сборки/тесты — по одному агенту за раз в основном чекауте; параллельные задачи (5/6/10/12) — в worktree с reflink `target/` (AGENTS.md §4), собирать разрешено каждому, слияние — главная сессия.

## Review Focus — привязка

RF-1 → Task 7 (unit таймаут `rustup which`); RF-2 → Task 8 (registry-путь, root-scoped clear); RF-3 → Task 7 (два crate_dir, один корень); RF-4 → Task 2 (переоткрытие во время job, `starts == 1`); RF-5 → Task 7 (generation при `set_rust_enabled(false)`) и Task 13 сценарий 8.

## Финальное ревью ветки

`ds-high` и `final-reviewer` параллельно, `master..rust-support`; строки риска — из `Агенты:` Task 3, 7, 8, 11 плюс Global Constraints (проверки `rg` из них — ревьюер выполняет буквально). Затем один `make codex_test` у главной сессии, PR, squash-merge.
