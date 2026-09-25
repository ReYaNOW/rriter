# Headless-режим RRiter — спека

Дата: 2026-09-25. Статус: на review.

## 1. Цель

Запускать `rriter` без окна, программно доводить его до нужного состояния (открыть файл или папку, кликнуть, набрать текст, проскроллить, открыть панель), получать PNG кадра, машинно-читаемое состояние UI и замеры стоимости кадра. Потребитель — Claude Code в терминале, пока пользователь занят полноэкранным приложением: ничего не должно появляться на экране и ничто не должно трогать состояние живого экземпляра редактора.

Вторая задача (тесты UI через этот же механизм) в спеку не входит, но дизайн обязан ей не мешать: детерминированный протокол команд, JSON-дамп состояния как поверхность для утверждений, ненулевой код выхода при ошибках скрипта.

### Не цели

- Снимок экрана (spectacle/grim): попадёт игра, а не приложение.
- Виртуальный дисплей (Xvfb, weston headless): не установлены, дефолтная сборка Wayland-only.
- Виртуальные часы: `Instant::now()` встречается 265 раз в 68 файлах, это отдельный рефакторинг. Анимации в headless идут по реальному времени.
- Windows и macOS: headless только Linux; на других ОС — сообщение и код выхода 2.
- Сравнение снимков с эталонами, golden-тесты: вторая задача.
- Изменение поведения оконного режима. Все рефакторинги ниже — без изменения поведения окна.

## 2. Факты, на которых стоит дизайн

Собраны разведкой 2026-09-25 (все ссылки на текущий `master`).

- Offscreen GL уже есть и работает на этой машине: `reviewer_stage2_integration::OffscreenContext` в `src/render_view/markdown_scroll_transition_review_tests.rs:17-145` — `dlopen("libEGL.so.1")`, `eglGetPlatformDisplay(EGL_PLATFORM_SURFACELESS_MESA = 0x31DD)`, pbuffer 1000x800, контекст OpenGL 3.3 Core, `glow::Context::from_loader_function`, настоящий `Renderer::new`. Только `cfg(all(test, target_os = "linux"))`. NVIDIA EGL 1.5 на машине пользователя поддерживает `EGL_KHR_surfaceless_context`; llvmpipe включается существующим `RRITER_EGL_VENDOR=mesa` (`src/main.rs:949-1011`).
- Пиксельного readback нет (0 `glReadPixels`, 0 FBO); кадр рисуется в framebuffer 0, единственный draw call — `gl.draw_arrays` в `Renderer::flush` (`src/render_view/core_text.rs:1457`). Крейт `image` с фичей `png` уже в зависимостях (`Cargo.toml:16`).
- `App.window: Option<Arc<winit::window::Window>>` (`src/app/app_state.rs:1032`): 473 обращения `self.window` в 26 файлах, из них 250 `self.window.as_ref().unwrap()`. Вызываемые методы окна: `request_redraw` (219 unwrap-мест), `inner_size` (48), `scale_factor` (4), `set_title` (1, через `update_window_title`), `set_maximized` (1), `is_maximized` (3), `focus_window` (2, диалог), `request_inner_size` (1, automation), `set_ime_allowed` (1), `set_cursor` (1), `window_handle` (2 production), `id` (1). Ноль вызовов `drag_window`, `set_fullscreen`, `set_visible`, `set_ime_cursor_area`.
- `Arc<Window>` уходит в фоновые потоки только ради `request_redraw`: `src/app/terminal_process.rs:144,616-618,672-673`, `src/app/tool_installer.rs:118,146-147,336,613`.
- `&ActiveEventLoop` в коде используется для: `exit()` (5 мест: `window_runtime.rs:311,371`, `about.rs:132,216`, `editor_keys.rs:435`), `set_control_flow` (8 мест: `events.rs:611,623,1507`, `about.rs:145,1648,1650,1654`), `create_window` (1: `app_window_external_methods.rs:55`, диалог подтверждения) и glutin `DisplayBuilder::build` (`window_runtime.rs:250`). `handle_main_mouse_input_inner` уже принимает `Option<&ActiveEventLoop>` под `cfg(test)` (`src/app/mouse/input.rs:478-481`) и параметр не читает.
- `winit::event::KeyEvent` нельзя сконструировать вне winit (приватное поле). Из него читаются только `physical_key`, `state`, `logical_key.to_text()`, `text`, `repeat` — в `src/app/keyboard/main_keys.rs`, `src/app/keyboard.rs`, `src/app/keyboard/editor_keys.rs`, `src/app/file_tree_dialog.rs`, `src/app/database/database_table_edit_methods.rs`, `src/app/database/database_app_methods.rs`, `src/app/api_client/api_client_app_request_methods.rs`. Модификаторы — `App.modifiers: ModifiersState` (`app_state.rs:1057`), пишутся из `ModifiersChanged` (`events.rs:535-541`).
- Мышь и колесо конструируемы снаружи: `handle_main_cursor_moved(PhysicalPosition<f64>)` (`mouse/cursor.rs:193`), `handle_main_mouse_input(event_loop, ElementState, MouseButton)` (`mouse/input.rs:419`), `handle_main_mouse_wheel(MouseScrollDelta)` (`mouse/wheel.rs:93`), `handle_main_ime_commit(&str)` (`keyboard.rs:255`).
- Все обработчики ввода требуют живой `Renderer` (`mouse/input.rs:485`, `mouse/wheel.rs:97`, `mouse/cursor.rs:197`): подача событий без GL невозможна в принципе.
- Кадр: `Renderer::draw` с 38 параметрами (`src/render_view/root_frame_renderer.rs:118`) собирается вручную в `src/app/events.rs:758-805` внутри `RedrawRequested`; из окна в этой ветке нужны `request_redraw` (`:642,812,1545`), `inner_size` (`:832,1169,1359,1386`), `set_cursor` (`:1479`); из GL — `present_main_surface` (`app_window_external_methods.rs:91-108`). `Renderer::resize(w,h)` (`src/renderer/renderer_primitives_tests.rs:15-23`) задаёт вьюпорт без окна.
- Решение «нужен ли кадр»: `about::about_to_wait` (`src/app/events/about.rs:95`) → `compute_about_wait_plan` (`about_helpers.rs:359-408`) → `window.request_redraw()` + `set_control_flow`.
- Диалог подтверждения (`PendingAction::Quit|CloseTab|CloseFile|CloseAllTabs|OpenFile`) — второе winit-окно 660x260 (`show_action_dialog`, `app_window_external_methods.rs:41-88`), рисуется тем же `Renderer` через `draw_dialog_window(&base_title)` (`src/render_view/ui.rs:935`, вызов `events.rs:378-427`), ответ — хит-тест `widgets::get_dialog_buttons(0,0,660*s,260*s,s,renderer)` (`events.rs:352-372`) → `begin_pending_action_save` / `discard_pending_action_changes` / `cancel_pending_action`. Затемнение фона — флаг `dialog_window_open` в `Renderer::draw`.
- Нативные пикеры — `rfd` за `platform::pick_file/pick_files/pick_folder/save_file*` (`src/platform.rs:1233-1270`), на Linux запускаются в потоках `rriter-*-picker` (`app_window_external_methods.rs:359,390,409`), результат через mpsc в `App.open_file_rx/open_folder_rx/save_file_rx`, опрос в `about.rs:1013-1092`. В headless без блокировки они откроют окно поверх игры.
- Внутренние оверлеи (file tree dialogs, database modal, git confirm, settings, search, welcome) — обычные поля состояния, рисуются в основной кадр, регистрируются в `UiRegistry` — headless их получает бесплатно.
- `UiRegistry` (`src/ui_system.rs:647-660`): `elements: Vec<UiElement>` с вариантами `Button|IconButton|TextInput|Rect {id, x, y, w, h…}`, `hovered: Option<UiId>`; `UiId` — 393 варианта с `Debug`, без serde и строковых имён; `clear()` в начале кадра (`events.rs:660`); клик — `find_at`/`find_overlay_at` → `handle_ui_click` (`ui_handlers.rs:254`).
- Телеметрия: `TELEMETRY_ENABLED: AtomicBool` (`root_helpers.rs:37`), thread-local `Telemetry` (`root_helpers.rs:431-489`: фазы root, flush time/count/vertices, swap, chrome detail), печать раз в 10 с `println!` в stdout (`root_frame_overlay_helpers.rs:721-790`). `--bench-scroll-render` (`about.rs:164-217`) — образец: импульсы `scroll_by(36.0*dir)` с шагом 1/120 с, `SCROLL_BENCH_START/DONE` в stdout.
- Состояние: каталоги только через `XDG_CONFIG/DATA/CACHE/STATE_HOME` (`src/platform/integration.rs:975-997`); `load_config` при первом запуске создаёт и пишет `config.json` (`main.rs:770-795`); при выходе `persist_state_and_shutdown` (`window_runtime.rs:331-365`) пишет config/panels/api и зовёт `shutdown_background_services` (`app_ide_tab_methods.rs:388-399`). При старте без действий пользователя поднимаются highlighter worker, rayon pool и проба `dart --version` (`refresh_dart_tool_state`, `main.rs:2411`). Single-instance нет. `kdeglobals` читается через `XDG_CONFIG_HOME`/`HOME` (`main.rs:891-899`).
- CLI: ручной разбор `std::env::args_os()` в `main.rs:2040-2098`, probe-режимы выходят до `EventLoop`; `initial_file_argument` (`:1948-1980`) — первый аргумент без `--`.
- Профиль сборки `panic = "abort"` (`Cargo.toml:203`): паника в headless завершает процесс.

## 3. Архитектура

Headless — третий хост для того же `App`, рядом с оконным режимом и `--pgo-train`. Точка входа `rriter --headless [опции]` в `main.rs` до создания `EventLoop`: изолированный профиль → offscreen GL → `App` с `renderer` и `window: Some(WindowHost::Headless)` → собственный цикл «команда → выполнить → ответ». Весь код приложения (ввод, layout, рендер) — production-путь, никаких `cfg(test)`-швов.

```
stdin / --script ──► headless::protocol ──► App::handle_main_*  ──► headless::frame (about_to_wait-логика + render_main_frame)
                                                                         │
stdout ◄── ok/err ◄── headless::{capture,dump,bench} ◄── pbuffer ◄───────┘
```

Пять рефакторингов-опор. Каждый — без изменения поведения окна, каждый проверяется тем, что существующие тесты проходят без правок ассертов.

### 3.1 `WindowHost` — `src/platform/window_host.rs`

```rust
pub enum WindowHost {
    Native(Arc<winit::window::Window>),
    Headless(HeadlessWindow),
}

pub struct HeadlessWindow {
    size: Mutex<PhysicalSize<u32>>,   // меняется командой resize
    scale_factor: Mutex<f64>,         // меняется командой scale
    title: Mutex<String>,
    maximized: AtomicBool,
    ime_allowed: AtomicBool,
    cursor_icon: Mutex<CursorIcon>,
    redraw_requested: AtomicBool,     // ставят App и фоновые потоки, читает headless-цикл
    id: WindowId,                     // WindowId::dummy() — winit даёт его без окна
}
```

Методы `WindowHost` повторяют имена и сигнатуры методов `winit::window::Window`, которые зовёт код: `request_redraw()`, `inner_size() -> PhysicalSize<u32>`, `scale_factor() -> f64`, `set_title(&str)`, `set_maximized(bool)`, `is_maximized() -> bool`, `focus_window()`, `request_inner_size(Size) -> Option<PhysicalSize<u32>>`, `set_ime_allowed(bool)`, `set_cursor(impl Into<Cursor>)`, `id() -> WindowId`. Для `Native` — делегирование; для `Headless` — запись в поля или no-op (`focus_window`). Дополнительно `native(&self) -> Option<&Arc<Window>>` для двух мест, которым нужен настоящий `Window` (создание GL-surface в `window_runtime.rs:210-230`, `window_handle()`), и `headless(&self) -> Option<&HeadlessWindow>` для headless-цикла (`take_redraw_request()`, `set_size`, `set_scale_factor`).

Тип поля `App.window` меняется на `Option<Arc<WindowHost>>`; `dialog_window` остаётся `Option<Arc<Window>>` — второе окно существует только в оконном режиме. 473 call-site'а вида `self.window.as_ref().unwrap().request_redraw()` компилируются без правок. Меняются только: `window_runtime.rs` (создание, `native()` для surface), `app_window_external_methods.rs:31` (`update_window_title(&WindowHost, …)`), `terminal_process.rs` и `tool_installer.rs` (тип параметра `Option<Arc<WindowHost>>`), `automation.rs:522-529` (`request_inner_size` через `WindowHost`), `events.rs:434` (`id()`), `platform.rs:15` и `app_state.rs:13` (импорты), тестовая фикстура `test_app()` (тип поля, значение `None` не меняется).

`HeadlessWindow::request_inner_size` меняет `size` и возвращает `Some(new)`: оконный `--pgo-train` путь этим не пользуется, headless использует его для команды `resize`.

### 3.2 `KeyInput` — `src/app/keyboard/key_input.rs`

```rust
#[derive(Clone, Debug, PartialEq)]
pub struct KeyInput {
    pub physical_key: winit::keyboard::PhysicalKey,
    pub logical_text: Option<SmolStr>,  // KeyEvent::logical_key.to_text()
    pub text: Option<SmolStr>,          // KeyEvent::text
    pub state: ElementState,
    pub repeat: bool,
}

impl From<&winit::event::KeyEvent> for KeyInput { … }
impl KeyInput {
    /// "ctrl+shift+p", "enter", "f5", "a", "shift+tab" → (KeyInput, ModifiersState).
    /// Неизвестный токен → Err с текстом. Регистр токенов не важен.
    pub fn parse_combo(s: &str) -> Result<(KeyInput, ModifiersState), String>;
}
```

`handle_main_keyboard_input(event_loop, KeyEvent)` (`main_keys.rs:175`) становится тонкой обёрткой: `KeyInput::from(&key_event)` → `handle_main_key_input(host, KeyInput)`. Внутренние функции семи файлов, принимавшие `KeyEvent`/`&KeyEvent`, переводятся на `KeyInput`/`&KeyInput`; чтения `event.logical_key.to_text()` становятся `event.logical_text.as_deref()`, остальные поля — один в один. Поведение не меняется: `KeyInput` содержит ровно те пять полей, которые код читал.

`parse_combo` покрывает: буквы и цифры (`physical_key = KeyCode::KeyA…`, `text = Some("a")`, с `shift` — заглавная), `enter`, `tab`, `escape`/`esc`, `backspace`, `delete`, `space`, `up/down/left/right`, `home/end/pageup/pagedown`, `f1..f12`, `insert`, знаки пунктуации ASCII (`.`, `,`, `/`, `-`, `=`, `;`, `'`, `[`, `]`, `\`, `` ` ``), модификаторы `ctrl`, `shift`, `alt`, `super`/`meta`. Для символов вне таблицы headless использует команду `type` (IME-commit), а не `key`.

### 3.3 `HostLoop` — `src/app/events/host_loop.rs`

```rust
pub enum HostLoop<'a> {
    Native(&'a ActiveEventLoop),
    Headless { exit_requested: &'a AtomicBool },   // читает headless-цикл
}
impl HostLoop<'_> {
    pub fn exit(&self);                              // Native → event_loop.exit(); Headless → флаг
    pub fn set_control_flow(&self, ControlFlow);     // Native → делегирование; Headless → no-op
    pub fn native(&self) -> Option<&ActiveEventLoop>; // только show_action_dialog
}
```

Все функции, принимавшие `&ActiveEventLoop` кроме `bootstrap`/`resume` (glutin требует настоящий цикл) и `ApplicationHandler`-методов, принимают `&HostLoop`: `about::about_to_wait`, `save_state_and_exit`, `show_action_dialog`, `AutomationController::tick/run_step`, `advance_automation`, `handle_main_mouse_input(_inner)`, `handle_main_keyboard_input(_inner)`, `handle_editor_keyboard_input`. `cfg(test)`-ветка `Option<&ActiveEventLoop>` в `mouse/input.rs:480-481` и тестовый вход `reviewer_markdown_read_mouse_input` переводятся на `HostLoop` (тесты передают `HostLoop::headless(&flag)`).

`show_action_dialog(host, action)`: при `host.native()` — как сейчас; иначе диалог не создаёт окно, а помечается `App.headless_dialog_open = true` (поле `App`, всегда `false` в оконном режиме); `pending_action` уже хранится в `App`. Раздел 4.6 описывает рисование и ответ.

### 3.4 `platform::offscreen_gl` — `src/platform/offscreen_gl.rs`

Перенос `OffscreenContext` из `markdown_scroll_transition_review_tests.rs:17-145` в production-модуль под `#[cfg(target_os = "linux")]`, публичный API:

```rust
pub struct OffscreenContext { /* display, context, surface, lib */ }
impl OffscreenContext {
    /// Surfaceless EGL + pbuffer width×height, OpenGL 3.3 Core. Ошибка — текст с этапом
    /// (dlopen / GetPlatformDisplay / Initialize / ChooseConfig / CreatePbuffer / CreateContext / MakeCurrent)
    /// и кодом eglGetError.
    pub fn new(width: u32, height: u32) -> Result<Self, String>;
    pub fn glow(&self) -> glow::Context;
    /// Пересоздаёт pbuffer под новый размер, контекст тот же (для команды resize).
    pub fn resize(&mut self, width: u32, height: u32) -> Result<(), String>;
    pub fn requested_context(&self) -> RequestedContext; // то, что ждёт Renderer::new
}
```

Текущие тесты `reviewer_stage2_integration` используют `platform::offscreen_gl::OffscreenContext` вместо своей копии; их ассерты не меняются. Модуль не собирается на не-Linux; вызывающий код headless на не-Linux печатает «headless mode is supported on Linux only» и выходит с кодом 2.

Использование `dlopen`, а не glutin: glutin 0.32 умеет pbuffer (`create_pbuffer_surface`), но его `Display::new` требует `RawDisplayHandle`, у которого нет surfaceless-варианта; проверенный путь уже есть.

### 3.5 `App::render_main_frame` — вынос кадра

Ветка `WindowEvent::RedrawRequested` (`events.rs:608-1572`) делится на две функции без изменения порядка операций:

- `App::render_main_frame(&mut self) -> FrameOutcome` — всё от `ui_registry.clear()` до конца `Renderer::draw` и пост-обработки (регистрация hover, автокомплит-статистика, `pending_key_log`), кроме презентации. `FrameOutcome { wants_another_frame: bool }` — то, что сейчас приводит к `request_redraw` внутри ветки (`:812, :1545`).
- Оконная ветка: `render_main_frame()` → `present_main_surface()` → `finish_present` → `record_presented_frame` → `set_control_flow`, как сейчас.

Обращения к `window.inner_size()` внутри кадра остаются — через `WindowHost` они отдают размер pbuffer. `window.set_cursor(icon)` (`:1479`) в headless пишет в `HeadlessWindow.cursor_icon` и попадает в `dump`.

Аналогично `about_to_wait` (`about.rs:95`) остаётся одной функцией с параметром `&HostLoop`; headless зовёт её напрямую перед каждым кадром — она уже опрашивает mpsc-приёмники (пикеры, LSP, git), тикает анимации и ставит `needs_redraw`/`request_redraw`.

## 4. Headless-подсистема — `src/headless/`

Модуль `src/headless/` (`mod.rs`, `protocol.rs`, `frame.rs`, `capture.rs`, `dump.rs`, `bench.rs`, `profile.rs`), весь под `#[cfg(target_os = "linux")]`; на других ОС `main.rs` при `--headless` печатает сообщение и выходит с кодом 2. Файлы по 200–1500 строк, тесты рядом в `#[cfg(test)]`.

### 4.1 CLI

```
rriter --headless [--script FILE] [--size WxH] [--scale S] [--profile DIR | --profile-from-user]
                  [--hz N | --budget-ms F] [--keep-profile] [FILE_OR_DIR]
```

- `--headless` разбирается в `main.rs` вместе с probe-флагами, до `EventLoop`, после `init_rayon_global_pool`. Ветка зовёт `headless::run(opts) -> ExitCode` и возвращает её код.
- `--script FILE` — команды из файла; без него — stdin. `-` = stdin явно.
- `--size` по умолчанию `1920x1080`, `--scale` по умолчанию `1.0`. Минимум 320x200, максимум 8192x8192; иначе код выхода 2 с текстом.
- Позиционный `FILE_OR_DIR`: файл → эквивалент команды `open`, каталог → `workspace`, выполняются до первой команды скрипта. Нет пути — welcome-экран.
- `--hz`/`--budget-ms` — бюджет кадра для `bench`/`record` (раздел 4.8). Взаимоисключающие; оба заданы → код 2.
- `--keep-profile` — не удалять временный профиль на выходе (для разбора состояния).
- Разбор ошибок аргументов — до любого GL: неверное значение → сообщение в stderr, код 2.

Коды выхода: 0 — все команды `ok` или `quit`/EOF; 1 — хотя бы одна команда вернула `err` (только в режиме `--script`; в stdin-режиме код всегда 0 при штатном `quit`/EOF); 2 — ошибка аргументов или платформа не поддерживается; 3 — не удалось создать GL-контекст или `Renderer`.

### 4.2 Изоляция профиля — `profile.rs`

Выполняется до `load_config` и до создания `App`:

1. Корень профиля: `--profile DIR` (создаётся, если нет; многоразовый) или временный каталог `${XDG_RUNTIME_DIR:-/tmp}/rriter-headless-<pid>/`, который удаляется на выходе (кроме `--keep-profile`).
2. `--profile-from-user`: во временный корень копируются `~/.config/RRiter`, `~/.local/share/RRiter`, `~/.local/state/RRiter` (без `cache`); `kdeglobals` копируется в `<root>/config/kdeglobals`, чтобы цвет выделения совпал с живым экземпляром. Несовместимо с `--profile`.
3. `std::env::set_var` для `XDG_CONFIG_HOME`, `XDG_DATA_HOME`, `XDG_CACHE_HOME`, `XDG_STATE_HOME` → `<root>/{config,data,cache,state}`. Это единственный рычаг, который знает `app_paths_with` (`integration.rs:975-997`); установка происходит в начале `main`, до любых потоков, поэтому гонок с чтением env нет.
4. `platform::set_headless(true)` — глобальный `AtomicBool` в `src/platform.rs`. Его читают: `pick_file/pick_files/pick_folder/save_file*` (возвращают `None` немедленно и ставят `App`-независимый счётчик `platform::headless_picker_requests()` с видом последнего запроса — попадает в `dump`), `refresh_dart_tool_state` (пропуск пробы `dart --version`), периодическая печать телеметрии (`root_frame_overlay_helpers.rs:721`, пропуск). `persist_state_and_shutdown` headless-цикл сам не вызывает; если до него доходит `about_to_wait` (путь `PendingAction::Quit` → `save_state_and_exit`), запись идёт в изолированный профиль, а `host.exit()` ставит флаг завершения — цикл выходит штатно.
5. `TELEMETRY_ENABLED` в headless не включается из конфига; включается только на время `bench`/`record` (раздел 4.8).

Инкрементальные записи (`save_recent_files`, `save_tabs_state`, `save_panel_state`) остаются включёнными: они пишут внутрь профиля. Это нужно `--profile DIR`, чтобы вкладки и workspace переживали запуск.

Выход: `shutdown_background_services()` (LSP, терминалы, installer, API, БД — с существующими таймаутами), затем удаление временного профиля, затем возврат кода.

### 4.3 Старт — `mod.rs::run`

1. Аргументы → `HeadlessOptions`; профиль (4.2).
2. `OffscreenContext::new(w, h)`; ошибка → stderr `headless: EGL setup failed at <stage>: <text>. Try RRITER_EGL_VENDOR=mesa`, код 3.
3. `Renderer::new(glow, scale, theme, requested_context)`; ошибка → код 3.
4. `App` создаётся тем же кодом, что в оконном режиме (текущий литерал в `main.rs:2235-2300` выносится в `App::new_from_config(config, …)` — рефакторинг без изменения поведения, чтобы headless и окно не расходились в инициализации), затем `window = Some(Arc::new(WindowHost::Headless(…)))`, `renderer = Some(renderer)`, `gl_config/gl_context/gl_surface = None`, `is_focused = true`.
5. `renderer.resize(w, h)`; позиционный путь → `open`/`workspace`.
6. `settle` (4.5) до первого стабильного кадра.
7. Цикл команд (4.4). Ответы — stdout, по строке на команду, `stdout.flush()` после каждой. Диагностика приложения (`eprintln!`) остаётся в stderr.

### 4.4 Протокол — `protocol.rs`

Строчный, UTF-8. Одна строка — одна команда; пустые строки и строки с `#` в первой непробельной позиции игнорируются. Ответ — ровно одна строка: `ok` или `ok <payload>` или `err <причина>`. Payload без переводов строк (JSON — компактный). Аргументы разделяются пробелами; последний аргумент команд `open`, `workspace`, `screenshot`, `dump`, `record`, `type` — «остаток строки» (пути с пробелами без кавычек).

| Команда | Действие | Ответ |
|---|---|---|
| `open <path>` | `open_file_in_tab(PathBuf)` тем же путём, что drag&drop файла (`events.rs:544-551`). Не существует / каталог / не читается → `err`. | `ok tabs=<n> active=<i>` |
| `workspace <dir>` | `apply_selected_workspace_folder` (как drop каталога): включает IDE-режим, watcher, дерево. Не каталог → `err`. | `ok` |
| `resize WxH` | `OffscreenContext::resize`, `HeadlessWindow.set_size`, `renderer.resize`, затем тот же код, что `WindowEvent::Resized` (`events.rs:506-533`) минус `gl_surface.resize`. Границы как у `--size`. | `ok <w>x<h>` |
| `scale S` | `HeadlessWindow.set_scale_factor`, `renderer.update_scale_factor`, как `ScaleFactorChanged` (`events.rs:481-504`). Диапазон 0.5–4.0. | `ok` |
| `mouse_move x y` | `handle_main_cursor_moved(PhysicalPosition{x,y})`. Координаты — физические пиксели, f64; вне буфера допустимы (как у окна). | `ok` |
| `click [left\|right\|middle] [down\|up]` | Без `down\|up`: `handle_main_mouse_input(host, Pressed, btn)` → кадр → `(Released, btn)`. Кнопка по умолчанию `left`. | `ok` |
| `dblclick [btn]` | Два `click` подряд без паузы (double-click в App детектится по времени между нажатиями). | `ok` |
| `wheel dx dy [lines\|px]` | `handle_main_mouse_wheel(MouseScrollDelta::LineDelta(dx,dy))` или `PixelDelta`. По умолчанию `lines`. | `ok` |
| `key <combo>` | `KeyInput::parse_combo` → `App.modifiers = mods` → `handle_main_key_input(host, Pressed)` → кадр → `(Released)` → `App.modifiers` восстанавливается. Мусорный combo → `err unknown key token '<t>'`. | `ok` |
| `type <text>` | `handle_main_ime_commit(text)` — как IME-ввод. Escape-последовательности `\n`, `\t`, `\\`. | `ok` |
| `settle [ms]` | Раздел 4.5. По умолчанию 500 мс. | `ok frames=<n> settled=<bool>` |
| `wait <ms>` | Крутит кадры реальное время `ms` (для анимаций и фоновых потоков: LSP, git, watcher). Максимум 60000. | `ok frames=<n>` |
| `screenshot <out.png>` | Один кадр (без `settle`), readback (4.7), запись PNG. Каталог создаётся. | `ok <abs path> <w>x<h>` |
| `dump [out.json]` | JSON состояния (4.6). Без файла — в payload. | `ok <json>` / `ok <abs path>` |
| `dialog save\|discard\|cancel` | Только при `headless_dialog_open`: вызывает тот же метод, что клик по кнопке. Иначе `err no dialog`. | `ok` |
| `bench <frames> [action…]` | Раздел 4.8. | `ok <json summary>` |
| `record <dir> <frames> [action…]` | Раздел 4.8. | `ok <json summary>` |
| `hz` | Определённая частота монитора и бюджет. | `ok hz=<f> budget_ms=<f> source=<arg\|monitor\|default>` |
| `quit` | Завершение цикла. | `ok` |

Правила:

- После каждой команды ввода (`mouse_move`, `click`, `dblclick`, `wheel`, `key`, `type`, `open`, `workspace`, `resize`, `scale`, `dialog`) выполняется один кадр (4.5, шаг «кадр»), чтобы состояние hover/registry соответствовало следующей команде. Полный `settle` — только по явной команде: команда ввода не должна ждать окончания анимаций скролла.
- Неизвестная команда → `err unknown command '<name>'`. Неверное число аргументов / нечисловое значение / не-UTF-8 строка (stdin читается как байты, `from_utf8` с ошибкой → `err invalid utf-8`) → `err`. Процесс продолжает работу.
- `--script`: после `err` выполнение продолжается, итоговый код 1. Это даёт тестам полный список ошибок за один прогон.
- EOF stdin или файла → как `quit`.
- Экранирование: аргументы-пути берутся как остаток строки, начиная с первого непробельного символа после имени команды и предыдущих аргументов; для `type` то же самое, поэтому ведущие пробелы текста передать нельзя (задокументированное ограничение).

Негативные случаи, обязательные для тестов парсера: пустая строка, только пробелы, `#` комментарий, `click banana`, `mouse_move 10`, `mouse_move a b`, `wheel 0 -3 furlongs`, `resize 10x10` (ниже минимума), `key ctrl+`, `key ctrl+foo`, `type` без аргумента (пустой текст — допустим, `ok`), байты `\xff\xfe` в строке, строка длиной 1 МБ (принимается, `err unknown command`).

### 4.5 Кадр и `settle` — `frame.rs`

```
fn step_frame(app, host) -> FrameStats {
    about::about_to_wait(app, host)               // mpsc-приёмники, анимации, needs_redraw → request_redraw
    if headless_window.take_redraw_request() || force {
        outcome = app.render_main_frame()          // тот же код, что окно
        if app.headless_dialog_open { draw_headless_dialog(app) }   // 4.6
        gl.finish()                                // кадр завершён, пиксели готовы
        if outcome.wants_another_frame { headless_window.request_redraw() }
    }
}
```

- `step_frame` не спит: pbuffer без vsync, `set_control_flow` в `HostLoop::headless` — no-op.
- `settle [ms]`: цикл `step_frame` до состояния «два кадра подряд без `redraw_requested` и `about_to_wait` вернул план `Wait` без `WaitUntil`» либо до истечения `ms`. Между кадрами — `thread::yield_now()`, чтобы фоновые потоки (highlighter, LSP) успевали ответить; если план — `WaitUntil(t)`, спим до `t` (не дольше остатка бюджета). Ответ `settled=false` при тайм-ауте — не ошибка: мигание курсора ставит `WaitUntil`, поэтому в headless мигание отключено флагом `platform::is_headless()` в `about.rs:1612-1618` (курсор всегда видим, `blink_alpha = 1.0`).
- Команда ввода → `step_frame(force = true)` ровно один раз.
- Все `Instant`-анимации идут по реальному времени: `wait <ms>` — единственный способ дать им пройти; `record` (4.8) снимает их покадрово.

### 4.6 Диалог подтверждения и `dump` — `dump.rs`

Диалог: при `headless_dialog_open` после основного кадра вызывается `renderer.draw_dialog_window(&base_title)` в том же буфере — в левом верхнем углу, область 660·s × 260·s; перед ним `renderer.resize(660·s, 260·s)`, после — `renderer.resize(w, h)`, ровно как оконная ветка `events.rs:378-427` делает для второй поверхности, только без `make_current`/`swap_buffers`. Флаг `dialog_window_open` в `Renderer::draw` = `dialog_window.is_some() || headless_dialog_open` — фон затемняется как в окне. Блокировки ввода в `mouse/input.rs:1644`, `cursor.rs:212`, `wheel.rs:768` получают тот же объединённый предикат `App::modal_dialog_open()`. Кнопки в `dump`: `widgets::get_dialog_buttons(0,0,660·s,260·s,s,renderer)` → три прямоугольника с именами `save|discard|cancel`. Команда `dialog <answer>` зовёт `begin_pending_action_save` / `discard_pending_action_changes` / `cancel_pending_action` и снимает `headless_dialog_open`. `Escape` через `key escape` работает тем же путём, что `main_keys.rs:297-310` (условие расширяется предикатом). Клик по координатам кнопки диалога в headless не поддерживается — только команда `dialog` (в окне клик идёт в отдельное окно, здесь — иная система координат).

`dump` — компактный JSON, сериализация через `serde_json::json!`/`Value` (без `derive` на `App`):

```json
{
  "size": [1920, 1080], "scale": 1.0, "cursor_icon": "Default",
  "mode": "ide" | "editor" | "welcome",
  "tabs": [{"index":0, "path":"/abs/or/null", "title":"main.rs", "active":true, "modified":false,
            "cursor":{"line":12,"col":4}, "scroll_y":0.0, "scroll_x":0.0, "markdown":false}],
  "editor": {"lines": 424, "selection": {"start":[l,c],"end":[l,c]} | null},
  "ide_panel": {"active": "explorer|git|database|api|terminal|search|lsp|none", "width": 300.0},
  "overlays": {"settings":false, "search":false, "welcome":false, "file_tree_dialog":"create|rename|delete|move|null",
               "context_menu": false, "lsp_actions_menu": false, "readonly_notice": false, "inline_git_popup": false},
  "dialog": null | {"action":"Quit|CloseTab|CloseFile|CloseAllTabs|OpenFile", "title":"…",
                    "buttons":[{"name":"save","rect":[x,y,w,h]}, …]},
  "picker_requested": null | "file|files|folder|save",
  "hover": {"ui": "UiId debug string" | null, "popup": true|false},
  "ui": [{"id":"IdeTabExplorer", "kind":"IconButton|Button|TextInput|Rect", "rect":[x,y,w,h], "overlay":false}, …]
}
```

- `ui` — `ui_registry.elements` последнего кадра, `id` = `format!("{:?}", id)` (у `UiId` есть `Debug`; 393 варианта, включая индексные `WelcomeRecentFile(3)`), `rect` — прямоугольник хит-теста (для `IconButton` — `icon_hit_rect`), `overlay` — элемент после `overlay_mark`. Порядок — порядок регистрации. Это даёт координаты для `mouse_move`/`click`.
- Поля берутся из `App`/`Renderer` по именам из разведки (`tabs`, `active_tab`, `editor.cursor`, `scroll_y`, `ide_panel`, `show_*`, `file_tree_*_dialog`, `pending_action`, `HOVER_STATE`). Точные имена вложенных полей `IdePanelState` уточняет исполнитель по коду; спека фиксирует состав ключей выше.
- JSON — одна строка; числа с плавающей точкой — как есть (без округления), координаты — в физических пикселях, как у `mouse_move`.

### 4.7 Снимок — `capture.rs`

`gl.read_pixels(0, 0, w, h, RGBA, UNSIGNED_BYTE, PixelPackData::Slice(&mut buf))` в переиспользуемый `Vec<u8>` (ёмкость `w*h*4`, `clear()` между снимками), переворот строк по Y на месте, `image::RgbaImage::from_raw` → `save_with_format(PNG)`. `gl.pixel_store_i32(PACK_ALIGNMENT, 1)` перед чтением. Перед `read_pixels` — `gl.finish()` (уже сделан в `step_frame`). Альфа принудительно 255 (фон непрозрачный, но pbuffer-конфиг может дать alpha 0 там, где ничего не рисовалось).

Подтверждённые ограничения: PNG — sRGB без цветового профиля; размер файла 1920x1080 ≈ 100–300 КБ.

### 4.8 Замеры — `bench.rs`

Бюджет кадра: `--budget-ms F` или `--hz N` → `1000/N`; иначе частота монитора: `winit::event_loop::EventLoop::builder().build()` → `available_monitors()` → максимум `refresh_rate_millihertz()` по мониторам → `1000/(mHz/1000)`; создание `EventLoop` на Wayland — подключение к композитору без поверхности, окон не создаёт. Если `EventLoop` не создаётся (нет дисплея) или частоты нет — 240 Гц и `source=default`. Определяется лениво при первом `bench`/`record`/`hz`, результат кэшируется. `EventLoop` уничтожается сразу после опроса (winit допускает один `EventLoop` на процесс — headless свой не создаёт, поэтому конфликта нет).

`bench <frames> [action…]`, `action` ∈ `none` (по умолчанию) | `wheel <dx> <dy>` (LineDelta каждый кадр) | `key <combo>` (нажатие каждый кадр) | `type <text>` (каждый кадр). Каждый кадр: применить action → `step_frame(force=true)` с замерами → записать строку. Замеры на кадр:

| Поле | Как |
|---|---|
| `frame` | индекс |
| `update_ms` | `about_to_wait` + подача action (CPU, `Instant`) |
| `draw_cpu_ms` | `render_main_frame` до `gl.finish()` |
| `gpu_ms` | `GL_TIME_ELAPSED` timer query (`glow` `begin_query/end_query/get_query_parameter_u64`), запрос вокруг `render_main_frame` + `finish`; если `ARB_timer_query` отсутствует — `null` |
| `total_ms` | update + draw_cpu + ожидание `finish` |
| `flush_calls`, `vertices` | из `Telemetry.flush_count`/`flush_vertices` за кадр |
| `root_phase_ms[5]` | `Telemetry.root_phase_time` (Prep, Cache, Pre-editor, Overlays, Chrome) |
| `chrome_ms[6]` | `Telemetry.chrome_detail_time` |
| `scroll_y` | `app.scroll_y` (для проверки, что action действует) |

Для этого `root_helpers.rs` получает `pub fn take_frame_telemetry() -> FrameTelemetry` — снимает и обнуляет накопители `Telemetry` (существующие поля, новых счётчиков в рендерере не появляется); `TELEMETRY_ENABLED` ставится в `true` на время `bench`/`record` и возвращается после. Периодическая печать в stdout под `is_headless()` не выполняется.

Сводка (payload `ok`): `{"frames":N, "budget_ms":4.17, "hz":240, "hz_source":"monitor", "total_ms":{"p50":..,"p95":..,"p99":..,"max":..}, "gpu_ms":{...}|null, "draw_cpu_ms":{...}, "over_budget":k, "worst":[{"frame":i,"total_ms":..,"update_ms":..,"draw_cpu_ms":..,"gpu_ms":..,"root_phase_ms":[..]}×5], "system":{"loadavg_before":[a,b,c],"loadavg_after":[..],"cpus":n,"gpu_util_before":u|null,"gpu_util_after":u|null,"process_cpu_ms":..}, "csv":"<abs path>"}`. CSV со всеми кадрами пишется в `<profile>/bench-<timestamp>.csv` (или рядом с `record`-каталогом). `gpu_util` — `nvidia-smi --query-gpu=utilization.gpu --format=csv,noheader,nounits` через `platform::run_command_output` с таймаутом 2 с; нет бинарника — `null`. `process_cpu_ms` — utime+stime из `/proc/self/stat` за время бенча.

`record <dir> <frames> [action…]` — то же, плюс на каждый кадр PNG `<dir>/frame-%04d.png` и в CSV колонки `scroll_y`, `sticky_anim_progress`, `search_anim_y`, `tab_scroll` (поля анимаций из аргументов `Renderer::draw`). Сводка дополнительно содержит `"motion":{"scroll_y_deltas":{"min":..,"max":..,"stddev":..},"nonmonotonic_frames":k}` для action `wheel`. Запись PNG — после завершения всех кадров из накопленных буферов (`frames × w × h × 4` байт; при `frames × w × h × 4 > 2 ГиБ` → `err too many frames for size`), чтобы кодирование PNG не попадало в замеры.

Интерпретация (в документации `docs/headless.md`): CPU-фазы — стоимость приложения, если `loadavg` ниже числа ядер; `gpu_ms` под игрой загрязнён — повторить бенч, минимум по прогонам — истинная стоимость; `RRITER_EGL_VENDOR=mesa` даёт CPU-растеризацию без GPU игры, но её `gpu_ms` нерепрезентативен.

## 5. Обёртка `scripts/rriter-headless`

Python 3, только stdlib, исполняемый. Находит бинарник `target/x86_64-unknown-linux-gnu/release/rriter` (переопределение `RRITER_BIN`), при отсутствии — сообщение «run make fast» и код 2. Не собирает.

- `rriter-headless shot <file-or-dir> [--size WxH] [--scale S] [--out PNG] [--settle MS] [--profile-from-user]` — `open`/`workspace` → `settle` → `screenshot` → печатает абсолютный путь PNG (по умолчанию `/tmp/rriter-headless/<name>-<ts>.png`), как `shot-web`.
- `rriter-headless run <script> [опции headless]` — прогон файла, stdout процесса транслируется, код выхода — код процесса.
- `rriter-headless repl [опции]` — интерактивно: stdin пользователя → процесс, ответы → stdout.
- `rriter-headless bench <file> <frames> [action…] [--hz N]` — `open` → `settle` → `bench` → печатает сводку (payload JSON) и путь CSV.

Строки stdout процесса, не начинающиеся с `ok`/`err`, обёртка печатает в stderr с префиксом `app:`. AGENTS.md получает одну строку в раздел 0 или 2: «UI проверять headless: `scripts/rriter-headless shot <file>` печатает PNG, `dump`/`bench` — см. `docs/headless.md`». `docs/headless.md` — протокол, примеры, интерпретация бенча (≤ 200 строк). `PROJECT_GUIDE.md` §4 — новые файлы.

## 6. Ошибки и границы

- Нет `libEGL.so.1`, surfaceless не создался, GL < 3.3 → код 3 с этапом и `eglGetError`, подсказка `RRITER_EGL_VENDOR=mesa`.
- Паника внутри `App` при `panic=abort` завершает процесс; обёртка пробрасывает stderr и код. Это сигнал бага, а не ситуация для обработки.
- `settle` при бесконечной анимации → `settled=false`, не зависание.
- Команды после `quit` не читаются. Сигнал `SIGPIPE`/закрытый stdout → выход с кодом 0 через штатный shutdown.
- Фоновые процессы (LSP при `open *.py`, терминал при панели Terminal в профиле) поднимаются как в окне и гасятся `shutdown_background_services` с существующими таймаутами; headless ничего нового не спавнит.
- Один `--headless` процесс может работать параллельно с живым редактором и с другими headless: общих файлов нет (свой XDG-корень), single-instance отсутствует и не добавляется.
- Размер pbuffer ограничен драйвером (`EGL_MAX_PBUFFER_WIDTH/HEIGHT`); `resize` сверх лимита → `err`.

## 7. Тестирование

Характеризационные тесты до рефакторингов (пишутся первыми, проходят на текущем коде и после):

- Существующие `reviewer_stage2_integration` и все `markdown_scroll_*` тесты — без изменения ассертов после переноса `OffscreenContext` и перевода на `HostLoop`.
- `KeyInput::from(&KeyEvent)` не тестируется напрямую (KeyEvent не конструируется), вместо этого — таблица `parse_combo` (каждый класс токенов + негатив: `ctrl+`, `+`, `ctrl+foo`, пустая строка, `ctrl+ctrl`) и тест, что `handle_main_key_input` c `KeyInput{Enter}` вставляет перевод строки в `test_app()` с offscreen-рендерером.
- `WindowHost::Headless`: `inner_size`/`scale_factor` возвращают заданное, `request_redraw` ставит флаг, `take_redraw_request` снимает, `request_inner_size` меняет размер.

Юнит-тесты headless: парсер протокола на каждой команде и на списке негативов из 4.4; `dump` — валидный JSON с обязательными ключами на `test_app()` с offscreen-рендерером; `capture` — переворот по Y на буфере 2x2; бюджет: `--hz 144` → 6.94, оба флага → ошибка аргументов, дефолт 240 при недоступном мониторе (мокается функцией-параметром).

Интеграционные (Linux, offscreen, `#[cfg(all(test, target_os = "linux"))]`, в `src/headless/tests.rs`): `open <tmpfile>` + `screenshot` → PNG нужного размера, есть пиксели цвета фона и пиксели не цвета фона; `dump` после `open` содержит вкладку с путём; `click` по `rect` элемента из `dump` (например, кнопка настроек) меняет `overlays.settings`; `key ctrl+q` на welcome ставит `exit_requested`; `bench 5` → 5 строк CSV, сводка с `frames=5`; `--script` с одной ошибочной командой → код 1, остальные команды выполнены.

Ручная приёмка: `scripts/rriter-headless shot src/main.rs` печатает путь, снимок открывается; `rriter-headless bench src/main.rs 480 wheel 0 -3` даёт сводку с `hz_source=monitor` и `hz=240` на машине пользователя; `record` на 60 кадрах скролла даёт монотонные `scroll_y`.

`make codex_test` в конце — по правилам AGENTS.md.

## 8. Файлы

| Файл | Действие |
|---|---|
| `src/platform/window_host.rs` | новый: `WindowHost`, `HeadlessWindow` |
| `src/platform/offscreen_gl.rs` | новый: перенос `OffscreenContext` |
| `src/platform.rs` | `pub mod` для двух новых, `is_headless()/set_headless()`, `headless_picker_requests`, гейт в `pick_*` |
| `src/app/keyboard/key_input.rs` | новый: `KeyInput`, `parse_combo` |
| `src/app/events/host_loop.rs` | новый: `HostLoop` |
| `src/app/app_state.rs` | тип `window`, поле `headless_dialog_open`, `modal_dialog_open()` |
| `src/app/events.rs` | `render_main_frame`, `HostLoop` в диспетчере, `id()` |
| `src/app/events/about.rs` | `&HostLoop`, гейт мигания |
| `src/app/events/window_runtime.rs` | `WindowHost::Native`, `native()` для surface |
| `src/app/app_window_external_methods.rs` | `show_action_dialog(host)`, `update_window_title(&WindowHost)` |
| `src/app/mouse/input.rs`, `src/app/keyboard/main_keys.rs`, `src/app/keyboard/editor_keys.rs`, `src/app/keyboard.rs`, `src/app/file_tree_dialog.rs`, `src/app/database/database_table_edit_methods.rs`, `src/app/database/database_app_methods.rs`, `src/app/api_client/api_client_app_request_methods.rs` | `KeyInput`, `HostLoop` |
| `src/app/automation.rs`, `src/app/terminal_process.rs`, `src/app/tool_installer.rs` | тип окна |
| `src/render_view/root_helpers.rs`, `root_frame_overlay_helpers.rs` | `take_frame_telemetry`, гейт печати |
| `src/render_view/markdown_scroll_transition_review_tests.rs` | использовать `platform::offscreen_gl` |
| `src/main.rs` | `--headless`, `App::new_from_config`, вызов `headless::run` |
| `src/headless/{mod,protocol,frame,capture,dump,bench,profile,tests}.rs` | новые |
| `scripts/rriter-headless` | новый |
| `docs/headless.md`, `AGENTS.md`, `PROJECT_GUIDE.md` §4 | документация |

Зависимости: новых крейтов нет (`image`+`png`, `serde_json`, `glow`, `winit`, `libc` уже в дереве).
