# Headless-режим RRiter — план реализации

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `rriter --headless` — запуск без окна на offscreen EGL, ввод командами из stdin/скрипта, PNG кадра, JSON-дамп состояния UI, замеры стоимости кадра; ничего не появляется на экране и не трогает состояние живого редактора.

**Architecture:** Headless — третий хост того же `App`: пять рефакторингов без изменения поведения окна (`WindowHost`, `HostLoop`, `KeyInput`, перенос `OffscreenContext` в production, вынос кадра и конструктора `App`) дают production-путь ввода/кадра без winit-окна; поверх них модуль `src/headless/` (профиль, протокол, цикл кадров, снимок, дамп, бенч) и Python-обёртка.

**Tech Stack:** Rust edition 2024, winit 0.30.13, glow 0.18, EGL через `dlopen` (`libEGL.so.1`, surfaceless Mesa-платформа), `image` 0.25 (png), `serde_json`; обёртка — Python 3 stdlib.

**Spec:** `docs/superpowers/specs/2026-09-25-headless-mode-design.md` (коммит 5c6319d). Исполнитель читает раздел спеки, указанный в задаче; факты с `file:line` в спеке — от master 5c6319d, номера строк могли сдвинуться после предыдущих задач — искать по символу.

## Global Constraints

- Окно не меняет поведения: все рефакторинги (Task 2–7) проходят существующие тесты без правок ассертов; меняются только сигнатуры вызовов в тестах.
- Headless только Linux: всё новое в `src/headless/` и `src/platform/offscreen_gl.rs` под `#[cfg(target_os = "linux")]`; на других ОС `--headless` → stderr `headless mode is supported on Linux only`, код 2.
- Коды выхода headless: 0 — все команды `ok`, выход по `quit`/EOF; 1 — хотя бы один `err`; 2 — аргументы/платформа; 3 — GL-контекст или `Renderer`.
- Новых крейтов нет (`image`+`png`, `serde_json`, `glow`, `winit`, `libc` уже в `Cargo.toml`).
- `--size` по умолчанию `1920x1080`, границы 320x200 … 8192x8192; `--scale` по умолчанию 1.0, команда `scale` — 0.5…4.0; `wait` ≤ 60000 мс; `record` ≤ 600 кадров; `settle` по умолчанию 500 мс.
- Бюджет кадра: `--budget-ms F` | `--hz N` → `1000/N` | частота монитора | 240 Гц (`hz_source` = `arg|monitor|default`).
- Никаких `.unwrap()`/`.expect()` в новом production-коде (тесты — можно). Существующие `self.window.as_ref().unwrap()` не трогать.
- Файлы: не больше 1600 строк; файлы, уже превысившие лимит (`main.rs` 2423, `events.rs` 1675, `about.rs` 1684, `app_state.rs` 1619, `editor_keys.rs` 1617, `mouse/input.rs` 2695, `automation.rs` 3581, `markdown_scroll_transition_review_tests.rs` 1999), могут расти максимум на несколько строк (поля структуры, изменение сигнатуры); логику класть в новые/другие файлы, как указано в задачах. Новый файл — не меньше 200 строк (исключения названы в задачах). Каждый новый файл — строка в `PROJECT_GUIDE.md` §4 в той же задаче.
- Без `rustfmt`/`cargo fmt` (дерево не fmt-clean) — стиль соседних строк руками.
- Тесты: `make test TEST_FILTER='<подстрока имени теста>' > /tmp/rriter-task<N>.log 2>&1`, затем `tail`/`grep` лога. Только foreground. Не больше трёх прогонов на шаг «до зелёного» — потом отчёт с падениями. Полный `make test` / `make codex_test` — только где задача это требует (Task 1 и финал).
- Глобальные `OnceLock` headless (`set_headless`, `set_app_root_override`) в тестах: `set_headless` тесты НЕ вызывают никогда (он перекрыл бы запись файлов для всех последующих тестов процесса); политика тестируется через чистые функции с параметром `Option<HeadlessPolicy>`. `set_app_root_override` тесты ставят только через `headless::tests_support::ensure_test_profile_root()` (Task 11) — один временный корень на тестовый процесс.
- Коммиты: `git add <явные пути>`; никогда `git add -A`/`git add .` (в workspace `.superpowers/` лежат артефакты SDD). Сообщение в стиле репозитория (`headless: …;`), последней строкой `Co-Authored-By: Claude Opus 5.5 (1M context) <noreply@anthropic.com>`. Дерево собирается после каждого коммита.
- Никаких `git push`, сетевых команд, установки пакетов.
- Предупреждения `dead_code` о headless-коде, который ещё не вызывается (между T9 и T11), допустимы; `#[allow(dead_code)]` не ставить.

## Решения плана (сверх буквы спеки)

Приняты до диспатча, чтобы не решались по ходу в разных задачах:

1. **`capture.rs` не создаётся:** readback+PNG (спека 4.7) живёт в `src/headless/frame.rs` — отдельный файл был бы ~60 строк (правило AGENTS.md: новый файл ≥ 200 строк). `HeadlessOptions`/CLI и профиль — в `profile.rs`.
2. **Override корня профиля проверяется в `app_paths()`, не в `app_paths_with`:** `app_paths_with` — чистая функция, её тесты (`src/platform/tests.rs`) иначе ломались бы после первого теста, выставившего override.
3. **`HostLoop::Headless` держит `&HeadlessLoopState`** (`exit_requested: AtomicBool`, `last_control_flow: Cell<ControlFlow>`), а не две ссылки — тестам нужен один объект.
4. **`FrameOutcome` без `wants_another_frame`:** `request_redraw()` внутри кадра идёт через `WindowHost::Headless` и сам ставит флаг, который читает `step_frame`. `FrameOutcome` несёт только локальные переменные, вычисленные до презентации и читаемые после (замеры автокомплита).
5. **Guards `render_suspended` и `!is_ready` остаются в оконной ветке** (первый кадр окна — очистка + present). Headless при старте ставит `app.is_ready = true`, `app.tried_maximize = true`.
6. **`App.headless_mode: bool`** (ставит `new_from_config`): App-уровневые отличия headless (мигание курсора, clipboard, проба dart) читают его, а не глобальную политику — иначе в тестах мигание не отключилось бы и `settle` никогда не сходился. Глобальная политика (`platform::is_headless`) — только для платформенных функций (пикеры, `open_url`, `reveal_path`, запись файла) и печати телеметрии.
7. **Тонкая обёртка `handle_main_keyboard_input(KeyEvent)` живёт в `key_input.rs`** (`main_keys.rs` 1593 строки).
8. **`App::new_from_config` — новый файл `src/app/app_bootstrap.rs`** (литерал `App { … }` + код после него до `run_app` + FAQ-текст ≈ 250 строк, `main.rs` худеет).
9. **`render_main_frame`/`finish_main_frame` — новый файл `src/app/events/main_frame.rs`** (≈ 900 строк уходят из `events.rs`).
10. **`modal_dialog_open()` — в `app_window_external_methods.rs`**, поля — в `app_state.rs` (+2 строки к 1619, осознанное исключение: только поля).
11. **In-process раннер:** `headless::HeadlessSession` — App + GL + состояние цикла + `execute(Command)`; `run()` = аргументы → глобальные политики → сессия → цикл. Интеграционные тесты строят сессию без `set_headless`.
12. **Пробa частоты монитора подменяема:** сессия хранит `hz_probe: fn() -> Option<f64>`; тесты ставят `|| None` (второй `EventLoop` в процессе winit не даёт).
13. **`dump` читает `writes_allowed` через `platform::headless_writes_allowed()`**; в тестах (политика не выставлена) это `true` — ассерты тестов этого поля не проверяют.

## Общие интерфейсы

| Символ | Где | Сигнатура / состав | Производит → потребляет |
|---|---|---|---|
| `WindowHost` | `src/platform/window_host.rs`, `pub use` в `platform` | `enum { Native(Arc<winit::window::Window>), Headless(HeadlessWindow) }`; методы с именами winit: `request_redraw()`, `inner_size() -> PhysicalSize<u32>`, `scale_factor() -> f64`, `set_title(&str)`, `set_maximized(bool)`, `is_maximized() -> bool`, `focus_window()`, `request_inner_size<S: Into<Size>>(S) -> Option<PhysicalSize<u32>>`, `set_ime_allowed(bool)`, `set_cursor(impl Into<Cursor>)`, `id() -> WindowId`, `window_handle()` (как у winit, для Native; Headless → `Err(HandleError::NotSupported)`), `native() -> Option<&Arc<Window>>`, `headless() -> Option<&HeadlessWindow>` | T2 → T3, T4, T6, T11, T12 |
| `HeadlessWindow` | там же | `new(size: PhysicalSize<u32>, scale_factor: f64) -> Self`; `take_redraw_request(&self) -> bool`; `set_size(&self, PhysicalSize<u32>)`; `set_scale_factor(&self, f64)`; `title(&self) -> String`; `cursor_icon(&self) -> CursorIcon`; `ime_allowed(&self) -> bool`; поля приватные (`Mutex`/`AtomicBool`, `id = WindowId::dummy()`) | T2 → T11, T12 |
| `OffscreenContext` | `src/platform/offscreen_gl.rs` (`cfg(target_os="linux")`) | `new(width: u32, height: u32) -> Result<Self, String>`; `glow(&self) -> glow::Context`; `resize(&mut self, w: u32, h: u32) -> Result<(), String>`; `requested_context(&self) -> String`; `gl_strings(&self) -> &GlStrings` (`pub renderer, version, vendor: String`); `size(&self) -> (u32, u32)` | T3 → T11, T14 |
| `offscreen_test_app` | `offscreen_gl.rs`, `#[cfg(test)] pub(crate) mod test_support` | `offscreen_test_app(width: u32, height: u32, scale: f32) -> (OffscreenContext, App)` — `reviewer_stage2_test_app()` + `Renderer` + `renderer.resize(w,h)` + `window = Some(Arc::new(WindowHost::Headless(HeadlessWindow::new(..))))` | T3 → T4, T5, T6 |
| `HostLoop`, `HeadlessLoopState` | `src/app/events/host_loop.rs` | `pub struct HeadlessLoopState { pub exit_requested: AtomicBool, pub last_control_flow: Cell<ControlFlow> }` (`Default`: `false`, `Wait`); `pub enum HostLoop<'a> { Native(&'a ActiveEventLoop), Headless(&'a HeadlessLoopState) }`; `HostLoop::headless(&HeadlessLoopState) -> HostLoop<'_>`; `exit(&self)`; `set_control_flow(&self, ControlFlow)`; `native(&self) -> Option<&ActiveEventLoop>` | T4 → T5, T11, T12 |
| `KeyInput` | `src/app/keyboard/key_input.rs` | `pub struct KeyInput { pub physical_key: PhysicalKey, pub logical_text: Option<SmolStr>, pub text: Option<SmolStr>, pub state: ElementState, pub repeat: bool }` (`Clone, Debug, PartialEq`); `impl From<&KeyEvent>`; `KeyInput::parse_combo(&str) -> Result<(KeyInput, ModifiersState), String>`; `KeyInput::released(&self) -> KeyInput`; `App::handle_main_key_input(&mut self, host: &HostLoop, input: KeyInput)` | T5 → T11, T14 |
| `render_main_frame` | `src/app/events/main_frame.rs` | `App::render_main_frame(&mut self) -> FrameOutcome`; `App::finish_main_frame(&mut self, outcome: FrameOutcome)`; `pub(crate) struct FrameOutcome { /* приватные поля: локальные, нужные после презентации */ }` | T6 → T11 |
| `new_from_config` | `src/app/app_bootstrap.rs` | `App::new_from_config(config: Config, options: AppInitOptions) -> App`; `pub(crate) struct AppInitOptions { …поля по локальным main.rs…, pub headless: bool }`; `AppInitOptions::headless() -> Self`; поле `App.headless_mode: bool` | T7 → T11 |
| Политика headless | `src/platform.rs` | `HeadlessPolicy { allow_writes: bool }`, `set_headless`, `headless_policy() -> Option<HeadlessPolicy>`, `is_headless()`, `headless_writes_allowed()`, `editor_writes_allowed(Option<HeadlessPolicy>) -> bool`, `ExternalRequest`, `note_external_request`, `take_external_request`, `intercept_external(Option<HeadlessPolicy>, ExternalRequest) -> bool` — полный код в Task 8 | T8 → T11, T12 |
| Корень профиля | `src/platform/integration.rs` | `set_app_root_override(PathBuf) -> Result<(), PathBuf>`, `app_root_override() -> Option<&'static Path>`, `app_paths_for_root(&Path) -> AppPaths` — код в Task 8 | T8 → T10, T11 |
| Протокол | `src/headless/protocol.rs` | `pub enum Command` (варианты в Task 9), `parse_line(&[u8]) -> Result<Option<Command>, String>` (`Ok(None)` — пустая/комментарий), `enum Response { Ok(Option<String>), Err(String) }` + `Response::line(&self) -> String` | T9 → T11–T15 |
| Опции | `src/headless/profile.rs` | `HeadlessOptions { script: Option<PathBuf>, size: (u32,u32), scale: f64, profile: ProfileChoice, budget: BudgetChoice, keep_profile: bool, allow_writes: bool, path: Option<PathBuf> }`; `enum ProfileChoice { Temp, Dir(PathBuf), FromUser }`; `enum BudgetChoice { Auto, Hz(f64), Ms(f64) }`; `parse_args(args: &[OsString]) -> Result<HeadlessOptions, String>`; `Profile::prepare(&HeadlessOptions) -> Result<Profile, String>`, `Profile::root(&self) -> &Path`, `Profile::finish(self)` (удаляет Temp, если не keep) | T10 → T11, T14 |
| Сессия | `src/headless/mod.rs` | `pub fn run(args: &[OsString]) -> ExitCode`; `pub(crate) struct HeadlessSession { app, gl: OffscreenContext, loop_state: HeadlessLoopState, had_error: bool, hz_probe: fn() -> Option<f64>, budget: BudgetChoice, profile_root: PathBuf, … }`; `HeadlessSession::new(options: &HeadlessOptions, profile_root: PathBuf) -> Result<Self, (u8, String)>`; `execute(&mut self, cmd: Command) -> Response`; `run_loop(&mut self, input: impl BufRead, out: impl Write) -> bool` (true = выход по quit/EOF/закрытому stdout); `exit_code(&self) -> u8` | T11 → T12–T15 |
| Кадр | `src/headless/frame.rs` | `step_frame(session_parts…, force: bool) -> bool` (кадр нарисован); `settle_loop(budget: Duration, step: impl FnMut() -> StepState) -> (u32, bool)`; `enum StepState { Redrawn, Idle { flow: ControlFlow } }`; `read_frame_rgba(gl: &glow::Context, w: u32, h: u32, buf: &mut Vec<u8>)`; `flip_rows_in_place(buf: &mut [u8], w: usize, h: usize)`; `write_png(path: &Path, buf: &[u8], w: u32, h: u32) -> Result<PathBuf, String>` | T11 → T12, T14, T15 |
| Телеметрия кадра | `src/render_view/root_helpers.rs` | `pub fn take_frame_telemetry() -> FrameTelemetry { flush_calls, vertices, root_phase_ms: [f32; 5], chrome_ms: [f32; 6] }` | T14 → T15 |
| Проба монитора | `src/platform.rs` | `pub fn probe_display_refresh_hz() -> Option<f64>` | T14 |

## Review Focus

1. **Оконный режим после рефакторингов 3.1–3.5** (порядок операций кадра, present, заголовок, максимизация, диалог вторым окном) — тесты окно не поднимают. Ожидание: поведение окна идентично. Закрепляют: характеризационные тесты Task 1 (ввод на offscreen-фикстуре, неизменные ассерты после Task 2/4/5) и тест Task 6 (`render_main_frame` на фикстуре регистрирует те же `UiId`, что ручной `Renderer::draw`); ревьюер Task 6 проверяет, что diff ветки `RedrawRequested` — только вынос блоков и вызовы.
2. **Headless трогает состояние пользователя** — запись мимо `app_paths()` (прямые `HOME`/`XDG_*`/`dirs::`), системный clipboard, `xdg-open`, пикеры, запись открытого файла. Ожидание: состояние RRiter пишется только в корень профиля; открытые файлы, мутации файлового дерева и Git — только с `--allow-writes` (решение пользователя 2026-09-25); на экране ничего. Закрепляют: Task 8 — тесты `app_paths_for_root`, `editor_writes_allowed`, `intercept_external` и аудит прямых путей в отчёте; Task 7 — `clipboard == None`.
3. **Бесконечная анимация / фоновые потоки в `settle`** — ожидание: `settled=false` по тайм-ауту, не зависание. Закрепляет Task 11: `settle_loop` с шагом, всегда возвращающим `Redrawn`, завершается за бюджет.
4. **Ошибка ввода-вывода и закрытый stdout** — `screenshot` в недоступный каталог, stdout закрыт читателем. Ожидание: `err io: …`, следующая команда выполняется; при `BrokenPipe` — штатный выход с кодом 0. Закрепляет Task 11: `screenshot /proc/rriter-nope/x.png` → `err io:` и следующий `dump`/`info`/`quit` → `ok`; `run_loop` с writer'ом, возвращающим `BrokenPipe`, завершается без паники.
5. **Сохранение защищённого файла в headless с `--allow-writes`** — `write_current_text_to_path` при `PermissionDenied` уходит в `write_text_file_elevated` → `pkexec` (`app_window_external_methods.rs:~456`, `platform/elevated_save.rs:~243`): окно пароля поверх игры. Ожидание: в headless эскалации нет никогда, ошибка записи — тот же readonly-notice. Закрепляет Task 8: `elevation_allowed(Some(..)) == false`, гейт перед вызовом эскалации.

## Порядок и зависимости

```
T1 страховка ─► T2 WindowHost ─► T3 offscreen_gl ─► T4 HostLoop ─► T5 KeyInput ─► T6 кадр ─► T7 new_from_config ─► T8 политика
                                                                                                                        │
T9 протокол (после T5, до T11) ─────────────────────────────────────────────────────────────────────────────────────────┤
                                                                                                                        ▼
                                                     T10 CLI+профиль ─► T11 рантайм ─► T12 диалог+dump ─► T13 обёртка+docs ─► T14 bench+info ─► T15 record
```

Задачи последовательны (общие файлы, один сборщик в основном чекауте). T9 зависит от T5 (`parse_combo`) и делит с T7 только строку объявления модуля в `main.rs` — её можно исполнить в любой момент между T5 и T11.

---

### Task 1: Страховка — baseline тестов и характеризация ввода

**Класс исполнителя:** `ds-low-agentic`.

**Files:**
- Create: `src/app/app_behavior_tests/app_behavior_host_cases.rs` (файл растёт в T3–T6 — исключение из правила 200 строк на момент создания)
- Modify: `src/app/app_behavior_tests.rs` — строка `include!("app_behavior_tests/app_behavior_host_cases.rs");` рядом с тремя существующими `include!` (без неё тесты не соберутся, а фильтр пройдёт пустым)
- Create (артефакт, не в git): `<workspace>/baseline-failures.txt`, где `<workspace>` — каталог SDD плана (`.superpowers/sdd/2026-09-25-headless-mode/`)

**Interfaces:**
- Consumes: `crate::render_view::…::reviewer_stage2_integration::{fixture, review_v3_root_frame}` (`src/render_view/markdown_scroll_transition_review_tests.rs:147,678`), `reviewer_markdown_read_mouse_input` (`src/app/mouse/input.rs:469`).
- Produces: тесты с префиксом `host_characterization_`; файл `baseline-failures.txt` (по имени теста на строку, пустой — если всё зелёное), который читают T2–T8 и финальная проверка.

- [ ] **Step 1: baseline.** Один полный прогон `make test > /tmp/rriter-task1-baseline.log 2>&1` на текущем master (минуты). Имена упавших тестов (`test … FAILED` / `panicked`) — в `baseline-failures.txt`. Известный кандидат: `reviewer_reader_v1_all_block_selection_seating_uses_real_vertices`.
- [ ] **Step 2: характеризационные тесты** на offscreen-фикстуре stage2 (`#[cfg(all(test, target_os = "linux"))]`), состояние — документ из нескольких сотен строк, один кадр `review_v3_root_frame`:
  - `host_characterization_click_opens_settings`: press+release левой кнопкой по элементу, который открывает настройки (найти `UiId` по `ui_handlers.rs`) → `app.show_settings == true`.
  Известно из pre-flight: `handle_main_cursor_moved` (`mouse/cursor.rs:~404`) и `handle_main_mouse_wheel` (`mouse/wheel.rs:~117`) делают `unwrap()` окна, а stage2-фикстура оставляет `window: None` — поэтому `host_characterization_cursor_moved_sets_hover` и `host_characterization_wheel_scrolls_editor` в Task 1 НЕ пишутся, их пишет Task 3 на `offscreen_test_app` (до HostLoop/KeyInput/выноса кадра, которые они и страхуют). В Task 1 — только клик через `reviewer_markdown_read_mouse_input` (передаёт `event_loop = None`); если и он паникует на окне — Task 1 сводится к baseline, тест клика тоже переходит в Task 3; в отчёте назвать обработчик и строку.
- [ ] **Step 3:** `make test TEST_FILTER='host_characterization_'` → зелёные на текущем коде.
- [ ] **Step 4: commit** `headless: characterization tests for input handlers on offscreen fixture;`

**Отчёт:** список тестов, содержимое `baseline-failures.txt`, какие обработчики нельзя вызвать без окна.

---

### Task 2: `WindowHost` — тип поля `App.window`

**Класс исполнителя:** `ds-low-agentic`. Спека: 3.1.

**Files:**
- Create: `src/platform/window_host.rs` (`WindowHost`, `HeadlessWindow`, юнит-тесты)
- Modify: `src/platform.rs` (`pub mod window_host; pub use window_host::{HeadlessWindow, WindowHost};`), `src/app/app_state.rs` (тип поля `window: Option<Arc<WindowHost>>`, импорт), `src/app/events/window_runtime.rs` (создание `WindowHost::Native`, `native()` для GL-surface и `window_handle`), `src/app/app_window_external_methods.rs` (`update_window_title(&WindowHost, …)`), `src/app/terminal_process.rs`, `src/app/tool_installer.rs` (тип `Option<Arc<WindowHost>>` в потоках), `src/app/automation.rs` (`request_inner_size`), `src/app/events.rs` (`id()`), тестовая фикстура `test_app()` (тип поля; значение `None`), `PROJECT_GUIDE.md` §4
- Test: `src/platform/window_host.rs` (`#[cfg(test)] mod tests`)

**Interfaces:**
- Produces: `WindowHost`, `HeadlessWindow` — ровно как в таблице «Общие интерфейсы». `HeadlessWindow::request_inner_size` через `WindowHost` меняет `size` и возвращает `Some(new)` (размер из `Size::to_physical(scale_factor)`). `set_cursor`: `Cursor::Icon(i)` → запомнить `i`, custom-курсор → `CursorIcon::Default`. `set_title` запоминает строку. `is_maximized` читает то, что поставил `set_maximized`. `focus_window` — no-op. Все Mutex-доступы — через `lock()` с обработкой отравления (`unwrap_or_else(PoisonError::into_inner)`), без паники.

- [ ] **Step 1: тесты `window_host_*`** в `window_host.rs`: `inner_size`/`scale_factor` возвращают заданное в `new`; `request_redraw` → `take_redraw_request()` = `true`, повторный — `false`; `request_inner_size(PhysicalSize::new(800,600))` → `Some(800x600)` и `inner_size()` = 800x600; `set_size`/`set_scale_factor` меняют значения; `set_cursor(CursorIcon::Text)` → `cursor_icon() == Text`; `set_title("x")` → `title() == "x"`; `id()` не паникует. Для `Native` тестов нет (окна в тестах нет).
- [ ] **Step 2:** реализация, перевод call-site'ов. Цель — чтобы 473 обращения `self.window…` компилировались без правок: имена и сигнатуры методов совпадают с winit. Если какой-то метод winit не нашёлся в списке спеки, а код его зовёт — добавить делегирование (Native) и no-op/поле (Headless), назвать в отчёте.
- [ ] **Step 3:** `make test TEST_FILTER='window_host_'`, затем `make test TEST_FILTER='host_characterization_'` и `TEST_FILTER='reviewer_stage2'` — зелёные, ассерты не менялись.
- [ ] **Step 4: commit** `headless: WindowHost wrapper for App.window (native | headless);`

---

### Task 3: `platform::offscreen_gl` — перенос `OffscreenContext` в production

**Класс исполнителя:** `ds-low-agentic`. Спека: 3.4.

**Files:**
- Create: `src/platform/offscreen_gl.rs` (`#[cfg(target_os = "linux")]`)
- Modify: `src/platform.rs` (`#[cfg(target_os = "linux")] pub mod offscreen_gl;`), `src/render_view/markdown_scroll_transition_review_tests.rs` (удалить локальную копию `OffscreenContext` строк 10–145, `fixture` использует `crate::platform::offscreen_gl::OffscreenContext::new(1000, 800).expect(..)`), `src/app/app_behavior_tests/app_behavior_host_cases.rs` (характеризационные тесты курсора и колеса, отложенные из Task 1 — см. Step 1b), `PROJECT_GUIDE.md` §4

**Interfaces:**
- Consumes: `WindowHost`, `HeadlessWindow` (T2).
- Produces: `OffscreenContext`, `GlStrings`, `test_support::offscreen_test_app` — как в таблице.

Поведение `OffscreenContext::new(w, h)`:
1. `dlopen("libEGL.so.1")`; нет → `Err("dlopen: …")`.
2. `eglQueryString(EGL_NO_DISPLAY, EGL_EXTENSIONS)` должна содержать `EGL_MESA_platform_surfaceless`; нет → `Err` с полным списком client extensions.
3. Далее как в текущей копии: `eglGetPlatformDisplay(0x31DD)` → `Initialize` → `ChooseConfig` (pbuffer, RGBA8) → `CreatePbufferSurface(w×h)` → `CreateContext` (OpenGL 3.3 Core) → `MakeCurrent`. Каждая ошибка — `Err(format!("{stage}: eglGetError=0x{code:04X}"))`, где `stage` ∈ `dlopen | GetPlatformDisplay | Initialize | ChooseConfig | CreatePbuffer | CreateContext | MakeCurrent`; уже созданные EGL-объекты освобождаются.
4. После `MakeCurrent` запомнить `GL_RENDERER`/`GL_VERSION`/`GL_VENDOR` в `GlStrings`.
- `resize(w, h)`: создать новый pbuffer под тем же config/context; успех → `MakeCurrent` на нём, старый уничтожить; ошибка → `Err`, старый pbuffer остаётся текущим, размер прежний.
- `requested_context()` → `"EGL surfaceless OpenGL 3.3 Core"` (строка идёт в `Renderer::new` как 4-й аргумент).
- `Drop` — как в текущей копии.
- `test_support::offscreen_test_app(w, h, scale)` — `#[cfg(test)] pub(crate) mod test_support` в конце файла: контекст `w×h`, `crate::app::reviewer_stage2_test_app()`, `Renderer::new(ctx.glow(), scale, app.theme.clone(), ctx.requested_context())`, `renderer.resize(w, h)`, `app.renderer = Some(..)`, `app.window = Some(Arc::new(WindowHost::Headless(HeadlessWindow::new(PhysicalSize::new(w,h), scale as f64))))`.

- [ ] **Step 1: тесты** в `offscreen_gl.rs` (`#[cfg(test)]`): `offscreen_gl_creates_context_and_reports_gl_strings` (`new(64,48)` → `Ok`, `gl_strings().version` не пуст); `offscreen_gl_resize_keeps_context` (`resize(128,96)` → `Ok`, `size() == (128,96)`, `glow()` рабочий — `gl.get_parameter_string(VERSION)` не пуст).
- [ ] **Step 1b: характеризация, отложенная из Task 1,** на `offscreen_test_app(1280, 800, 1.0)` с документом из нескольких сотен строк и `show_welcome = false`, кадр — stage2 `review_v3_root_frame`: `host_characterization_cursor_moved_sets_hover` (курсор в центр прямоугольника зарегистрированного `IconButton`/`Button` → после второго кадра `ui_registry.hovered == Some(<id>)` или эквивалентное поле hover по `cursor.rs`); `host_characterization_wheel_scrolls_editor` (`LineDelta(0.0, -3.0)` над текстом → целевой `scroll_y` вырос); клик по элементу настроек → `show_settings` (если Task 1 его не написал). Эти тесты — страховка для Task 4–6: там меняются только вызовы, ассерты — нет.
- [ ] **Step 2:** перенос + доработки выше; stage2-фикстура на новом модуле.
- [ ] **Step 3:** `make test TEST_FILTER='offscreen_gl_'`, `TEST_FILTER='reviewer_stage2'`, `TEST_FILTER='markdown_scroll_'`, `TEST_FILTER='host_characterization_'` — зелёные, ассерты stage2 не менялись.
- [ ] **Step 4: commit** `headless: move surfaceless EGL OffscreenContext into platform::offscreen_gl;`

---

### Task 4: `HostLoop` вместо `&ActiveEventLoop` в логике приложения

**Класс исполнителя:** `ds-low-agentic`. Спека: 3.3 (кроме абзаца про `headless_dialog_open` — это Task 12).

**Files:**
- Create: `src/app/events/host_loop.rs` (≈ 80 строк с тестами — исключение из правила 200 строк: отдельный тип-адаптер, класть его в `events.rs`/`about.rs` нельзя — оба за лимитом)
- Modify: `src/app/events.rs` (диспетчер `WindowEvent`/`about_to_wait` строит `HostLoop::Native(event_loop)`), `src/app/events/about.rs`, `src/app/events/window_runtime.rs` (`save_state_and_exit`; `bootstrap`/`resume` остаются на `&ActiveEventLoop`), `src/app/app_window_external_methods.rs` (`show_action_dialog(host, action)`), `src/app/automation.rs` (`AutomationController::tick/run_step`, `advance_automation`), `src/app/mouse/input.rs` (`handle_main_mouse_input(_inner)`; удалить `cfg(test)`-вариант `Option<&ActiveEventLoop>`), `src/app/keyboard/main_keys.rs`, `src/app/keyboard/editor_keys.rs` (`handle_main_keyboard_input(_inner)`, `handle_editor_keyboard_input`), тесты, вызывавшие эти функции (`reviewer_markdown_read_mouse_input` и др.), `src/app/app_behavior_tests/app_behavior_host_cases.rs`, `PROJECT_GUIDE.md` §4

**Interfaces:**
- Consumes: `offscreen_test_app` (T3).
- Produces: `HostLoop`, `HeadlessLoopState` — как в таблице. `exit()`: Native → `event_loop.exit()`, Headless → `exit_requested.store(true)`. `set_control_flow(f)`: Native → делегирование, Headless → `last_control_flow.set(f)`. `native()`: Native → `Some`, Headless → `None`.
- `show_action_dialog(&mut self, host: &HostLoop, action)`: `if let Some(event_loop) = host.native()` — текущий код без изменений; иначе — `return` (до Task 12 headless-диалога нет; `pending_action` уже в `App`).

- [ ] **Step 1: тесты** `host_loop_headless_exit_sets_flag`, `host_loop_headless_records_control_flow` (`WaitUntil(t)` читается обратно), `host_loop_headless_has_no_native` — в `host_loop.rs`.
- [ ] **Step 2:** перевод сигнатур. Все `exit()` (`window_runtime.rs:311,371`, `about.rs:132,216`, `editor_keys.rs:435`) и все `set_control_flow` (`about.rs:145,1648,1650,1654`, `events.rs:611,623,1507`) в функциях, переводимых на `HostLoop`, идут через него; те, что остаются в `ApplicationHandler`-методах и оконной ветке `RedrawRequested` — могут звать `event_loop` напрямую. Тесты вместо `None` передают `&HostLoop::headless(&HeadlessLoopState::default())`.
- [ ] **Step 3:** характеризационные тесты Task 1 переписать только в месте вызова (новый параметр), ассерты не трогать. Если в Task 1/3 были отложенные тесты — дописать на `offscreen_test_app`.
- [ ] **Step 4:** `make test TEST_FILTER='host_loop_'`, `TEST_FILTER='host_characterization_'`, `TEST_FILTER='reviewer_'`, `TEST_FILTER='automation'` — зелёные.
- [ ] **Step 5: commit** `headless: HostLoop abstraction over ActiveEventLoop for exit/control flow;`

---

### Task 5: `KeyInput` и `parse_combo`

**Класс исполнителя:** `ds-low-agentic`. Спека: 3.2.

**Files:**
- Create: `src/app/keyboard/key_input.rs` (`KeyInput`, `From<&KeyEvent>`, `parse_combo`, `released`, обёртка `App::handle_main_keyboard_input(&mut self, host: &HostLoop, event: KeyEvent)` → `self.handle_main_key_input(host, KeyInput::from(&event))`, тесты)
- Modify: `src/app/keyboard.rs` (`mod key_input;` + `pub(crate) use`; плюс его собственные обработчики, принимающие `KeyEvent`/`&KeyEvent` и читающие `logical_key` (`keyboard.rs:~484,540`, вызываются из `main_keys.rs:~723`) — переводятся на `&KeyInput` так же, как остальные файлы), `src/app/keyboard/main_keys.rs` (тело бывшего `handle_main_keyboard_input` → `handle_main_key_input(host, input: KeyInput)`), `src/app/keyboard/editor_keys.rs`, `src/app/file_tree_dialog.rs`, `src/app/database/database_table_edit_methods.rs`, `src/app/database/database_app_methods.rs`, `src/app/api_client/api_client_app_request_methods.rs` (параметры `KeyEvent`/`&KeyEvent` → `KeyInput`/`&KeyInput`; `event.logical_key.to_text()` → `event.logical_text.as_deref()`; остальные поля один в один), `PROJECT_GUIDE.md` §4

**Interfaces:**
- Consumes: `HostLoop` (T4), `offscreen_test_app` (T3).
- Produces: `KeyInput`, `parse_combo`, `released()` (копия с `state = Released`, `repeat = false`), `App::handle_main_key_input`.

`parse_combo` (спека 3.2): токены через `+`, регистр не важен, пробелы по краям обрезаются. Последний токен — клавиша, предыдущие — модификаторы (`ctrl`/`control`, `shift`, `alt`, `super`/`meta`). Клавиши:
- `a`–`z` → `KeyCode::KeyA…`, `text`/`logical_text` = буква (с `shift` — заглавная);
- `0`–`9` → `Digit0…`, текст — цифра (с `shift` — тот же символ; раскладку не эмулировать);
- `enter` (`text = "\r"`), `tab` (`"\t"`), `escape`/`esc`, `backspace`, `delete`, `space` (`" "`), `up/down/left/right`, `home/end/pageup/pagedown`, `insert`, `f1`–`f12` → соответствующие `KeyCode`, текст `None` кроме указанных; `logical_text` для именованных клавиш — `None`, как у winit `Key::Named(..).to_text()` (проверить по winit: `to_text()` у `Named` для Enter/Tab/Space/Escape/Backspace/Delete — сверить с исходником winit в `~/.local/share/cargo/registry/src/*/winit-0.30.13/src/keyboard.rs` и повторить);
- ASCII-пунктуация `. , / - = ; ' [ ] \ `` ` `` → `Period, Comma, Slash, Minus, Equal, Semicolon, Quote, BracketLeft, BracketRight, Backslash, Backquote`, текст — сам символ.
- Когда зажат `ctrl`, `alt` или `super` — `text = None`, `logical_text` — символ клавиши: шорткаты читают `physical_key`, а `text = Some` при Ctrl рискует вставкой символа. Если при переводе файлов видно, что какой-то шорткат читает `text`/`logical_text` при Ctrl — назвать в отчёте.
- Ошибки: пустая строка → `Err("empty key combo")`; пустой токен (`ctrl+`, `+`, `ctrl++a`) → `Err("empty key token")`; неизвестный → `Err(format!("unknown key token '{t}'"))`; только модификаторы (`ctrl`), повтор модификатора (`ctrl+ctrl+a`) → `Err`.

- [ ] **Step 1: тесты `key_input_*`:** таблица `parse_combo` — по одному кейсу на класс (буква, `shift+a`→`"A"`, цифра, `enter`, `esc`, `f5`, `pageup`, `.`, `` ` ``, `ctrl+shift+p` → `ModifiersState::CONTROL | SHIFT` + `KeyP`, `SUPER+Q` — регистр); негатив: `""`, `"ctrl+"`, `"+"`, `"ctrl+foo"`, `"ctrl+ctrl"`, `"ctrl+ctrl+a"`, `"ctrl"`. `released()` меняет только `state`/`repeat`.
- [ ] **Step 2:** тест `key_input_enter_inserts_newline` на `offscreen_test_app` (редактор с текстом `"ab"`, курсор 1, `show_welcome = false`): `handle_main_key_input(&HostLoop::headless(..), parse_combo("enter").0)` → текст `"a\nb"` (перевод строки в формате редактора).
- [ ] **Step 3:** перевод семи файлов; `handle_main_keyboard_input` в `events.rs` вызывается как раньше.
- [ ] **Step 4:** `make test TEST_FILTER='key_input_'`, затем `TEST_FILTER='keyboard'`, `TEST_FILTER='database'`, `TEST_FILTER='api_client'`, `TEST_FILTER='file_tree'`, `TEST_FILTER='host_characterization_'` — зелёные.
- [ ] **Step 5: commit** `headless: KeyInput (constructible KeyEvent subset) and parse_combo;`

---

### Task 6: вынос кадра — `render_main_frame` / `finish_main_frame`

**Класс исполнителя:** `ds-low-agentic`. Спека: 3.5, первые три абзаца.

**Files:**
- Create: `src/app/events/main_frame.rs`
- Modify: `src/app/events.rs` (ветка `WindowEvent::RedrawRequested` и объявление `mod main_frame;` — `src/app/events/mod.rs` нет, подмодули объявлены в `events.rs`), `src/app/app_behavior_tests/app_behavior_host_cases.rs`, `PROJECT_GUIDE.md` §4

**Interfaces:**
- Consumes: `offscreen_test_app` (T3).
- Produces: `App::render_main_frame(&mut self) -> FrameOutcome` — от строки после guard'ов `render_suspended`/`!is_ready` (сейчас `let (autocomplete_frame_start, autocomplete_prev_frame) = …`, `events.rs:~644`) до последнего GL-вызова перед `let present_start = Instant::now();`; `App::finish_main_frame(&mut self, outcome: FrameOutcome)` — всё после записи swap-телеметрии (сейчас с `if let (Some(frame_start), Some(swap_start), Some(metrics)) = …` до конца ветки). `FrameOutcome` — `pub(crate)` структура с приватными полями ровно для локальных, объявленных в первой части и читаемых во второй (сейчас это `autocomplete_prev_frame`, `autocomplete_frame_start`, `autocomplete_swap_start`, `autocomplete_frame_metrics` — последние два считаются в `events.rs:~1484-1485`, до present, поэтому вся эта группа строк уходит в `render_main_frame`).

Оконная ветка после выноса: guards → `let outcome = self.render_main_frame();` → present/`finish_present`/`record_presented_frame`/`record_swap_telemetry` (как сейчас) → `self.finish_main_frame(outcome)`. Операции не переставляются; diff — вынос блоков, структура, три вызова. Внутри вынесенных функций `request_redraw()` остаётся как есть.

- [ ] **Step 1: тест** `host_frame_render_main_frame_registers_same_ui` на `offscreen_test_app(1280, 800, 1.0)` с `show_welcome = false` и коротким документом: `review_v3_root_frame`-эквивалент уже есть в stage2 (ручной `Renderer::draw`); тест вызывает `app.render_main_frame()` и проверяет, что `ui_registry.elements` не пуст и содержит id вкладок/статус-бара, которые регистрирует ручной вызов на том же состоянии (сравнить множества `format!("{:?}", id)` для элементов, общих для обоих путей — ручной вызов передаёт часть аргументов `None`/пустыми, поэтому проверка — «ручное ⊆ render_main_frame»). Второй кадр не паникует; `finish_main_frame(outcome)` не паникует.
- [ ] **Step 2:** вынос. `events.rs` худеет примерно на 850–900 строк.
- [ ] **Step 3:** `make test TEST_FILTER='host_frame_'`, `TEST_FILTER='reviewer_stage2'`, `TEST_FILTER='host_characterization_'` — зелёные.
- [ ] **Step 4: commit** `headless: split RedrawRequested into render_main_frame/present/finish_main_frame;`

**Review Focus этой задачи:** ревьюер сравнивает вынесенный код с удалённым из `events.rs` построчно (`git diff --color-moved`), любая перестановка или изменение выражения — дефект.

---

### Task 7: `App::new_from_config` и App-уровневые отличия headless

**Класс исполнителя:** `ds-low-agentic`. Спека: 3.5 (абзац про `new_from_config`), 4.2 п.4 (dart, clipboard, мигание).

**Files:**
- Create: `src/app/app_bootstrap.rs` (`AppInitOptions`, `App::new_from_config`, FAQ-текст, тесты)
- Modify: `src/main.rs` (вызов `App::new_from_config(config, options)` вместо литерала и кода после него; `load_config`, `configure_tool_paths`, `TELEMETRY_ENABLED.store`, построение `EventLoop` остаются в `main.rs`), `src/app/app_state.rs` (поле `pub headless_mode: bool`), `src/app.rs` (`mod app_bootstrap;`), фикстура `test_app()` в `src/app/app_behavior_tests/app_behavior_autocomplete_base_cases.rs:~64` (поле `headless_mode: false`), `src/app/events/about.rs` (гейт мигания), `PROJECT_GUIDE.md` §4. Код `main.rs` до литерала (чтение файла-аргумента, обновление и сохранение recent files) остаётся в `main.rs`.

**Interfaces:**
- Produces: `App::new_from_config(config: Config, options: AppInitOptions) -> App`; `AppInitOptions` — поля: ровно те локальные `main.rs`, которые читают литерал `App { … }` и код после него до `run_app` (исполнитель перечисляет: путь файла-аргумента, `has_file_arg`, `run_ide_on_startup`, `automation_options`, `scroll_bench_idx`/`scroll_bench_seconds`, `pgo_train`, …), плюс `pub headless: bool`; `AppInitOptions::headless() -> Self` — всё `None`/`false`, `headless: true`; `App.headless_mode`.
- Headless-отличия в `new_from_config` при `options.headless`: `headless_mode = true`, `clipboard = None`, `refresh_dart_tool_state()` не вызывается. Больше ничего не отличается.
- Мигание (`about.rs:~1612-1622`): при `app.headless_mode` курсор не мигает — `last_blink_state` остаётся видимым, а `idle_blink_enabled` (`about.rs:~1622`, `= app.is_focused && app.dialog_window.is_none()`) получает `&& !app.headless_mode`; тогда `compute_about_wait_plan` (`about_helpers.rs:~387`) не ставит `WaitUntil` ради мигания — иначе `settle` не сходится.

- [ ] **Step 1: тест `app_bootstrap_headless_init`:** `new_from_config(Config::default(), AppInitOptions::headless())` (`Config::default()` есть, `main.rs:~61`) → `headless_mode`, `clipboard.is_none()`, `show_welcome == true`, `base_title == "Добро пожаловать"`. Вариант `headless: false` в тестах не строить: он зовёт `refresh_dart_tool_state()` → фоновый `dart --version`.
- [ ] **Step 2:** тест `app_bootstrap_headless_cursor_does_not_blink`: на `test_app()` с `headless_mode = true`, `is_focused = true` — `compute_about_wait_plan` (или обёртка, которую строит `about.rs` вокруг него; если вход собирается внутри `about_to_wait` — вынести сборку `idle_blink_enabled` в маленькую функцию, рост `about.rs` ≤ 10 строк) даёт план без `WaitUntil` ради мигания, а `last_blink_state` не переключается.
- [ ] **Step 3:** вынос из `main.rs` (порядок операций сохраняется; создание `faq_editor` переезжает внутрь `new_from_config` — чистая операция).
- [ ] **Step 4:** `make test TEST_FILTER='app_bootstrap_'`, `TEST_FILTER='host_'` — зелёные; `main.rs` компилируется (тесты собирают бинарный крейт).
- [ ] **Step 5: commit** `headless: App::new_from_config with headless init options (no clipboard, no dart probe, no blink);`

---

### Task 8: политика headless и корень профиля в `platform`

**Класс исполнителя:** `ds-low-agentic` (security-гейты — код ниже полный, но точки входа мутаций дерева/Git в Step 3b исполнитель находит сам). Спека: 4.2 п.3–4, 4.8 абзац про печать телеметрии; расширение гейта на дерево и Git — решение пользователя 2026-09-25.

**Files:**
- Modify: `src/platform.rs` (политика, внешние запросы, гейты в `pick_file`/`pick_files`/`pick_folder`/`save_file*` (`~:1233-1270`), `open_url` (`~:1311`), `reveal_path` (`~:1279`)), `src/platform/integration.rs` (override корня, `app_paths()` `~:555`), `src/app/app_window_external_methods.rs` (гейт в `write_current_text_to_path` `~:456`), `src/render_view/root_frame_overlay_helpers.rs` (гейт периодической печати `~:721`)
- Test: `src/platform/tests.rs`

**Interfaces:**
- Produces: всё ниже; потребители — T10 (`set_app_root_override`, `app_paths_for_root`), T11 (`set_headless`), T12 (`take_external_request`, `headless_writes_allowed`).

Код в `src/platform.rs` (рядом с остальными `OnceLock`-глобалами, `use std::sync::{Mutex, OnceLock}` — по месту):

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HeadlessPolicy {
    pub allow_writes: bool,
}

static HEADLESS_POLICY: OnceLock<HeadlessPolicy> = OnceLock::new();

/// Set once by `headless::run` before any thread starts. Tests never call it:
/// the policy is process-wide and would block file writes in every later test.
pub fn set_headless(policy: HeadlessPolicy) {
    let _ = HEADLESS_POLICY.set(policy);
}

pub fn headless_policy() -> Option<HeadlessPolicy> {
    HEADLESS_POLICY.get().copied()
}

pub fn is_headless() -> bool {
    headless_policy().is_some()
}

pub fn headless_writes_allowed() -> bool {
    editor_writes_allowed(headless_policy())
}

/// Windowed mode (`None`) always writes; headless writes only with `--allow-writes`.
pub(crate) fn editor_writes_allowed(policy: Option<HeadlessPolicy>) -> bool {
    policy.is_none_or(|policy| policy.allow_writes)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExternalRequest {
    PickFile,
    PickFiles,
    PickFolder,
    SaveFile,
    OpenUrl(String),
    RevealPath(PathBuf),
}

static LAST_EXTERNAL_REQUEST: Mutex<Option<ExternalRequest>> = Mutex::new(None);

pub fn note_external_request(request: ExternalRequest) {
    let mut last = LAST_EXTERNAL_REQUEST
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    *last = Some(request);
}

/// Returns the last intercepted request and clears it (read by `dump`).
pub fn take_external_request() -> Option<ExternalRequest> {
    LAST_EXTERNAL_REQUEST
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .take()
}

/// `true` when the caller must not touch the desktop: headless records the
/// request instead of opening a picker, browser, or file manager.
pub(crate) fn intercept_external(policy: Option<HeadlessPolicy>, request: ExternalRequest) -> bool {
    if policy.is_none() {
        return false;
    }
    note_external_request(request);
    true
}
```

Гейты — первой строкой каждой функции:

```rust
// pick_file
if intercept_external(headless_policy(), ExternalRequest::PickFile) {
    return None;
}
// pick_file_with_filter → ExternalRequest::PickFile, return None
// pick_files → ExternalRequest::PickFiles, return Vec::new()
// pick_folder → ExternalRequest::PickFolder, return None
// каждый save_file* → ExternalRequest::SaveFile, return None
// open_url(url) -> io::Result<()>: ExternalRequest::OpenUrl(url.to_string()), return Ok(())
// reveal_path(path) -> io::Result<Child>: ExternalRequest::RevealPath(path.to_path_buf()),
//     return Err(io::Error::new(io::ErrorKind::Unsupported, "disabled in headless mode"))
//     — «успех без процесса» в этом типе не выразить; вызывающие уже обрабатывают Err
//     (проверить, что ни один не паникует на Err; назвать в отчёте, как каждый его показывает)
```

Если `pick_*` на Linux запускаются в потоке и результат приходит через mpsc (`app_window_external_methods.rs:~359,390,409`) — гейт внутри `platform::pick_*` возвращает «ничего не выбрано» в тот же канал, поток завершается; ничего больше не меняется.

Код в `src/platform/integration.rs`:

```rust
static APP_ROOT_OVERRIDE: OnceLock<PathBuf> = OnceLock::new();

/// Headless profile root: all four app directories live under it, env is ignored.
/// First call wins; set before any thread starts.
pub fn set_app_root_override(root: PathBuf) -> Result<(), PathBuf> {
    APP_ROOT_OVERRIDE.set(root)
}

pub fn app_root_override() -> Option<&'static Path> {
    APP_ROOT_OVERRIDE.get().map(PathBuf::as_path)
}

pub(crate) fn app_paths_for_root(root: &Path) -> AppPaths {
    AppPaths {
        config: root.join("config"),
        data: root.join("data"),
        cache: root.join("cache"),
        state: root.join("state"),
    }
}

pub fn app_paths() -> AppPaths {
    if let Some(root) = app_root_override() {
        return app_paths_for_root(root);
    }
    app_paths_with(CURRENT_PLATFORM, |name| std::env::var_os(name))
}
```

`set_app_root_override`, `app_root_override`, `app_paths_for_root` должны быть доступны как `crate::platform::…` (так же, как сейчас `crate::platform::app_paths`).

Гейт записи — первые строки `App::write_current_text_to_path`:

```rust
if !crate::platform::editor_writes_allowed(crate::platform::headless_policy()) {
    self.show_readonly_notice();
    return false;
}
```

(`show_readonly_notice` определён в `git_diff.rs:~1434`; если сигнатура другая — тот же notice, что показывается при ошибке записи в этой функции.)

Эскалация: в `platform.rs` рядом с `editor_writes_allowed`:

```rust
/// pkexec/UAC prompts would pop over the user's fullscreen app: never in headless.
pub(crate) fn elevation_allowed(policy: Option<HeadlessPolicy>) -> bool {
    policy.is_none()
}
```

и в `write_current_text_to_path` ветка, которая при ошибке прав зовёт `write_text_file_elevated`, сначала проверяет `crate::platform::elevation_allowed(crate::platform::headless_policy())`; `false` → путь ошибки записи (readonly-notice, `return false`), как если бы эскалация отказала.

Печать телеметрии: накопление и 10-секундный сброс живут в `finalize_root_frame_telemetry` (`root_frame_overlay_helpers.rs:~671`), печать — `~:721-790`. Гейт `if !crate::platform::is_headless() { … }` оборачивает **только** блок `println!`; ранний `return` из функции запрещён — он остановит накопление, которое читает `take_frame_telemetry` (T14).

- [ ] **Step 1: тесты** в `src/platform/tests.rs`: `headless_policy_editor_writes_allowed` (`None` → true, `Some{allow_writes:false}` → false, `Some{true}` → true); `headless_policy_elevation_allowed` (`None` → true, `Some{true}` → false, `Some{false}` → false); `headless_policy_intercept_external` (`None` → false и `take_external_request()` = `None` после предварительного `take`; `Some` → true, `take` возвращает запрос, повторный `take` → `None`); `headless_app_paths_for_root` (четыре подкаталога корня). `set_headless`/`set_app_root_override` в тестах не вызывать.
- [ ] **Step 2:** код выше + гейты.
- [ ] **Step 3: аудит путей состояния (только отчёт):** найти все места, где production-код строит путь состояния RRiter мимо `app_paths()`/`config_dir()/data_dir()/cache_dir()/state_dir()` (прямое чтение `HOME`, `XDG_*`, `dirs::`, `home_dir`). По каждому — `file:line`, пишет ли. Раздел «Аудит путей» в отчёте.
- [ ] **Step 3b: гейт мутаций файлового дерева и Git (решение пользователя 2026-09-25: вариант 1).** Без `--allow-writes` в headless не выполняются: файловое дерево — create file/folder, rename, delete (в т.ч. в Trash), move/drag-move, paste/duplicate, если есть; Git — любые команды, меняющие репозиторий или рабочее дерево (stage/unstage, commit, checkout/switch, discard/restore, stash, reset, pull/merge/rebase, push не бывает без пользователя — тоже закрыть, если есть кнопка). Реализация — один хелпер в `app_window_external_methods.rs`:

```rust
/// Headless without --allow-writes: disk-mutating UI actions are refused with the readonly notice.
pub(crate) fn headless_write_blocked(&mut self) -> bool {
    if crate::platform::editor_writes_allowed(crate::platform::headless_policy()) {
        return false;
    }
    self.show_readonly_notice();
    true
}
```

и `if self.headless_write_blocked() { return; }` первой строкой каждой точки входа такого действия — там, где действие подтверждено пользователем и сейчас начнётся I/O или запуск git-процесса (не при открытии диалога переименования, а при его применении). Для Git — предпочтительно одна точка в `src/app/git_panel/git_process.rs`, если все мутирующие команды проходят через одну функцию запуска, которая знает, мутирующая ли команда; иначе — в каждом обработчике. Read-only git (status, diff, log, graph) не трогать. Список закрытых точек (`file:line`, действие) — раздел «Гейт мутаций» в отчёте; всё, что пишет на диск вне профиля, но не закрыто (БД, API-клиент, installer, терминал), — там же с причиной.
- Файлы этого шага дополнительно: обработчики файлового дерева (`src/app/file_tree.rs`, `src/app/file_tree_dialog.rs` и соседние), `src/app/git_panel/*` — исполнитель находит сам. Файлы > 1600 строк растут не больше чем на строку на точку входа.
- [ ] **Step 4:** `make test TEST_FILTER='headless_policy_'`, `TEST_FILTER='headless_app_paths'`, `TEST_FILTER='platform'` — зелёные.
- [ ] **Step 5: commit** `headless: platform policy (external request interception, editor write gate) and app root override;`

---

### Task 9: протокол команд — парсер

**Класс исполнителя:** `ds-low-agentic`. Спека: 4.4. Зависит от T5 (`KeyInput::parse_combo`) — исполняется после T5; с T7 делит `main.rs` (T9 добавляет только объявление модуля в начале файла, T7 выносит литерал `App` — места не пересекаются).

**Files:**
- Create: `src/headless/protocol.rs` (парсер + тесты), `src/headless/mod.rs` (пока только `pub(crate) mod protocol;` под `#[cfg(target_os = "linux")]`; T11 дополняет — исключение из правила 200 строк на время между задачами)
- Modify: `src/main.rs` (`#[cfg(target_os = "linux")] mod headless;`), `PROJECT_GUIDE.md` §4

**Interfaces:**
- Consumes: `KeyInput::parse_combo` (T5).
- Produces:

```rust
pub(crate) enum MouseButtonArg { Left, Right, Middle }
pub(crate) enum ClickPhase { Both, Down, Up }
pub(crate) enum WheelUnit { Lines, Px }
pub(crate) enum DialogAnswer { Save, Discard, Cancel }
pub(crate) enum BenchAction { None, Wheel { dx: f64, dy: f64 }, Key { input: KeyInput, mods: ModifiersState }, Type(String) }
pub(crate) enum Command {
    Open(PathBuf), Workspace(PathBuf),
    Resize { w: u32, h: u32 }, Scale(f64),
    MouseMove { x: f64, y: f64 },
    Click { button: MouseButtonArg, phase: ClickPhase }, DblClick { button: MouseButtonArg },
    Wheel { dx: f64, dy: f64, unit: WheelUnit },
    Key { input: KeyInput, mods: ModifiersState, combo: String },
    Type(String),
    Settle { ms: u64 }, Wait { ms: u64 },
    Screenshot(PathBuf), Dump(Option<PathBuf>),
    Dialog(DialogAnswer),
    Bench { frames: u32, csv: Option<PathBuf>, action: BenchAction },
    Record { frames: u32, dir: PathBuf, action: BenchAction },
    Info, Quit,
}
pub(crate) fn parse_line(line: &[u8]) -> Result<Option<Command>, String>;
pub(crate) enum Response { Ok(Option<String>), Err(String) }
impl Response { pub(crate) fn line(&self) -> String; } // "ok" | "ok <p>" | "err <r>"; '\n'/'\r' в payload/причине → ' '
pub(crate) const SIZE_MIN: (u32, u32) = (320, 200);
pub(crate) const SIZE_MAX: (u32, u32) = (8192, 8192);
pub(crate) fn parse_size(s: &str) -> Result<(u32, u32), String>; // "WxH" с границами; общий с CLI (T10)
```

Грамматика (спека 4.4 — таблица и «Правила»):
- Байты → `std::str::from_utf8`, ошибка → `Err("invalid utf-8")`. Хвостовые `\r`/`\n` срезаются. Пустая / только пробелы / первый непробельный `#` → `Ok(None)`.
- Имя команды — первый токен; неизвестное → `Err(format!("unknown command '{name}'"))` (для строки 1 МБ имя в сообщении обрезать до 64 символов).
- Остаток строки (`open`, `workspace`, `screenshot`, `dump`, `type`) — от первого непробельного символа после имени до конца строки, хвостовые пробелы сохраняются только у `type`. Пустой остаток: `open`/`workspace`/`screenshot` → `Err("missing path")`; `dump` → `Dump(None)`; `type` → `Type(String::new())`.
- `type`: escape `\n`, `\t`, `\\`; остальные `\x` — остаются как есть (обратный слэш и символ); одиночный `\` в конце — как есть.
- Числа: `f64` через `parse`, `NaN`/`inf`/`-inf` → `Err("non-finite number")`; целые (`frames`, `ms`) — `u32`/`u64`, отрицательные/дробные → `Err`.
- Границы: `resize` через `parse_size` (вне 320x200…8192x8192 → `Err`); `scale` 0.5…4.0; `settle`/`wait` ≤ 60000 (`settle` без аргумента → 500, `wait` без аргумента → `Err`); `bench` 1…100000 кадров; `record` 1…600.
- `click`: до двух необязательных токенов, каждый — кнопка (`left|right|middle`) или фаза (`down|up`), максимум по одному каждого вида, порядок любой; иное → `Err`. `dblclick [btn]`.
- `wheel dx dy [lines|px]`, по умолчанию `lines`; иное → `Err`.
- `key <combo>` — ровно один токен; ошибка `parse_combo` → `Err(format!("unknown key token ..."))` — текст ошибки `parse_combo` как есть.
- `bench <frames> [csv=<path>] [action…]`, `record <frames> <dir> [action…]`: `action` — `none` (по умолчанию) | `wheel <dx> <dy>` | `key <combo>` | `type <остаток>`.
- `dialog save|discard|cancel`, `info`, `quit` — лишние аргументы → `Err`.

- [ ] **Step 1: тесты `headless_protocol_*`** — по одному позитивному на каждую команду таблицы 4.4 и весь список негативов спеки: пустая строка, только пробелы, `# комментарий`, `click banana`, `mouse_move 10`, `mouse_move a b`, `mouse_move nan 5`, `wheel 0 -3 furlongs`, `resize 10x10`, `key ctrl+`, `key ctrl+foo`, `type` без аргумента (→ `Type("")`), `record 5` без каталога, `b"\xff\xfe"`, строка 1 МБ (`Err` с «unknown command»). Плюс: `open  /a b/c.rs` (путь с пробелом), `type \tx\\n` → таб, `x`, `\n`; `click up right`; `click left left` → `Err`; `bench 10 csv=/tmp/a.csv wheel 0 -3`; `record 601 /tmp/x` → `Err`; `Response::line` для `Ok(None)`, `Ok(Some("a\nb"))` → `"ok a b"`, `Err`.
- [ ] **Step 2:** парсер.
- [ ] **Step 3:** `make test TEST_FILTER='headless_protocol_'` — зелёные.
- [ ] **Step 4: commit** `headless: line protocol parser;`

---

### Task 10: CLI-опции и изоляция профиля — `profile.rs`

**Класс исполнителя:** `ds-low-agentic`. Спека: 4.1, 4.2 п.1–2.

**Files:**
- Create: `src/headless/profile.rs`
- Modify: `src/headless/mod.rs` (`mod profile;`), `src/platform.rs` (реэкспорт `AppPaths` сейчас под `#[cfg(test)]` (`platform.rs:~36-37`) — сделать безусловным `pub(crate) type AppPaths = integration::AppPaths;`), `PROJECT_GUIDE.md` §4

**Interfaces:**
- Consumes: `parse_size` (T9), `platform::app_paths()`, `platform::AppPaths`.
- Produces: `HeadlessOptions`, `ProfileChoice`, `BudgetChoice`, `parse_args`, `Profile` — как в таблице; плюс для тестов `Profile::prepare_in(opts: &HeadlessOptions, runtime_base: &Path, user_paths: &AppPaths) -> Result<Profile, String>` (`prepare` = `prepare_in(opts, <$XDG_RUNTIME_DIR или /tmp>, &platform::app_paths())` — вызывается в `run()` **до** `set_app_root_override`, поэтому `app_paths()` отдаёт настоящие каталоги пользователя).

`parse_args(args)` — `args` без имени программы, содержит `--headless` (игнорируется). Флаги спеки 4.1: `--script FILE|-`, `--size WxH`, `--scale S` (0.5…4.0, конечное), `--profile DIR`, `--profile-from-user`, `--hz N` (> 0), `--budget-ms F` (> 0), `--keep-profile`, `--allow-writes`, один позиционный `FILE_OR_DIR`. Ошибки (`Err(текст)`, вызывающий печатает в stderr и выходит с кодом 2): неизвестный флаг, флаг без значения, неверное число, размер вне границ, `--hz` вместе с `--budget-ms`, `--profile` вместе с `--profile-from-user`, два позиционных. Значения — `OsString` → путь без `to_string_lossy` (пути), числа — через `to_str()` (не-UTF-8 число → ошибка).

`Profile`:
- `Temp`: каталог `<runtime_base>/rriter-headless-<pid>`; если существует — `…-<pid>-<n>` (n = 1, 2, …) — чужой каталог не переиспользуется. Создаётся `create_dir_all` с правами 0700.
- `Dir(p)`: `create_dir_all(p)`; не удаляется никогда.
- `FromUser`: как `Temp`, затем рекурсивная копия `user_paths.config` → `<root>/config`, `user_paths.data` → `<root>/data`, `user_paths.state` → `<root>/state` (отсутствующий исходный каталог — пропуск; `cache` не копируется). Симлинки не разыменовываются и не копируются; копируются только обычные файлы и каталоги.
- `finish(self)`: удаляет корень рекурсивно **только** если он создан этим `Profile` как `Temp`/`FromUser` и `keep_profile == false`; ошибка удаления — сообщение в stderr, не паника. Удаление — единственное разрушительное действие задачи: путь берётся только из поля, заполненного в `prepare_in`, никогда из аргументов.

- [ ] **Step 1: тесты `headless_profile_*`:** `parse_args` — дефолты (`size (1920,1080)`, `scale 1.0`, `Temp`, `Auto`, `script None`), каждый флаг, позиционный путь, `--script -`; негатив: `--size 10x10`, `--size 1920x`, `--size x`, `--scale abc`, `--scale nan`, `--hz 0`, `--hz 144 --budget-ms 5`, `--profile /x --profile-from-user`, `--foo`, `--size` без значения, два позиционных. `prepare_in` во временном каталоге теста: `Temp` создаёт `rriter-headless-<pid>`, повторный `prepare_in` при существующем даёт `-1`; `finish` удаляет; `keep_profile` — остаётся; `Dir` не удаляется после `finish`; `FromUser` копирует файлы config/data/state из фейковых `user_paths`, не копирует `cache` и симлинк.
- [ ] **Step 2:** реализация.
- [ ] **Step 3:** `make test TEST_FILTER='headless_profile_'` — зелёные.
- [ ] **Step 4: commit** `headless: CLI options and isolated profile root;`

---

### Task 11: рантайм — сессия, кадр, settle, снимок, `main.rs`

**Класс исполнителя:** `ds-low-agentic`. Спека: 4.1 (коды выхода), 4.3, 4.4 (исполнение всех команд, кроме `dump`, `dialog`, `info`, `bench`, `record`), 4.5, 4.7, 6.

**Files:**
- Create: `src/headless/frame.rs` (`step_frame`, `settle_loop`, `StepState`, readback/PNG), `src/headless/tests.rs` (интеграционные, `#[cfg(all(test, target_os = "linux"))]`, модуль `tests_support`)
- Modify: `src/headless/mod.rs` (`run`, `HeadlessSession`, `execute`, `run_loop`), `src/main.rs` (≤ 10 строк: ветка `--headless` сразу после `handle_startup_helper`, до `init_rayon_global_pool`: Linux → `std::process::exit(headless::run(&args) as i32)`, не-Linux → stderr `headless mode is supported on Linux only`, `exit(2)`; вся логика — в `headless::run`; `init_rayon_global_pool` — приватная в корне крейта, `headless` зовёт её как `crate::init_rayon_global_pool()`), `src/app/events.rs` + `src/app/events/main_frame.rs` (вынос тел веток `WindowEvent::Resized` и `ScaleFactorChanged` в `App::handle_main_resized(&mut self, size: PhysicalSize<u32>)` / `App::handle_main_scale_factor_changed(&mut self, scale: f64)` в `main_frame.rs`; в оконной ветке остаются только `gl_surface.resize` на своём месте и вызов — порядок операций не меняется), `src/app/events/about.rs` (`about_to_wait` → `pub(crate)`), `PROJECT_GUIDE.md` §4

**Interfaces:**
- Consumes: всё из T2–T10: `OffscreenContext`, `WindowHost::Headless`, `HostLoop::Headless`, `KeyInput`, `render_main_frame`/`finish_main_frame`, `new_from_config`/`AppInitOptions::headless()`, `set_headless`, `set_app_root_override`, `parse_line`/`Command`/`Response`, `parse_args`/`Profile`.
- Produces: `run`, `HeadlessSession`, `step_frame`, `settle_loop`, `StepState`, `read_frame_rgba`, `flip_rows_in_place`, `write_png` — как в таблице; `tests_support::{ensure_test_profile_root, session_for_test(w, h) -> HeadlessSession}` для T12–T15.

`run(args)`: `parse_args` (ошибка → stderr, 2) → `Profile::prepare` (ошибка → 2) → `set_app_root_override(profile.root())` → `set_headless(HeadlessPolicy { allow_writes })` → `init_rayon_global_pool` (как в `main.rs`) → `HeadlessSession::new` (ошибка GL/`Renderer` → stderr `headless: EGL setup failed at <stage>: <text>. Try RRITER_EGL_VENDOR=mesa`, код 3) → позиционный путь (файл → как `open`, каталог → как `workspace`; ошибка — строка в stderr `headless: <path>: <причина>`, `had_error = true`; в stdout ничего) → `settle 500` без ответа → `run_loop(stdin | файл --script)` → `app.shutdown_background_services()` → drop сессии → `profile.finish()` → код (`exit_code()`: 1 при `had_error`, иначе 0).

`HeadlessSession::new(options, profile_root)`: `OffscreenContext::new(w, h)` → `load_config()` (крейт-корень `main.rs`, доступна из дочернего модуля) → `configure_tool_paths` → `App::new_from_config(config, AppInitOptions::headless())` → `Renderer::new(glow, scale, theme, requested_context)` → `renderer.resize(w, h)` → `window = Some(Arc::new(WindowHost::Headless(HeadlessWindow::new(size, scale))))`, `renderer = Some(..)`, `is_ready = true`, `tried_maximize = true`, `is_focused = true`. Порядок полей структуры: `app` раньше `gl` (drop GL-объектов `Renderer` при текущем контексте).

`execute(cmd)` — по таблице 4.4:
- `open p`: не существует / каталог / `File::open` не удался → `err`; иначе тот же вызов, что ветка `WindowEvent::DroppedFile` для файла (`events.rs:~544-551`) → кадр → `ok tabs=<n> active=<i>`.
- `workspace d`: не каталог → `err`; иначе как `DroppedFile` для каталога (`apply_selected_workspace_folder`) → кадр → `ok`.
- `resize WxH`: `gl.resize` (ошибка → `err`, всё прежнее) → `HeadlessWindow::set_size` → `renderer.resize` → `app.handle_main_resized(size)` → кадр → `ok WxH`.
- `scale S`: `set_scale_factor` → `app.handle_main_scale_factor_changed(S)` → кадр → `ok`.
- `mouse_move x y` → `handle_main_cursor_moved(PhysicalPosition::new(x, y))` → кадр.
- `click [btn] [phase]`: `Both` → `Pressed` → кадр → `Released` → кадр; `Down`/`Up` — одно событие и кадр. `dblclick` — два `Both` подряд без паузы.
- `wheel` → `LineDelta(dx as f32, dy as f32)` или `PixelDelta(PhysicalPosition::new(dx, dy))` → кадр.
- `key`: сохранить `app.modifiers`, поставить `mods`, `handle_main_key_input(host, input.clone())` → кадр → `handle_main_key_input(host, input.released())` → кадр → восстановить `modifiers`.
- `type t` → `handle_main_ime_commit(&t)` → кадр.
- `settle [ms]` → `settle_loop` → `ok frames=<n> settled=<bool>`.
- `wait ms` → `step_frame(force=false)` в цикле реальное время `ms`; если шаг ничего не нарисовал — сон 2 мс → `ok frames=<n>`.
- `screenshot p` → `step_frame(force=true)` → `read_frame_rgba` → `flip_rows_in_place` → альфа 255 → `write_png` (`create_dir_all` родителя) → `ok <abs path> <w>x<h>`; ошибка I/O → `err io: <текст>`.
- `quit` → `ok`, цикл завершается.
- `dump`, `dialog`, `info`, `bench`, `record` → `err '<name>' is not available yet` (заменяют T12, T14, T15).
- После каждой команды: если `loop_state.exit_requested` — ответ печатается, цикл завершается.

`run_loop(input, out)`: `read_until(b'\n')` в переиспользуемый `Vec<u8>` → `parse_line` (`Ok(None)` — пропуск без ответа; `Err(r)` → `err r`, `had_error = true`) → `execute` → `writeln!(out, "{}", resp.line())` + `flush`; ошибка записи (в т.ч. `BrokenPipe`) → выход из цикла как по `quit`. EOF → как `quit`. Ошибка чтения → stderr, выход.

`step_frame(force)` — псевдокод спеки 4.5: `about::about_to_wait(app, &HostLoop::headless(&loop_state))`; если `headless_window.take_redraw_request() || force` — `let outcome = app.render_main_frame();` (место для диалога — T12) `gl.finish()`, `app.finish_main_frame(outcome)` → `true`; иначе `false`. Не спит.

`settle_loop(budget, step)`: чистая функция: вызывает `step()` пока не наберётся два подряд `Idle { flow: Wait }` или не истечёт `budget`; после `Idle { WaitUntil(t) }` спит до `min(t, deadline)`; после `Idle { Poll }` — `yield_now`; после `Idle { Wait }` — 5 мс; `Redrawn` сбрасывает счётчик. Возвращает `(число Redrawn, settled)`. Сессия строит `StepState` из `step_frame` и `loop_state.last_control_flow`.

Readback: `pixel_store_i32(PACK_ALIGNMENT, 1)`; `buf.resize(w*h*4, 0)`; `read_pixels(0,0,w,h,RGBA,UNSIGNED_BYTE, PixelPackData::Slice(Some(&mut buf[..])))`; `flip_rows_in_place` — `split_at_mut` + `swap_with_slice` половин; буфер — поле сессии (переиспользуется). PNG — `image::save_buffer_with_format(path, buf, w, h, ExtendedColorType::Rgba8, ImageFormat::Png)` (обычный `save_buffer` выбирает формат по расширению); путь без родителя — `create_dir_all` не вызывать.

`tests_support::ensure_test_profile_root()` — `OnceLock`: временный каталог процесса тестов, `set_app_root_override` один раз (все последующие тесты процесса пишут состояние туда — это безопаснее, чем в настоящий `~/.config`). `session_for_test(w, h)` — `ensure_test_profile_root()`, `HeadlessSession::new(&HeadlessOptions{ size: (w,h), ..default }, root)`, `hz_probe = || None`. `set_headless` не зовётся.

- [ ] **Step 1: юнит-тесты `headless_frame_*`:** `flip_rows_in_place` на 2x2 (строки поменялись); `settle_loop` — шаг всегда `Redrawn` → `settled=false` и время ≈ бюджету (≤ бюджет + 50 мс); шаг `Idle{Wait}` всегда → `settled=true` за 2 шага; `Redrawn, Idle{Wait}, Redrawn, Idle{Wait}, Idle{Wait}` → `settled=true`, frames=2.
- [ ] **Step 2: интеграционные `headless_session_*`** в `tests.rs`: `open <tmpfile>` + `screenshot <tmp>/out.png` → PNG 640x400 (`image::open`), есть пиксель цвета фона темы (±2 на канал) и пиксель не цвета фона; `open` несуществующего → `err`, каталога → `err`; `resize 800x600` → `ok 800x600`, следующий снимок 800x600; `quit` → `run_loop` возвращается, `exit_code() == 0`, команды после `quit` не выполняются (клавиатурного выхода в приложении нет: Ctrl+Q закрывает файл/вкладки, `editor_keys.rs:~409,631`; `PendingAction::Quit` приходит только из оконного `CloseRequested` — тест спеки §7 «`key ctrl+q` ставит `exit_requested`» заменён этим); `HostLoop::headless(&state).exit()` внутри сессии → цикл завершается после текущего ответа; `run_loop` над `Cursor` `"bogus\nmouse_move 10 10\n# c\n\nquit\nmouse_move 1 1\n"` → ровно 3 строки ответа (`err unknown command 'bogus'`, `ok`, `ok`), `exit_code() == 1`; `screenshot /proc/rriter-nope/x.png` → `err io:`, следующая `mouse_move 1 1` → `ok`; `run_loop` с writer'ом, чей `write` возвращает `BrokenPipe`, → возврат без паники; `click` / `click right down` / `dblclick` / `wheel 0 -3` / `wheel 0 -40 px` / `type héllo` / `settle 200` / `wait 50` → `ok`.
- [ ] **Step 3:** реализация, `main.rs`, вынос `Resized`/`ScaleFactorChanged`.
- [ ] **Step 4:** `make test TEST_FILTER='headless_'` и `TEST_FILTER='host_'` — зелёные.
- [ ] **Step 5: ручная проверка бинаря** (сборка тестов уже тёплая; бинарь — `make fast` запрещён исполнителю — не делать; проверку бинаря делает контроллер после задачи).
- [ ] **Step 6: commit(ы)** — `headless: extract Resized/ScaleFactorChanged handlers;` отдельным коммитом, затем `headless: runtime session, frame loop, settle, screenshot, --headless entry;`

---

### Task 12: диалог подтверждения и `dump`

**Класс исполнителя:** `ds-low-agentic`. Спека: 3.3 (последний абзац), 4.6.

**Files:**
- Create: `src/headless/dump.rs` (JSON дампа, прямоугольники кнопок диалога, тесты)
- Modify: `src/app/app_state.rs` (поле `pub headless_dialog_open: bool`; фикстура `test_app()` и `new_from_config` — `false`), `src/app/app_window_external_methods.rs` (`App::modal_dialog_open(&self) -> bool { self.dialog_window.is_some() || self.headless_dialog_open }`; `show_action_dialog`: ветка без `native()` ставит `headless_dialog_open = true` и `request_redraw()`; `close_dialog()` снимает флаг), места с предикатом «диалог открыт» → `modal_dialog_open()`: `src/app/mouse/input.rs:~1644`, `src/app/mouse/cursor.rs:~212`, `src/app/mouse/wheel.rs:~768`, `src/app/events/about.rs:~1622`, `src/app/keyboard/main_keys.rs:~297-310` (Escape), `src/app/events/main_frame.rs` (`blink_alpha` и аргумент `dialog_window_open` в `Renderer::draw`), `src/renderer/renderer_primitives_tests.rs` (`Renderer::resize_viewport(&mut self, x: i32, y_gl: i32, w: u32, h: u32)` рядом с `resize`), `src/headless/frame.rs` (рисование диалога в `step_frame`), `src/headless/mod.rs` (команды `dump`, `dialog`), `src/headless/tests.rs`, `PROJECT_GUIDE.md` §4

**Interfaces:**
- Consumes: `take_external_request`, `headless_writes_allowed` (T8), сессия (T11).
- Produces: `dump::dump_json(app: &App, window: &HeadlessWindow, renderer: &Renderer) -> serde_json::Value`; `dump::dialog_layout(w: u32, h: u32, scale: f32) -> DialogLayout { ox, oy_top, dw, dh }`; `dump::dialog_buttons(layout, renderer) -> [(&'static str, [f32; 4]); 3]` (из кортежа `(Button, Button, Button)`, который возвращает `widgets::get_dialog_buttons`, `widgets.rs:~503`).

Рисование (спека 4.6): после `render_main_frame`, если `headless_dialog_open`: `dialog_layout` → `renderer.resize_viewport(ox, h − oy_top − dh, dw, dh)` → то же, что оконная ветка `events.rs:~378-427` делает между `make_current` и `swap_buffers` (`draw_dialog_window(&base_title)`, `flush`) → `renderer.resize(w, h)`. Если оконная ветка очищает поверхность диалога (`gl.clear`), в headless очистка ограничивается прямоугольником диалога через `SCISSOR_TEST` (`gl.scissor` = viewport диалога), состояние scissor после — как было до; иначе `glClear` сотрёт весь основной кадр. Буфер меньше диалога — рисуется от угла, обрезается viewport'ом.

`dialog save|discard|cancel`: не `headless_dialog_open` → `err no dialog`; иначе `begin_pending_action_save` / `discard_pending_action_changes` / `cancel_pending_action` → кадр → `ok`. Флаг снимает только `close_dialog()`.

`dump [file]`: состав ключей — ровно спека 4.6 (`size`, `scale`, `cursor_icon`, `mode`, `tabs`, `editor`, `ide_panel`, `overlays`, `dialog`, `external_request`, `clipboard: "disabled"`, `writes_allowed`, `hover`, `ui`). `mode`: `welcome` при `show_welcome`, иначе `ide` при `is_ide_mode`, иначе `editor`. `tabs[].path` — абсолютный путь или `null` (путь в JSON — `display()`; это вывод, не персист). `ui` — `ui_registry.elements` последнего кадра: `id` = `format!("{:?}", id)`, `kind`, `rect` (у `IconButton` — хит-прямоугольник), `overlay`. `dialog.buttons` — `get_dialog_buttons(0,0,dw,dh,s,renderer)` со сдвигом `(ox, oy_top)`. `external_request` — `take_external_request()` (сбрасывается чтением). Без файла → `ok <json одной строкой>`; с файлом → `create_dir_all` родителя, запись, `ok <abs path>`; I/O → `err io:`. Имена вложенных полей `IdePanelState` и overlays — по коду.

- [ ] **Step 1: тесты `headless_dump_*` / `headless_dialog_*`** на `session_for_test`: dump — валидный JSON со всеми ключами верхнего уровня, `mode == "welcome"` на старте; после `open <tmp>` — `tabs` содержит вкладку с этим путём и `active: true`, `mode == "editor"`; `mouse_move` в центр `rect` элемента из `ui`, открывающего настройки (тот же `UiId`, что в Task 1/3), затем `click` → `overlays.settings == true`; диалог: `open <tmp>`, `type x`, закрытие вкладки (шорткат закрытия вкладки из `main_keys.rs`) → `dialog.action == "CloseTab"`, три кнопки `save|discard|cancel` с прямоугольниками внутри буфера; `dialog cancel` → `dialog == null`, вкладка на месте; снова закрыть → `dialog discard` → `dialog == null`, вкладка закрыта; `dialog save` без диалога → `err no dialog`; `dialog save` для изменённой вкладки с путём во временном каталоге → файл на диске содержит новый текст, `dialog == null` (тесты не ставят политику — запись разрешена; ошибку записи тестом не вызывать: `PermissionDenied` уводит в `pkexec`). Поведение при неудачном сохранении — как у окна: для `CloseTab` `begin_pending_action_save` закрывает диалог до записи (`app_window_external_methods.rs:~286,342`) — не менять. Закрытие вкладки — Ctrl+Q/Ctrl+4 по `editor_keys.rs:~631,946`. `key escape` при открытом диалоге закрывает его, как в окне. Скриншот при открытом диалоге — пиксель в центре буфера отличается от того же пикселя без диалога.
- [ ] **Step 2:** реализация.
- [ ] **Step 3:** `make test TEST_FILTER='headless_'`, `TEST_FILTER='host_'`, `TEST_FILTER='dialog'` — зелёные.
- [ ] **Step 4: commit** `headless: centered confirmation dialog and JSON dump of UI state;`

---

### Task 13: обёртка и документация

**Класс исполнителя:** `ds-low-agentic`. Спека: 5.

**Files:**
- Create: `scripts/rriter_headless.py` — отступление от имени спеки `scripts/rriter-headless`: AGENTS.md §3 разрешает создавать только `.rs/.py/.dart/.md/.txt`, `chmod` не в списке разрешённых команд; соседние скрипты — `snake_case.py` (`build_windows.py`). Запуск: `python3 scripts/rriter_headless.py …`.
- Create: `docs/headless.md` (≤ 200 строк: CLI и коды выхода, протокол с таблицей команд, примеры `--script`, что изолировано и что нет (спека 4.2 последний абзац), ограничения: реальные часы, ведущие пробелы в `type`, клик по диалогу только командой `dialog`)
- Modify: `AGENTS.md` (одна строка в §0: «UI проверять headless: `python3 scripts/rriter_headless.py shot <file>` печатает путь PNG; протокол, `dump`, `bench` — `docs/headless.md`»), `PROJECT_GUIDE.md` §4

**Interfaces:**
- Consumes: бинарь `target/x86_64-unknown-linux-gnu/release/rriter` (или `RRITER_BIN`), протокол.
- Produces: подкоманды `shot`, `run`, `repl`, `--self-test`; T14 добавляет `bench`.

Обёртка (Python 3, stdlib): нет бинаря → stderr «rriter binary not found at <path>; run make fast», код 2; сама не собирает. `shot <file-or-dir> [--size] [--scale] [--out PNG] [--settle MS=500] [--profile-from-user]` — запускает `rriter --headless [опции]`, пишет в stdin `open|workspace <abs>`, `settle <MS>`, `screenshot <out>`, `quit`; читает ответы построчно; любой `err` → печать в stderr, код 1; успех — печатает абсолютный путь PNG (по умолчанию `/tmp/rriter-headless/<name>-<YYYYmmdd-HHMMSS>.png`). `run <script> [опции]` — `--script`, ответы в stdout, код выхода процесса. `repl [опции]` — stdin пользователя → процесс построчно. Все подкоманды: строки stdout процесса не на `ok`/`err` → stderr с префиксом `app: `; stderr процесса пробрасывается; код выхода процесса сохраняется. `--self-test` — проверки без бинаря: разбор аргументов подкоманд, классификация строк (`ok`, `ok x`, `err x`, `okay` → `app:`), построение списка команд `shot` для файла и каталога, путь по умолчанию.

- [ ] **Step 1:** `--self-test` сначала (падает), затем обёртка; `python3 scripts/rriter_headless.py --self-test` → успех (команды нет в списке AGENTS.md §3, но §4 разрешает в SuperPowers-workflow запускать тесты как угодно).
- [ ] **Step 2:** `docs/headless.md`, строка в `AGENTS.md`, §4 `PROJECT_GUIDE.md`.
- [ ] **Step 3: commit** `headless: rriter_headless.py wrapper and docs;`

---

### Task 14: `bench`, `info` и частота монитора

**Класс исполнителя:** `ds-low-agentic`. Спека: 4.8 (кроме `record`), 4.4 (`info`).

**Files:**
- Create: `src/headless/bench.rs` (бюджет, замеры, сводка, CSV, системные метрики, тесты)
- Modify: `src/render_view/root_helpers.rs` (`pub struct FrameTelemetry`, `pub fn take_frame_telemetry() -> FrameTelemetry` — снимает и обнуляет существующие накопители thread-local `Telemetry`: flush count/vertices, `root_phase_time[5]`, `chrome_detail_time[6]`; новых счётчиков в рендерере нет), `src/platform.rs` (`probe_display_refresh_hz`), `src/headless/mod.rs` (команды `bench`, `info`), `scripts/rriter_headless.py` (подкоманда `bench <file> <frames> [action…] [--hz N]`: `open` → `settle` → `bench` → печать JSON-сводки и пути CSV; `--self-test` дополнить), `docs/headless.md` (раздел «Бенч»: поля, интерпретация — абзац спеки 4.8 «Интерпретация»), `src/headless/tests.rs`, `PROJECT_GUIDE.md` §4

**Interfaces:**
- Consumes: сессия, `step_frame`, `KeyInput` (T11, T5), `platform::run_command_output`.
- Produces: `bench::resolve_budget(choice: BudgetChoice, probe: impl FnOnce() -> Option<f64>) -> Budget { hz: f64, budget_ms: f64, source: &'static str }`; `bench::percentiles(&mut [f64]) -> Percentiles { p50, p95, p99, max }`; `bench::parse_proc_stat_cpu_ms(stat: &str, clk_tck: f64) -> Option<f64>`; `bench::FrameRow` (поля таблицы спеки 4.8 + для T15 `Option`-поля анимаций); `bench::run_frames(session, frames, action, record_dir: Option<&Path>) -> Result<BenchSummary, String>` — общий цикл для `bench` и `record` (T15 включает `record_dir`).

`probe_display_refresh_hz()` (Linux; на прочих ОС — `None`): `OnceLock<Option<f64>>`; `EventLoop::builder().build()` → ошибка → `None`; `run_app` с одноразовым `ApplicationHandler`, который в `resumed` берёт максимум `available_monitors().filter_map(|m| m.refresh_rate_millihertz())` / 1000.0 и зовёт `event_loop.exit()`; `window_event` — пусто. Окно не создаётся. Вызов лениво — первый `bench`/`record`/`info`, главный поток; сессия зовёт через поле `hz_probe` (в `run()` — `platform::probe_display_refresh_hz`, в тестах — `|| None`).

Бюджет: `Ms(f)` → `(1000/f, f, "arg")`; `Hz(n)` → `(n, 1000/n, "arg")`; `Auto` + `Some(hz)` → `"monitor"`; `Auto` + `None` → `(240, 4.1667, "default")`.

`bench <frames> [csv=path] [action]`: `TELEMETRY_ENABLED = true` на время прогона (прежнее значение восстанавливается и при ошибке); GPU timer query (`create_query`, `begin_query(TIME_ELAPSED)`/`end_query` вокруг `render_main_frame` + `finish`, результат `get_query_parameter_u64(QUERY_RESULT)`; `TIME_ELAPSED` — ядро OpenGL 3.3, но `create_query` поддержку не проверяет: после первого `begin_query`/`end_query` проверить `gl.get_error() == NO_ERROR`, иначе или при ошибке создания → `gpu_ms: null` для всего прогона); на кадр: action (`wheel` → `LineDelta`; `key` → press+release без кадра между ними; `type` → IME-commit) → замер `update_ms` (`about_to_wait` + action) → `draw_cpu_ms` (`render_main_frame` до `gl.finish()`) → `total_ms` (всё, включая ожидание `finish`) → `take_frame_telemetry()` → строка CSV. Сводка — ровно JSON спеки 4.8 (`frames`, `budget_ms`, `hz`, `hz_source`, `total_ms`/`gpu_ms`/`draw_cpu_ms` перцентили, `over_budget`, `worst` ×5, `system` {`loadavg_before/after` из `/proc/loadavg`, `cpus` = `available_parallelism`, `gpu_util_before/after` — `nvidia-smi --query-gpu=utilization.gpu --format=csv,noheader,nounits` через `platform::run_command_output` с таймаутом 2 с, нет бинаря/ошибка → `null`, `process_cpu_ms` — utime+stime `/proc/self/stat` (поля после последней `)`) × 1000 / `sysconf(_SC_CLK_TCK)` за время бенча}, `csv` — абсолютный путь). CSV по умолчанию — `${XDG_RUNTIME_DIR:-/tmp}/rriter-headless/bench-<unix_ms>.csv` (не в профиле). CSV пишется через буферизованный writer, строки формируются в переиспользуемую `String`.

`info` → `ok {"hz":..,"budget_ms":..,"hz_source":..,"gl_renderer":..,"gl_version":..,"gl_vendor":..,"writes_allowed":..,"profile":"<root>"}`.

- [ ] **Step 1: тесты `headless_bench_*`:** `resolve_budget` — `Hz(144)` → 6.944 (±0.001), `Ms(5.0)`, `Auto`+`Some(165.0)` → `monitor`, `Auto`+`None` → 240/`default`; `percentiles` на 1..=100 → p50 50, p95 95, p99 99, max 100 (метод ближайшего ранга — зафиксировать в тесте); `parse_proc_stat_cpu_ms` на строке с `comm` вида `(rr iter) x)` и на мусоре → `None`; сессия: `bench 5` → `frames == 5`, CSV — заголовок + 5 строк, `hz_source == "default"`; `bench 3 wheel 0 -3` на длинном документе → `scroll_y` в последней строке CSV больше, чем в первой; `bench 2 csv=/proc/rriter-nope/a.csv` → `err io:`; `info` — JSON со всеми ключами; после `bench` `TELEMETRY_ENABLED` вернулся к прежнему значению.
- [ ] **Step 2:** реализация, обёртка, docs.
- [ ] **Step 3:** `make test TEST_FILTER='headless_'`; `python3 scripts/rriter_headless.py --self-test`.
- [ ] **Step 4: commit** `headless: frame-cost bench, info, monitor refresh probe;`

---

### Task 15: `record`

**Класс исполнителя:** `ds-low-agentic`. Спека: 4.8 (абзац `record`).

**Files:**
- Modify: `src/headless/bench.rs` (`record_dir` в `run_frames`, колонки анимаций, `motion` в сводке), `src/headless/mod.rs` (команда `record`), `src/headless/tests.rs`, `docs/headless.md` (абзац про `record`: траектория анимации, стоимость кадра — `bench`)

**Interfaces:**
- Consumes: `run_frames`, `FrameRow`, readback/PNG из `frame.rs` (переиспользуемый буфер сессии).
- Produces: команда `record <frames> <dir> [action]` → `ok <json summary>`.

Поведение: `create_dir_all(dir)` (ошибка → `err io:`); `<dir>/frames.csv` с колонками `bench` + `sticky_anim_progress`, `search_anim_y`, `tab_scroll` (значения из `App` — те же поля, что идут аргументами в `Renderer::draw`); после каждого кадра — readback и `<dir>/frame-%04d.png` (время readback/кодирования вне `update_ms`/`draw_cpu_ms`/`gpu_ms`). Сводка = сводка `bench` + `"motion": {"scroll_y_delta_min", "scroll_y_delta_max", "nonmonotonic_frames"}` — число кадров, где знак дельты `scroll_y` сменился относительно предыдущей ненулевой дельты при неизменном action (чистая функция `motion_stats(&[f64]) -> Motion`).

- [ ] **Step 1: тесты `headless_record_*`:** `motion_stats` — монотонный ряд → 0; `[0, 1, 3, 2, 4]` → 1 (знак сменился на кадре 3), дельты min/max; пустой и одноэлементный ряд → нули; сессия: `record 3 <tmp> wheel 0 -3` → 3 PNG `frame-0000..0002.png` нужного размера, `frames.csv` с заголовком + 3 строки и колонкой `tab_scroll`, сводка с `motion`; `record 3 /proc/rriter-nope` → `err io:`.
- [ ] **Step 2:** реализация, docs.
- [ ] **Step 3:** `make test TEST_FILTER='headless_'`.
- [ ] **Step 4: commit** `headless: record per-frame PNG and motion trajectory;`

---

## После задач (контроллер, не задача плана)

1. Финальное ревью всей ветки (SDD), затем `make codex_test` в фоне — упавшие тесты сверить с `baseline-failures.txt`.
2. Ручная приёмка спеки §7 на свежем `make fast`-бинаре (его строит `codex_test`): `python3 scripts/rriter_headless.py shot src/main.rs` → открыть PNG (Read); `bench src/main.rs 480 wheel 0 -3` → `hz_source=monitor`, `hz=240`; `record` 60 кадров скролла → монотонный `scroll_y` (`nonmonotonic_frames == 0`).
3. Kanri: карточку «Headless screenshot mode…» (доска RRiter, Features) → Done; новая карточка «UI-тесты через headless» в Backlog (`kanri-add`, пишет только при закрытом Kanri).

## Покрытие спеки

| Раздел спеки | Задача |
|---|---|
| 3.1 WindowHost | T2 |
| 3.2 KeyInput | T5 |
| 3.3 HostLoop | T4; headless-диалог — T12 |
| 3.4 offscreen_gl | T3 |
| 3.5 render_main_frame, new_from_config | T6, T7; Resized/ScaleFactorChanged — T11 |
| 4.1 CLI | T10 (разбор), T11 (коды выхода, `main.rs`) |
| 4.2 профиль и политика | T10 (профиль), T8 (override, гейты, печать телеметрии), T7 (clipboard, dart, мигание) |
| 4.3 старт | T11 |
| 4.4 протокол | T9 (парсер), T11/T12/T14/T15 (исполнение) |
| 4.5 кадр и settle | T11 |
| 4.6 диалог и dump | T12 |
| 4.7 снимок | T11 |
| 4.8 bench/record | T14, T15 |
| 5 обёртка и docs | T13, T14 (bench) |
| 6 ошибки и границы | T3 (EGL-этапы), T11 (коды, I/O, BrokenPipe, settle), T12 |
| 7 тестирование | T1 (характеризация), тесты в каждой задаче, финал |
| 8 файлы | все; `capture.rs` → `frame.rs` (решение 1) |
