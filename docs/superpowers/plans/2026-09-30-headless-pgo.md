# Headless PGO Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `make max` строит PGO-бинарь по профилю, снятому на Linux только headless-прогонами сценария, который покрывает всю функциональность, и падает, если какая-то группа сценария перестала выполняться.

**Architecture:** `AutomationController` уже тикается из `about_to_wait`, который вызывает и headless `step_frame`; добавляем модель сценария (`full`/`startup`/`welcome`/`smoke`/`group:<name>`), headless-вход `--pgo-train` с циклом 60 кадров/с, общие шаги сырого ввода и ожидания по условию, группы шагов с автопропуском при отсутствии внешнего инструмента, семь новых групп. `scripts/pgo_pipeline.py` на Linux запускает три headless-прогона (Windows/macOS-упаковка остаётся на GUI-прогоне), мержит без `-sparse`, проверяет маркеры групп и копирует бинарь туда, куда его клал `make max`.

**Tech Stack:** Rust nightly (build-std, `-Cprofile-generate/-use`), EGL pbuffer headless (только Linux), Python 3 (`unittest`), `llvm-profdata` (rustup), `llvm-cxxfilt` (системный), GNU make.

**Spec:** `docs/superpowers/specs/2026-09-30-headless-pgo-design.md`. Отступление от спеки, принятое при pre-flight: headless существует только на Linux (`src/headless/mod.rs:3`), а `scripts/build_windows.py:668` и `scripts/build_macos.py:221` снимают PGO через тот же `run_pipeline` GUI-прогоном — GUI-путь пайплайна остаётся для не-Linux, удаляется только Wayland-ветка Linux.

## Global Constraints

- Язык кода, комментариев, коммитов — как в соседнем коде (английский).
- Исходники не править через `sed -i`/перенаправления; новые файлы-логи — можно. `rustfmt`/`cargo fmt` не запускать вообще — стиль соседних строк руками.
- Жёсткий лимит 1600 строк на файл. `src/headless/tests.rs` (1510) и `src/app/automation_database.rs` (1490) не растут. `scripts/pgo_pipeline.py` (1697) после Task 12 ≤ 1600.
- Нет `.unwrap()`/`.expect()` в production-путях; тестам можно.
- Нет нового глобального изменяемого состояния (`static` + `Mutex`/`OnceLock`/atomic с данными приложения): состояние шагов — в `App`/`AutomationController` или читается из состояния фичи.
- Новые файлы — только `.rs`, `.py`, `.md`, `.txt`. Новый `.rs` < 200 строк допустим только для файла группы `src/app/automation_<group>.rs` (решение пользователя: группа = свой файл); прочий новый код — в существующие файлы.
- `PGO_AUTOMATION_SCENARIO_VERSION` = 18 синхронно в `src/app/automation.rs:14` и `scripts/pgo_pipeline.py:38` (меняет Task 12, одним коммитом с пайплайном; `self_test` пайплайна сверяет их, `:1562`).
- Шаг сценария, завершающий прогон, называется `AutomationStep::Finish` (`automation.rs:216`).
- Headless-тесты ждут асинхронный результат условием, не фиксированным `wait`; всё, что тест пишет вне процесса, уникально на процесс (`std::process::id()`, порт 0).
- Фокусные тесты: `make test TEST_FILTER=<module path>` (например `headless::ui_tests_pgo`); `make test` сам выставляет `RRITER_PDFIUM_PATH` из `target/pdfium.path` (`Makefile:95`). Длинный вывод — в `/tmp/rriter-<task>.log`, смотреть `tail -n 40`/`grep`. Не больше 4 прогонов `make test` на задачу, потом отчёт с падениями. Тесты только в foreground. `make codex_test`, `make fast`, `make max` в задачах не запускать.
- Коммит(ы) в задаче явными путями, на ветке `headless-pgo`. Внешнее нельзя: push, удалённые ветки, PR, комментарии, CI.
- Шаги сценария не выходят в сеть и не трогают пользовательский HOME.
- Новые группы самодостаточны: первый шаг группы (`Call`) пишет нужные ей файлы в `<workspace>/pgo_<group>/` и при необходимости коммитит их в fixture-репозиторий; группа не рассчитывает на Python-фикстуры пайплайна.
- Каждая групповая задача добавляет свою строку в `FULL_GROUPS` последним коммитом, после зелёного `group:<name>`.

## Общие интерфейсы

| Имя | Где | Создаёт | Использует |
|---|---|---|---|
| `enum PgoScenario { Full, Startup, Welcome, Smoke, Group(String) }`; `PgoScenario::parse(&str) -> Result<Self, String>` (`full`, `startup`, `welcome`, `smoke`, `group:<name>`); `as_str(&self) -> String` | `src/app/automation.rs` | Task 1 | все |
| `AutomationOptions { workspace, report_path, timeout, scenario: PgoScenario }` (GUI-запуск в `main.rs` ставит `Full`) | `src/app/automation.rs:106` | Task 1 | Task 2, 4 |
| `AutomationController::outcome(&self) -> Option<Result<(), String>>` (`None` — не закончен, `Err(failed_step)`) | `src/app/automation.rs` | Task 1 | Task 2 |
| Отчёт прогона: сохраняет `status`, `scenario_version`, `failed_step` (`automation.rs:648-660`), добавляет `"scenario"`, `"frames"` (кадры считает контроллер по тикам), `"skipped_groups": [{"group", "reason"}]` | `src/app/automation.rs` | Task 1 (`scenario`, `frames`), Task 3 (`skipped_groups`) | Task 12 |
| `HeadlessOptions.automation: Option<AutomationOptions>` → `AppInitOptions::headless(automation)` | `src/headless/profile.rs:19`, `src/app/app_bootstrap.rs:84` | Task 2 | Task 2 |
| `HeadlessSession::run_automation(&mut self, pace: Option<Duration>) -> PgoRunOutcome`; `struct PgoRunOutcome { success: bool, frames: u64, failed_step: Option<String> }` | `src/headless/frame.rs` | Task 2 | Task 2–11 |
| CLI: `rriter --headless --pgo-train --pgo-scenario S --pgo-workspace W --pgo-report R --pgo-timeout-seconds T --profile P`; exit 0 успех, 1 шаг/таймаут, 2 ошибка запуска (EGL, аргументы, workspace). Флаг `--profile` здесь — каталог профиля rriter (не путь `.profdata` пайплайна) | `src/headless/profile.rs`, `src/main.rs` | Task 2 | Task 12, 13 |
| `run_pgo_scenario(scenario: &str, timeout_ms: u64) -> PgoRunOutcome` — `ensure_test_profile_root()` + `reset_api_test_state()`, per-PID workspace с git через `crate::app::automation::ensure_fixture_repository` (`pub(crate)`), сессия с automation, `run_automation(None)` | `src/headless/ui_tests_pgo.rs` (не `tests.rs`) | Task 2 | Task 3–11 |
| `AutomationStep::WaitUntil { what: &'static str, check: fn(&App) -> bool, timeout_ms: u64 }` | `src/app/automation.rs` | Task 3 | Task 4–11 |
| `AutomationStep::Call { what: &'static str, run: fn(&mut App, &Path) -> Result<(), String> }` (`&Path` — workspace; `Call` не блокирует кадр дольше ~50 мс — сеть/процессы через `std::thread::spawn`, результат ждать `WaitUntil` по состоянию фичи) | там же | Task 3 | Task 4–11 |
| `AutomationStep::Key(&'static str)` — спецификация `KeyInput::parse_combo` (`crate::app::keyboard`, её же использует команда `key` драйвера) | там же | Task 3 | Task 4–11 |
| `AutomationStep::Wheel { at: AutomationTarget, dx: f32, dy: f32 }`, `Click { at: AutomationTarget, button: AutomationButton, mods: &'static str, clicks: u8 }`, `Drag { from: AutomationTarget, to: AutomationTarget, steps: u16 }` (все мышиные шаги сначала `handle_main_cursor_moved`) | там же | Task 3 | Task 4–11 |
| `enum AutomationTarget { Ui(UiId), Point(f32, f32), Find(fn(&App) -> Option<(f32, f32)>) }` (`Ui` — центр прямоугольника из `app.ui_registry.element_hits()`); `enum AutomationButton { Left, Right, Middle }` | там же | Task 3 | Task 4–11 |
| `AutomationStep::GroupStart { name: &'static str, requires: Option<fn(&App) -> Result<(), String>> }`, `GroupEnd`; `requires` → `Err(reason)` — контроллер пропускает шаги до `GroupEnd`, пишет `skipped_groups`, прогон не проваливается | там же | Task 3 | Task 9, 11, 12 |
| `src/app/automation_groups.rs`: `pub(super) const FULL_GROUPS: &[&str]`, `pub(super) fn group_steps(name: &str, workspace: &Path) -> Option<Vec<AutomationStep>>` (оборачивает шаги группы в `GroupStart`/`GroupEnd`); шаги `startup`/`welcome` — здесь же | новый | Task 3 (реестр), Task 4 (startup/welcome) | Task 5–11 |
| Группа: `src/app/automation_<group>.rs` с `pub(super) fn steps(workspace: &Path) -> Vec<AutomationStep>` и `pub(super) fn requires(app: &App) -> Result<(), String>` при внешнем инструменте | новые | Task 5–11 | реестр |
| Имена групп: `pdf`, `api_mock`, `git_changes`, `editor_ops`, `lsp_nav`, `input_scroll`, `terminal_ops` | реестр | Task 5–11 | Task 12 |
| `PgoScenario::saves_session_on_exit()` (`Full`), `restores_session()` (`Startup`) | `src/app/automation.rs` | Task 4 | Task 4 |
| `scripts/pgo_coverage.py`: `parse_profdata_show(text)`, `module_of(demangled) -> str`, `module_summary(...)`, `check_markers(functions, markers, skipped) -> list[str]`, `count_pgo_warnings(build_log) -> PgoWarnings`, `GROUP_MARKERS: dict[str, list[str]]` | новый | Task 12 | Task 12 |
| Пайплайн: `--install-binary PATH`, `--scenarios full,startup,welcome` (дефолт на Linux; на не-Linux игнорируется, GUI-прогон `full`); `start_fixtures()`, `run_scenario(name, report_path)`; отчёт `automation-report-<scenario>.json` | `scripts/pgo_pipeline.py` | Task 12 | Task 13 |

## Review Focus

1. Headless не поднял EGL / неверный workspace под instrumented-бинарём — выход с кодом 2 и понятной строкой в stderr, без зависания до таймаута пайплайна. Тест: Task 2.
2. Шаг сценария никогда не выполняет условие — код 1 и имя шага в отчёте не позже `timeout_ms`; `.profraw` всё равно пишется. Тест: Task 3.
3. `startup` без сохранённой сессии (или с непарсящимся файлом) — старт без вкладок без паники, прогон падает на шаге «restored tabs». Тест: Task 4 (unit на парсер + прогон без preload).
4. `target/pdfium.path` отсутствует или указывает на несуществующий файл; нет `llvm-cxxfilt` — пайплайн падает до сборок с подсказкой. Тест: Task 12.
5. Вывод `llvm-profdata show` с недеманглируемыми именами, замыканиями, generic- и trait-impl — `module_of` не падает, относит к модулю типа или `<other>`. Тест: Task 12.

---

### Task 1: Модель сценария и результат контроллера

**Агенты:** implementer ds-low-agentic · reviewer нет (контракт сразу потребляет Task 2 с ревьюером; поля отчёта проверяют тесты Task 12).

**Files:** Modify `src/app/automation.rs` (`PgoScenario`, поле `scenario`, `outcome()`, поля отчёта `scenario`/`frames`, счётчик тиков), `src/app/automation_fixtures.rs` (выбор шагов: `Full` → `full_pgo_scenario`, `Smoke` → `WaitReady`, `ResizeWindow { 1600, 900 }`, `WaitFrames(3)`, `Finish`; `Startup`/`Welcome`/`Group` пока → ошибка сценария «not implemented» с именем), `src/main.rs` (GUI-`AutomationOptions` с `scenario: PgoScenario::Full`), `src/app/automation_tests.rs`.

- [ ] Step 1: unit-тесты в `automation_tests.rs`: `parse` всех форм, `group:` без имени → `Err`, мусор → `Err` с текстом входа, `""` → `Err`; `as_str` обратим к `parse`; `outcome()` = `None` до конца, `Some(Ok)` после `Finish`, `Some(Err(name))` после провала шага; отчёт содержит `status`, `scenario_version`, `failed_step`, `scenario`, `frames`.
- [ ] Step 2: `make test TEST_FILTER=app::automation > /tmp/rriter-t1.log 2>&1; tail -n 40 /tmp/rriter-t1.log` → FAIL.
- [ ] Step 3: реализация.
- [ ] Step 4: прогон → PASS.
- [ ] Step 5: Commit `PGO automation: scenario model, controller outcome, report fields`.

### Task 2: Headless-раннер `--pgo-train`

**Агенты:** implementer ds-low-agentic · reviewer ds-high — high: контракт кодов выхода потребляет пайплайн; общий resize pbuffer меняет поведение всех headless-тестов; тест-хелпер используют девять задач.

**Files:** Modify `src/headless/profile.rs` (флаги, `HeadlessOptions.automation`), `src/app/app_bootstrap.rs:84` (`AppInitOptions::headless(automation)`; все вызывающие обновить), `src/headless/mod.rs` (сессия с automation; вход в IDE-режим как `workspace` `:494-506`, кроме `Welcome`; CLI-вход, возвращающий код; команда `resize` через общий путь), `src/headless/frame.rs` (`run_automation`, `PgoRunOutcome`, сверка размера окна с `gl.size()`), `src/main.rs:137-139,267-270` (telemetry-флаг `--pgo-train` выставляется и для headless-ветки). Create `src/headless/ui_tests_pgo.rs` (+`mod` в `src/headless/mod.rs`), в нём `run_pgo_scenario`.

**Решения:**
- `--pgo-train` подразумевает `--allow-writes`; дефолт `--pgo-scenario full`; окно 2560×1440, scale 1.333, если размер не задан флагами профиля.
- `run_automation(pace)`: цикл `step_frame(force=false)`; при `Some(pace)` сон до дедлайна `start + n*pace`, опоздавший кадр — следующий сразу, без пачки; стоп по `loop_state.exit_requested` или общему таймауту. Результат — `controller.outcome()`. Shutdown сервисов уже делает `about_to_wait` (`about.rs:166-169`), повторно не вызывать.
- CLI: `pace = Some(Duration::from_micros(16_667))`; код 0/1/2; выход через существующий `std::process::exit(headless::run(..))`.
- Resize: после тика automation в `step_frame`, если `app.window.inner_size() != gl.size()`, — тот же путь, что команда `resize` (`mod.rs:524-535`); команда `resize` использует его же.

- [ ] Step 1: тесты в `ui_tests_pgo.rs`: (a) `run_pgo_scenario("smoke", 20_000)` → `success`, `frames > 3`, GL-размер 1600×900; (b) `smoke` с таймаутом 0 → `success == false`, `failed_step` не пуст; (c) разбор CLI: `--pgo-train --pgo-scenario group:x` → `Group("x")`, `--pgo-scenario nope` → ошибка с `nope`; (d) Review Focus 1: CLI-вход с несуществующим `--pgo-workspace` → код 2 (функция, возвращающая код, без `process::exit`).
- [ ] Step 2: `make test TEST_FILTER=headless::ui_tests_pgo > /tmp/rriter-t2.log 2>&1` → FAIL.
- [ ] Step 3: реализация.
- [ ] Step 4: прогон → PASS; `make test TEST_FILTER=headless::ui_tests_layout` → PASS (resize общий).
- [ ] Step 5: Commit `Headless PGO runner: --pgo-train with paced loop, exit codes, pbuffer resize`.

### Task 3: Шаги ожидания, вызова, ввода и групп; реестр

**Агенты:** implementer ds-low-agentic · reviewer ds-high — high: семантику шагов, пропуска групп и маршрутизацию ввода используют семь групповых задач и пайплайн.

**Files:** Create `src/app/automation_groups.rs` (реестр; всегда отдельным файлом), Modify `src/app/automation.rs` (варианты, `AutomationTarget`, `AutomationButton`, ветки `name()`/`timeout()` `:220`,`:317` — `name = what`/`name`, `timeout = timeout_ms`; `skipped_groups` в отчёте), `src/app/automation_controller_steps.rs` (одна ветка, делегирующая исполнение новых шагов — в `automation_groups.rs` или `automation_semantic_actions.rs`, что меньше), `src/app/automation_fixtures.rs` (`Full` дописывает `group_steps` каждой из `FULL_GROUPS` перед `Finish`; `Group(name)` → `WaitReady` + `group_steps(name)` + `Finish`, неизвестное имя → провал с именем), `src/app.rs` (`mod`), `src/headless/ui_tests_pgo.rs`.

**Решения:**
- `Key` → `KeyInput::parse_combo` → тот же путь нажатия, что команда `key` драйвера (`headless/mod.rs:392-401`: press → шаг → release с подменой `modifiers`). Мышь: `handle_main_cursor_moved`, затем `handle_main_mouse_input`; wheel — обработчик, который зовёт команда `wheel`. Если последовательность драйвера и шага совпадает — вынести её в нейтральную функцию, которую зовут оба; копию не заводить.
- `WaitUntil` проверяет `check` раз в кадр; истёк `timeout_ms` → провал шага `what`. `Call` — один раз, `Err(e)` → провал `what: e`.
- `GroupStart` с `requires`, вернувшим `Err(reason)`, — переход за соответствующий `GroupEnd`, запись `{group, reason}` в `skipped_groups`, прогон продолжается.
- Тестовые группы `test_never`, `test_input`, `test_skip` регистрируются только под `cfg(test)`.

- [ ] Step 1: тесты в `ui_tests_pgo.rs` через `run_pgo_scenario`: (a) Review Focus 2: `group:test_never` (`WaitUntil { check: |_| false, timeout_ms: 200 }`) → `success == false`, `failed_step` содержит `what`, < 5 с; (b) `group:test_input`: `Call` пишет `pgo_test_input/a.py` (50 строк) и открывает, `Key("ctrl+a")`, `TypeText("x")`, `Key("ctrl+z")`, `Click` по тексту с `clicks: 2`, `Wheel` вниз, `WaitUntil` на изменение скролла → `success`; (c) `group:test_skip` (`requires` → `Err("no tool")`) → `success`, в отчёте `skipped_groups` с `no tool`; (d) `group:does_not_exist` → провал с именем.
- [ ] Step 2: `make test TEST_FILTER=headless::ui_tests_pgo > /tmp/rriter-t3.log 2>&1` → FAIL.
- [ ] Step 3: реализация.
- [ ] Step 4: прогон → PASS; `make test TEST_FILTER=headless::ui_tests_editor` → PASS (общий путь ввода драйвера).
- [ ] Step 5: Commit `PGO automation: WaitUntil/Call/input/group steps, group registry`.

### Task 4: Сценарии `startup` и `welcome`, исключение по сессии

**Агенты:** implementer ds-low-agentic · reviewer ds-high — high: исключение из запрета сохранения/загрузки сессии под automation не должно задеть пользовательский старт, GUI-тренировку на Windows/macOS и прочие сценарии.

**Files:** Modify `src/app/automation.rs` (`saves_session_on_exit`, `restores_session`), `src/app/app_ide_tab_methods.rs:441` (гард `is_automation_mode` в `save_tabs_state`), `src/app/app_ide_startup_methods.rs:89` и `:280` (гарды загрузки и `preload_ide_startup`), `src/app/events/about.rs:166-169` (перед `shutdown_background_services`: при `saves_session_on_exit()` — `app.save_tabs_state()`; одна точка для GUI и headless), `src/app/automation_groups.rs` (шаги `startup`, `welcome`), `src/app/automation_fixtures.rs` (ветки `Startup`, `Welcome`), `src/headless/ui_tests_pgo.rs`. `window_runtime.rs:354` не трогать.

**Решения:**
- Гарды ослабляются только для automation со сценарием `Full` (сохранение) и `Startup` (загрузка); без automation и для прочих сценариев — как сейчас.
- `startup`: `WaitUntil` «restored tabs» (вкладок ≥ 3), `WaitUntil` «active tab highlighted», 2 с timed-скролла существующим шагом, `Finish`.
- `welcome`: `WaitUntil` «welcome visible», `ApplyWorkspace`, `Call` «enter ide» (`enter_ide_mode_deferred` + `finish_ide_deferred`, как команда `workspace` драйвера `headless/mod.rs:494-496`), `WaitReady`, `WaitFileTree`, `Finish`.
- Под `cfg(test)` `load_open_tabs`/`save_open_tabs` — no-op (`src/state_persistence.rs:333,360`), поэтому тест засевает сессию через `App::preload_ide_session` (`app_ide_startup_methods.rs:290`), а файл сессии проверяется unit-тестом парсера.

- [ ] Step 1: тесты: (a) unit — таблица `saves_session_on_exit`/`restores_session` для всех вариантов; (b) headless — сессия со сценарием `startup`, перед прогоном `preload_ide_session` с 3 файлами → `success`, 3 вкладки; (c) Review Focus 3 — `startup` без preload → `success == false`, `failed_step` = «restored tabs»; unit на `parse_open_tabs_content_checked` (или фактический парсер файла сессии) с мусорными байтами → ошибка без паники; (d) `welcome` → `success`.
- [ ] Step 2: `make test TEST_FILTER=headless::ui_tests_pgo > /tmp/rriter-t4.log 2>&1` → FAIL.
- [ ] Step 3: реализация.
- [ ] Step 4: прогон → PASS; `headless::ui_tests_ide_startup` и `headless::ui_tests_welcome` → PASS.
- [ ] Step 5: Commit `PGO scenarios: startup with restored session, welcome`.

### Task 5: Группа `pdf`

**Агенты:** implementer ds-low-agentic · reviewer нет.

**Files:** Create `src/app/automation_pdf.rs`; Modify `src/pdf/mod.rs:4` (гейт `cfg(test)` снять с модуля `fixture`, а `write_fixture_pdf_mixed`, `write_fixture_pdf_highlight`, `write_garbage`, `write_empty` пометить `#[cfg(test)]` — в prod-сборке они не используются), `src/app/automation_groups.rs`, `src/app.rs`, `src/headless/ui_tests_pgo.rs`.

**Шаги группы** (образец — `src/headless/ui_tests_pdf.rs:56-80,228-256`): `Call` — `write_fixture_pdf(<ws>/pgo_pdf)` (файл `rriter-fixture.pdf`, 3 страницы) и открыть вкладку; `WaitUntil` «pdf rasterized»; `Wheel` вниз ×20; `Key("pagedown")` ×3; `Key("end")`, `Key("home")`; `Click` по `PdfDarkToggle` дважды; `Key("ctrl+f")`, `TypeText("second")`, `WaitUntil` «pdf search done» (совпадений 2).

- [ ] Step 1: тест `pgo_group_pdf` — `run_pgo_scenario("group:pdf", 60_000)` → `success`.
- [ ] Step 2: `make test TEST_FILTER=headless::ui_tests_pgo::pgo_group_pdf > /tmp/rriter-t5.log 2>&1` → FAIL.
- [ ] Step 3: реализация.
- [ ] Step 4: прогон → PASS; `headless::ui_tests_pdf` → PASS.
- [ ] Step 5: Commit `PGO group: pdf`; затем строка в `FULL_GROUPS` — Commit `PGO: pdf in full scenario`.

### Task 6: Группа `api_mock`

**Агенты:** implementer ds-low-agentic · reviewer нет.

**Files:** Create `src/app/automation_api_mock.rs`; Modify реестр, `src/app.rs`, `src/headless/ui_tests_pgo.rs`; при переносе хелпера — `src/headless/tests.rs` (только удаление перенесённого).

**Шаги** (образцы — `src/headless/ui_tests_api_mock.rs:21-34,79`, `src/headless/tests.rs:926-1010`): `Call` — `app.ide_panel.api.mock.port = 0`; `Click` `SidebarSlot(ApiClient)`; `Click` `ApiMockAddManualRoute`; `Click` `ApiMockStaticResponseInput(0)`, `Key("ctrl+a")`, `TypeText` ответа; `Click` `ApiMockServerToggle`; `WaitUntil` «mock server running»; `Call` — запрос к mock на loopback в `std::thread::spawn` сырым `TcpStream`, как `request_to_mock` (`tests.rs:962-986`): эту функцию перенести из `cfg(test)` в модуль API Mock (production) и звать из теста и из группы — не копировать; `WaitUntil` «mock request logged» по `app.ide_panel.api.mock_server_logs`; повторить запрос ×5; `Click` `ApiMockServerToggle` (стоп). Горячее обновление и Python-маршруты не входят (нет UI-пути для static, Python требует `uv`).

- [ ] Step 1–4 как в Task 5: `pgo_group_api_mock` (60 с) → FAIL → реализация → PASS; `headless::ui_tests_api_mock` → PASS.
- [ ] Step 5: Commit `PGO group: api_mock`; строка в `FULL_GROUPS` — Commit `PGO: api_mock in full scenario`.

### Task 7: Группа `git_changes`

**Агенты:** implementer ds-low-agentic · reviewer нет.

**Files:** Create `src/app/automation_git_changes.rs`; Modify реестр, `src/app.rs`, `src/headless/ui_tests_pgo.rs`.

**Шаги** (образцы — `ui_tests_git_diff.rs:58-146`, `ui_tests_git_commit.rs:50-72`, `ui_tests_deleted_tab.rs:76-82`): `Call` — `<ws>/pgo_git/changes.py` (≥ 300 строк) и `<ws>/pgo_git/gone.py` закоммитить в fixture-репозиторий, затем изменить `changes.py` в ≥ 5 местах; открыть git-панель, `WaitUntil` «status shows changes.py»; `Click` `GitFileDiff(0,0)` с `clicks: 2`; `WaitUntil` «diff view open»; `Click` `GitDiffNextHunk` ×4, `GitDiffPrevHunk` ×2; `Wheel` по diff ×10; `Click` `GitDiffRollbackHunk(0, 0)`, `Key("ctrl+s")`; `Click` `GitFile(0,0)` (stage); `Click` `GitMessageInput`, `TypeText("pgo commit")`, `Click` `GitCommit`; `WaitUntil` «commit landed» — по состоянию git-панели (список изменений пуст / счётчик коммитов графа вырос), без статиков и без внешнего `git`; `Call` — открыть `gone.py` во вкладке и удалить файл с диска; `WaitUntil` «deleted tab» (состояние вкладки `deleted`, которое `dump` показывает как `tab["deleted"]`).

- [ ] Step 1–4: `pgo_group_git_changes` (90 с) → FAIL → реализация → PASS; `headless::ui_tests_git_diff` → PASS.
- [ ] Step 5: Commit `PGO group: git_changes`; строка в `FULL_GROUPS` — Commit `PGO: git_changes in full scenario`.

### Task 8: Группа `editor_ops`

**Агенты:** implementer ds-low-agentic · reviewer нет.

**Files:** Create `src/app/automation_editor_ops.rs`; Modify реестр, `src/app.rs`, `src/headless/ui_tests_pgo.rs`.

**Шаги** (образцы — `ui_tests_multi_cursor.rs:30-45`, `ui_tests_editor.rs:316,335-342,397`): `Call` — `<ws>/pgo_editor/ops.py` (≥ 200 строк) и открыть; multi-cursor — `Click` с `mods: "alt"` в 5 строках (`AutomationTarget::Find` по смещению строки, как тест считает через `line_offsets`), `TypeText("_x")`, `Key("escape")`; `Key("ctrl+a")`, `Key("ctrl+c")`, `Key("ctrl+end")`, `Key("ctrl+v")` ×3; `Key("ctrl+z")` ×5, `Key("ctrl+y")` ×3; `WaitUntil` «buffer changed» по состоянию редактора.

- [ ] Step 1–4: `pgo_group_editor_ops` (60 с) → FAIL → реализация → PASS; `headless::ui_tests_multi_cursor` → PASS.
- [ ] Step 5: Commit `PGO group: editor_ops`; строка в `FULL_GROUPS` — Commit `PGO: editor_ops in full scenario`.

### Task 9: Группа `lsp_nav`

**Агенты:** implementer ds-low-agentic · reviewer нет.

**Files:** Create `src/app/automation_lsp_nav.rs`; Modify реестр, `src/app.rs`, `src/headless/ui_tests_pgo.rs`.

**Шаги** (образец — `ui_tests_goto_definition.rs:27-48,111-118,187-222`): `requires` — `ty` резолвится (`platform::resolve_executable`/`command_for_tool`, как решает `skip_without_ty`), иначе `Err("ty not found")` → группа пропускается; `Call` — `<ws>/pgo_lsp/nav.py` (функции с вызовами между ними) и открыть; `WaitUntil` «ty running» (статус LSP); goto definition на 3 вызовах — ctrl+hover и клик так, как тест (`begin_ctrl_hover`, `Click` с `mods: "ctrl"`); `WaitUntil` «cursor at definition» после каждого.

- [ ] Step 1: тест `pgo_group_lsp_nav` — `run_pgo_scenario("group:lsp_nav", 90_000)` → `success`; если `ty` нет — в отчёте `skipped_groups` с `lsp_nav` (тест проверяет одно из двух по наличию `ty`, как `skip_without_ty`).
- [ ] Step 2–4: FAIL → реализация → PASS; `headless::ui_tests_goto_definition` → PASS.
- [ ] Step 5: Commit `PGO group: lsp_nav`; строка в `FULL_GROUPS` — Commit `PGO: lsp_nav in full scenario`.

### Task 10: Группа `input_scroll`

**Агенты:** implementer ds-low-agentic · reviewer нет.

**Files:** Create `src/app/automation_input_scroll.rs`; Modify реестр, `src/app.rs`, `src/headless/ui_tests_pgo.rs`.

**Шаги** (образец скроллбара — `ui_tests_editor.rs:3-30`, `render_view::editor_vertical_scrollbar`): `Call` — детерминированно сгенерировать `<ws>/pgo_scroll/big.rs` (20 000 строк реалистичного Rust: функции, match, строки, комментарии; без rand) и открыть; `Wheel` вниз ×60 и вверх ×30 по тексту с `WaitFrames(2)` между пачками по 10 (анимация); `Key("pagedown")` ×20, `Key("pageup")` ×10; `Key("ctrl+end")`, `Key("ctrl+home")`; `Drag` ползунка `EditorScrollbarY` сверху вниз за 40 шагов (`AutomationTarget::Find` — позиция ползунка из геометрии `editor_vertical_scrollbar`); `WaitUntil` «scrolled to bottom».

- [ ] Step 1–4: `pgo_group_input_scroll` (60 с) → FAIL → реализация → PASS; `headless::ui_tests_editor` → PASS.
- [ ] Step 5: Commit `PGO group: input_scroll`; строка в `FULL_GROUPS` — Commit `PGO: input_scroll in full scenario`.

### Task 11: Группа `terminal_ops`

**Агенты:** implementer ds-low-agentic · reviewer нет.

**Files:** Create `src/app/automation_terminal_ops.rs`; Modify реестр, `src/app.rs`, `src/headless/ui_tests_pgo.rs`.

**Шаги** (образцы — `ui_tests_terminal.rs:25-100`, `tests.rs:192-200`): `requires` — `cfg!(unix)`, иначе `Err("terminal workload is unix-only")`; открыть терминал; `Click` `TerminalAdd`; `Click` `TerminalTab(1)`; `Click` `TerminalBody`, `TypeText("seq 1 20000")`, `Key("enter")`; `WaitUntil` «output done» (в буфере есть `20000`); `Key("ctrl+f")`, `TypeText("1999")`, `WaitUntil` «terminal search shown»; `Key("escape")`; `Click` `TerminalTab(0)`, `Click` `TerminalTab(1)`; `Click` `TerminalTabClose(1)`.

- [ ] Step 1–4: `pgo_group_terminal_ops` (60 с) → FAIL → реализация → PASS; `headless::ui_tests_terminal` → PASS.
- [ ] Step 5: Commit `PGO group: terminal_ops`; строка в `FULL_GROUPS` — Commit `PGO: terminal_ops in full scenario`.

### Task 12: Пайплайн: headless на Linux, три прогона, покрытие, маркеры

**Агенты:** implementer ds-low-agentic · reviewer ds-high — high: рефакторинг `run_training` и `validate_training_environment` затрагивает упаковку Windows/macOS (`build_windows.py:668`, `build_macos.py:221`), которую локально не прогнать.

**Files:** Create `scripts/pgo_coverage.py`, `scripts/pgo_fixtures.py` (вынос `_write_fixture_files` и `_openapi_*`, `pgo_pipeline.py:497-725,800-903`; в `pgo_pipeline.py` — re-export для `tests/test_build_automation.py:743,786`), `tests/test_pgo_coverage.py`; Modify `scripts/pgo_pipeline.py`, `src/app/automation.rs:14`, `tests/test_build_automation.py` (только если без правки не проходит).

**Решения:**
- `validate_training_environment` (`:1041-1055`): проверки платформ остаются, на Linux удаляется только требование `WAYLAND_DISPLAY`; `WINIT_UNIX_BACKEND` (`:988-989`) — удалить. На Linux argv — headless (`--headless --pgo-train --pgo-scenario S --pgo-workspace --pgo-report --pgo-timeout-seconds --profile <state>/headless-profile[-welcome]`), на Windows/macOS — прежний GUI-argv и один прогон `full`. API `PgoConfig`/`run_pipeline` для `build_*.py` не меняется (новые поля — с дефолтами).
- `run_training` (`:1090-1196`) → `start_fixtures()` (PG + `LocalApiServer`, один раз, остановка в `finally`) и `run_scenario(name, report_path)`; отчёт `automation-report-<scenario>.json`; проверки `local_api_request_count >= 1` (`:1189`) и `validate_database_fixture_telemetry` (`:1187`) — только для `full`; `scenario_version` — для всех; код ≠ 0 → ошибка с именем сценария, `failed_step` из отчёта и хвостом stderr (30 строк); `skipped_groups` → предупреждение в лог. Прогоны `full` и `startup` делят `--profile`-каталог, `welcome` — свой.
- `--run-only`/`--run-executable` (`:1198-1238`, `pgo-script`) идут тем же `run_scenario` по всем `--scenarios`; `automation_sources` для проверки свежести (`:1220-1223`) — glob `src/app/automation*.rs` + `src/headless/*.rs`.
- `RRITER_PDFIUM_PATH` = содержимое `target/pdfium.path`; нет файла / путь не существует → ошибка «pdfium not found, run `make pdfium`» до любой сборки.
- `ty` для `lsp_nav`: `PATH` хоста наследуется (`base_environment`, `:395-398`); если `ty` на `PATH` нет, а у хоста есть managed-установка RRiter (путь — по `src/app/tool_installer.rs`), её каталог добавляется в `PATH` прогона. Не нашёлся — группа пропустится сама (Task 9), пайплайн печатает предупреждение.
- Merge — `llvm-profdata merge` без `-sparse` (`:1263`); `LLVM_PROFILE_FILE` уже `rriter-%p-%m.profraw` (`:966`). После прогонов каждый `.profraw` непустой, иначе ошибка.
- `pgo_coverage.py`: `llvm-profdata show --all-functions --counts` (тем же способом, что вызывается `llvm-profdata`, `:1249-1252`) → `llvm-cxxfilt` через `shutil.which("llvm-cxxfilt")` (в rustup его нет; нет в `PATH` → ошибка до тренировки) → `target/pgo-profiles/<triple>/coverage.txt` (модуль | сработало | всего) + в лог модули `rriter::*` с нулём. `GROUP_MARKERS`: 1–3 подстроки деманглированных имён на каждую группу (`pdf`, `api_mock`, `git_changes`, `editor_ops`, `lsp_nav`, `input_scroll`, `terminal_ops`), на `startup`, `welcome` и на существующие крупные части `full` (markdown, database, api_client, git graph, terminal, settings, project search). Маркер — функция, выполняемая только на пути группы. `check_markers` пропускает группы из `skipped_groups`; непустой результат → ошибка «группа: маркер», финальная сборка не идёт. Формат вывода `show` локально не проверен — парсер терпим к лишним строкам, тест на фрагменте из документации LLVM, главная сессия сверит на первом `make max`.
- `count_pgo_warnings(log)` по логу use-сборки: число «no profile data available for function» и «hash mismatch»; функции горячих модулей (`render_view`, `renderer`, `editor`, `highlighter`, `scroll`, `app::mouse`, `app::keyboard`) — отдельным списком «HOT» в лог; сборку не валит. Проверь, что use-сборка (`build_with_profile`, `:476`) передаёт `-Cllvm-args=-pgo-warn-missing-function` (сейчас есть только в `Makefile:282`); нет — добавить.
- `--install-binary PATH`: копия итогового бинаря атомарно (temp рядом + `os.replace`).
- Версия сценария 18 в обоих местах.

- [ ] Step 1: тесты `tests/test_pgo_coverage.py`: `parse_profdata_show` на фрагменте формата; Review Focus 5 — `module_of` на `rriter::app::git_diff::build_diff_view`, `<rriter::pdf::worker::Worker>::run`, `<rriter::app::App as core::fmt::Debug>::fmt`, `rriter::app::f::{closure#0}`, `_RNvCs_garbage`, `std::io::stdio::print`; `check_markers` (все найдены / маркер с нулём / маркер отсутствует / группа в skipped); `count_pgo_warnings` на фрагменте лога с HOT и не-HOT функцией; Review Focus 4 — поиск pdfium на отсутствующем `pdfium.path` и на пути к несуществующему файлу, отсутствие `llvm-cxxfilt` → исключение с подсказкой; headless-argv для `full`/`welcome` на Linux и GUI-argv на не-Linux (платформа параметром).
- [ ] Step 2: `python3 -m unittest tests/test_pgo_coverage.py` → FAIL.
- [ ] Step 3: реализация; `wc -l scripts/pgo_pipeline.py` ≤ 1600.
- [ ] Step 4: `python3 -m unittest tests/test_pgo_coverage.py tests/test_build_automation.py` → PASS; `python3 scripts/pgo_pipeline.py --self-test` → PASS; для каждого маркера — `rg`, что функция существует и вызывается на пути группы (список «маркер → `file:line`» в отчёт).
- [ ] Step 5: Commit `PGO pipeline: headless training on Linux, 3 scenarios, coverage and group markers`.

### Task 13: Makefile и документация

**Агенты:** implementer ds-low · reviewer нет. Строго после Task 12 (`make max` зовёт новые флаги пайплайна).

**Files:** Modify `Makefile`, `docs/headless.md`, `PROJECT_GUIDE.md` §4, `PGO_AUTOMATION_PLAN.md`.

**Правки Makefile (продиктованы):**
- `max-nopgo:` — нынешнее тело `max` (`:56-65`) без изменений; `max-nopgo` в `.PHONY` (`:29`).
- `max: pdfium` — вызов `python3 scripts/pgo_pipeline.py` с аргументами нынешнего `pgo-auto` (`:286-297`) + `--install-binary "target/$(TARGET)/release/$(BINARY_NAME)"`; эхо «🔥 MAX: свежий headless PGO → Fat LTO».
- `pgo-auto: max`, `pgo: max` — синонимы без тела. `all: max` (`:31`) оставить.
- `pgo-train: pdfium`, `pgo-use: pdfium`, `pgo-script: pdfium`; в `@echo` у `pgo-script`, `pgo-gen-fast`, `pgo-auto`, `pgo-train` «GUI» → «headless».
- `pgo-run` (`:261-265`): instrumented-бинарь с `RRITER_PDFIUM_PATH=$$(cat target/pdfium.path)` и headless-argv (`--headless --pgo-train --pgo-scenario full --pgo-workspace ... --pgo-report ... --profile ...`); удалить эхо «ЗАКРОЙТЕ редактор» (`:263`); `pgo-run: pdfium`.
- `pgo-bench-build`: удалить строку `--rustflag=-Clto=fat` (`:221`).

**Документация:** `docs/headless.md` — раздел «PGO training» (флаги, коды выхода, сценарии, `group:<name>` для итераций, пропуск групп); `PROJECT_GUIDE.md` §4 — новые `src/app/automation_*.rs`, `scripts/pgo_coverage.py`, `scripts/pgo_fixtures.py`; `PGO_AUTOMATION_PLAN.md` — строка сверху: на Linux GUI-тренировка заменена headless, ссылка на спеку.

- [ ] Step 1: правки.
- [ ] Step 2: `make -n max`, `make -n pgo-bench-build`, `make -n pgo-run` — без ошибок make; `python3 scripts/pgo_pipeline.py --help | rg 'install-binary|scenarios'` — оба флага есть.
- [ ] Step 3: Commit `Makefile: make max builds headless PGO, max-nopgo, fix pgo-bench-build LTO flag; docs`.

### Task 14: Финальная проверка (главная сессия)

Не для сабагента. Финальное ревью ветки (`ds-high` + `final-reviewer`) → правки → `make max > /tmp/rriter-max.log 2>&1` в фоне → код 0, `target/<triple>/release/rriter` обновлён, маркеры прошли, `coverage.txt` есть, реальный формат `llvm-profdata show` совпал с парсером, список HOT-предупреждений (непустой — разбор до приёмки) → `make pgo-bench-build` собирается → `make codex_test` → PR и merge по AGENTS §3.
