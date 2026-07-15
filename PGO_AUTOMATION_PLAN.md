# План автоматизации PGO и унификации build-скриптов RRiter

## 1. Статус документа

Этап сравнения baseline/PGO был реализован ранее. Этапы автоматизации PGO и
интерактивных platform build-скриптов теперь также имеют рабочую реализацию:

- `src/app/automation.rs` запускает детерминированный сценарий внутри настоящего
  event loop RRiter и управляет интерфейсом через `UiId` и обычные обработчики
  ввода;
- сценарий использует настоящий renderer/window, IDE workspace, вкладки,
  редактирование и сохранение, поиск, прокрутку и minimap, Project Search, Git,
  OpenAPI/API Client, контекстные меню, LSP/Problems, terminal и settings;
- `scripts/pgo_pipeline.py` создаёт изолированный fixture и config root, собирает
  instrumented binary в отдельном Cargo target, запускает GUI-тренировку,
  проверяет отчёт и `.profraw`, делает merge, сохраняет manifest/summary и только
  затем выполняет `profile-use` build;
- обычный `make fast`, instrumented PGO и PGO-use больше не делят один target
  cache;
- `scripts/build_windows.py` и `scripts/build_macos.py` при запуске без аргументов
  в TTY открывают пошаговое меню; CLI-режим для CI и старых команд сохранён;
- общая immutable-модель `BuildPlan` и проверки конфликтов находятся в
  `scripts/build_common.py`;
- свежий и повторно используемый PGO-профиль доступны как из Makefile, так и из
  Windows/macOS build-скриптов.

Оставшиеся разделы документа сохраняются как архитектурное обоснование,
расширенный checklist покрытия и требования к native smoke/release validation.
Проверки настоящего Win32/AppKit окна, signing/notarization и Rosetta должны
выполняться на соответствующих ОС; Linux training требует активную Wayland
session и не откатывается молча на X11.

## 2. Цели автоматизации PGO

Автоматизация должна давать не просто любой `.profraw`, а устойчивый профиль, отражающий реальную работу редактора.

Главные цели:

- Запускать настоящий RRiter с настоящим platform window и OpenGL context.
- Работать на Linux Wayland, Windows и macOS.
- Не использовать координатные внешние макросы, AutoHotkey, xdotool или AppleScript как основной механизм.
- Управлять редактором из Rust-кода внутри RRiter.
- Проходить основные пользовательские сценарии, а не только скролл.
- Не трогать пользовательские конфиги, файлы, Git-репозитории и credentials.
- Всегда завершать редактор штатно, чтобы LLVM успел записать `.profraw`.
- Давать понятный отчёт: какие сценарии пройдены, пропущены или упали.
- Пересобирать финальный бинарник только после успешной проверки профиля.
- После PGO-сборки запускать baseline/PGO comparison из текущего изменения.

## 3. Основной архитектурный принцип

Нужен реальный event loop и реальный window, но управление должно идти через внутренние маршруты RRiter.

Причины:

- OS-level automation зависит от темы, DPI, раскладки, положения окна и скорости машины.
- Координаты быстро ломаются при изменении UI.
- Внешние клики не дают надёжных wait conditions.
- Внутренний сценарий может ждать конкретный `UiId`, состояние worker или завершение LSP.
- Внутренний сценарий может использовать те же input/action handlers, что обычный пользователь.
- Реальный renderer, GPU, swap, text shaping и UI draw path всё равно будут выполняться.

Предлагаемая схема:

```text
build script
  -> instrumented RRiter
  -> --pgo-train <scenario-set>
  -> real event loop + real native window
  -> AutomationController
  -> normal mouse/keyboard/UI action routes
  -> graceful close
  -> *.profraw
  -> llvm-profdata merge
  -> profile validation
  -> PGO release build
  -> baseline/PGO comparison
```

## 4. Новые компоненты PGO automation

### 4.1 `AutomationController`

Добавить отдельное состояние автоматизации в `App`.

Рекомендуемая ответственность:

- Текущий scenario.
- Текущий step.
- Deadline step и всего run.
- Число frame после изменения UI.
- Последний найденный `UiId` и его bounds.
- Очередь ожидаемых async results.
- Лог выполненных шагов.
- Итоговый status.
- Запрос штатного закрытия окна.

Контроллер не должен работать в обычном запуске.

Активация только через явный CLI-флаг:

```text
--pgo-train
```

или:

```text
--automation-scenario pgo-full
```

В release-сборке без флага overhead должен быть практически нулевым:

- `Option<AutomationController>` в состоянии;
- одна дешёвая проверка в frame/event path;
- никакой фоновой thread и polling без активного сценария.

### 4.2 `AutomationStep`

Сценарии оформить как Rust enum, а не свободный JSON со строковыми командами.

Пример состава enum:

```text
WaitForWindowReady
WaitForUi(UiId)
ClickUi(UiId)
RightClickUi(UiId)
MovePointerToUi(UiId)
Wheel { target, delta }
Shortcut(AutomationShortcut)
TypeText(Arc<str>)
PressKey(AutomationKey)
ResizeWindow { width, height }
SetScaleScenario(...)
WaitForHighlightIdle
WaitForProjectSearchDone
WaitForGitRefreshDone
WaitForLspState(...)
WaitForTerminalPrompt(...)
Assert(...)
Repeat { count, steps }
CloseGracefully
```

Преимущества enum:

- compile-time coverage;
- невозможность опечатки в названии действия;
- простой unit-test;
- явная platform availability;
- удобная статистика покрытия шагов;
- отсутствие парсинга и allocations в frame loop.

### 4.3 Поиск элементов через `UiRegistry`

Для кнопок и menu items не хранить координаты.

Алгоритм:

1. Дождаться frame, где нужный `UiId` зарегистрирован.
2. Получить bounds из `UiRegistry`.
3. Взять центр bounds.
4. Передать pointer move через обычный mouse route.
5. Передать click/release через обычный input route.
6. Дождаться ожидаемого изменения состояния.

Это профилирует:

- UI registration;
- hit-test;
- hover state;
- mouse routing;
- action handler;
- последующий render.

Если нужная область пока не использует `UiId`, сначала нужно решить, должна ли она быть переведена на declarative registry. Не добавлять отдельные automation-only hitboxes.

### 4.4 Текстовый ввод

Текст должен идти через тот же путь, что обычный ввод.

Нужно покрыть два режима:

- character/IME commit path;
- key/shortcut path.

Сценарий обязан проверять:

- вставку ASCII;
- кириллицу;
- Unicode;
- многострочную вставку;
- backspace/delete;
- selection replacement;
- undo/redo;
- clipboard paste через controlled clipboard backend.

Нельзя напрямую вызывать `Editor::insert_str` для всех шагов: это не прогонит keyboard routing, autocomplete и redraw scheduling.

Прямой editor API допустим только при подготовке fixture до начала измеряемого сценария.

### 4.5 Wait conditions вместо sleep

Основное правило: не строить сценарий на фиксированных задержках.

Допустимые ожидания:

- window создан;
- renderer создан;
- минимум N frame представлен;
- `UiId` появился;
- panel open state изменился;
- async generation совпала;
- project search worker прислал `Done`;
- Git refresh завершён;
- LSP server готов или стабильно `Missing`;
- highlighter обработал нужную revision;
- terminal получил prompt marker;
- API request завершён;
- animation достигла epsilon.

Каждый wait имеет timeout и понятное сообщение.

Пример ошибки:

```text
scenario=project-search step=17 wait=ProjectSearchDone timeout=8s generation=4
```

### 4.6 Управление временем

PGO должен видеть реальную animation/render нагрузку, поэтому нельзя полностью заменять время fake clock.

Рекомендуется два режима:

- Deterministic logic mode для unit-tests контроллера.
- Real-time training mode для настоящего PGO run.

В training mode:

- frame cadence реальная;
- scroll impulses повторяемые;
- seed фиксированный;
- число операций фиксированное;
- async waits condition-based.

## 5. Изолированное окружение

Каждый PGO run должен использовать новый temporary root.

Linux:

```text
HOME
XDG_CONFIG_HOME
XDG_DATA_HOME
XDG_CACHE_HOME
XDG_STATE_HOME
```

Windows:

```text
USERPROFILE
APPDATA
LOCALAPPDATA
```

macOS:

```text
HOME
~/Library/Application Support/RRiter
~/Library/Caches/RRiter
```

Требования:

- Не копировать реальный пользовательский config.
- Не менять recent files пользователя.
- Не читать реальные credentials.
- Не открывать реальные рабочие проекты.
- Не оставлять terminal process после завершения.
- Удалять temporary root после успешного run.
- При ошибке сохранять root только при `--keep-failed-artifacts`.

## 6. Fixture workspace

Создать детерминированный workspace специально для automation.

Он должен содержать:

- Rust-файлы;
- Python-файлы;
- JSON/TOML/Markdown;
- большой файл для scroll/minimap;
- файл с diagnostics;
- файл с autocomplete cases;
- несколько nested folders;
- ignored folders;
- binary fixture;
- UTF-8 имена;
- пробелы в путях;
- длинный путь в пределах platform limit;
- symlink fixture только там, где поддерживается;
- line endings LF и CRLF;
- BOM/UTF-16 fixture для file format path;
- Git history с branch/merge/tag;
- staged, modified, untracked и renamed files.

Fixture не должен генерироваться случайно без seed.

Лучший вариант:

- компактный исходный fixture хранится в `tests/fixtures/pgo_workspace`;
- перед run он копируется во временную директорию;
- Git repository создаётся детерминированно через `git2`, без shell string;
- timestamps нормализуются;
- generated large files создаются локально до запуска RRiter.

## 7. Полный набор PGO-сценариев

### 7.1 Startup и window lifecycle

Покрыть:

- запуск без файла;
- запуск IDE mode;
- запуск с initial file;
- создание window;
- renderer init;
- первый frame;
- resize;
- maximize/restore, где безопасно;
- focus loss/gain;
- scale-factor path;
- graceful close.

Метрики и assertions:

- window ready;
- renderer ready;
- минимум 3 presented frame;
- отсутствие panic;
- штатный exit code;
- создан `.profraw`.

### 7.2 Tabs и editor core

Покрыть:

- открыть несколько файлов;
- переключение вкладок;
- close tab;
- reopen/restore path;
- split между IDE и single-file режимами, если применимо;
- ввод текста;
- delete/backspace;
- selection;
- double/triple click;
- word navigation;
- home/end/page navigation;
- undo/redo;
- copy/cut/paste;
- save;
- dirty state;
- external file change refresh.

Повторы нужны не одинаковые:

- короткие edits;
- большой paste;
- edits в начале, середине и конце gap buffer;
- edits рядом с Unicode boundaries.

### 7.3 Render, scroll и minimap

Расширить текущий `ScrollRenderBench`.

Покрыть:

- медленный wheel scroll;
- быстрый wheel burst;
- смена направления;
- horizontal scroll;
- drag scrollbar;
- minimap navigation;
- sticky headers;
- selection while scrolling;
- active diagnostics;
- autocomplete overlay;
- hover overlay;
- terminal visible;
- side panel visible;
- bottom panel visible;
- tabs overflow.

Нужны отдельные фазы:

1. Warm cache.
2. Continuous scroll.
3. Intermittent scroll.
4. Scroll + hover.
5. Scroll + selection.
6. Scroll + panel animation.

### 7.4 Search in file

Покрыть:

- открыть search;
- type query;
- next/previous;
- case sensitivity;
- replace one;
- replace all;
- no-result path;
- large result count;
- close search.

### 7.5 Project search

Покрыть:

- обычный substring;
- case-sensitive;
- include patterns;
- exclude patterns;
- ignored folders;
- result preview;
- scroll result list;
- click result;
- jump into file;
- repeated query;
- cancellation старого generation;
- capped result path.

### 7.6 File tree

Покрыть:

- expand/collapse;
- keyboard navigation;
- open file;
- rename;
- create file/folder;
- copy path;
- copy/move;
- delete/trash только внутри fixture;
- context menu;
- refresh после filesystem watcher event;
- long names;
- Unicode paths.

### 7.7 Git panel

Покрыть:

- status collection;
- graph collection;
- branch labels;
- staged/unstaged groups;
- open diff;
- stage/unstage;
- rollback только fixture-файла;
- commit dialog path без реального push;
- context menus;
- graph pagination;
- refresh.

Никаких network remote operations в PGO training.

### 7.8 Syntax highlighting и Tree-sitter

Покрыть несколько языков:

- Rust;
- Python;
- JavaScript/TypeScript;
- JSON;
- TOML;
- Bash;
- HTML/CSS;
- C/C++.

Для каждого:

- initial parse;
- incremental edit;
- multiline change;
- syntax error;
- fold calculation;
- large file.

### 7.9 LSP, Ruff и Ty

PGO run не должен зависать при отсутствии server.

Режимы:

- Tool available: пройти diagnostics, hover, completion, go-to-definition, code action.
- Tool missing: зафиксировать стабильный `Missing`, пройти UI disabled state и продолжить.
- Tool crashes: максимум bounded retry согласно существующей policy, затем stable error.

Для официального release PGO желательно заранее обеспечить managed Ty/Ruff в изолированном tool root.

Отчёт должен явно писать:

```text
lsp.ty=covered
lsp.ruff=covered
```

или:

```text
lsp.ty=skipped_missing
```

Нельзя молча считать missing path успешным полным покрытием.

### 7.10 Terminal

Покрыть:

- открыть terminal;
- resize PTY;
- input;
- большой output;
- ANSI colors;
- scrollback;
- selection/copy;
- clear;
- close;
- process-tree shutdown.

Команда должна быть platform-specific и детерминированной:

- Unix shell: встроенный fixture command;
- Windows: PowerShell или cmd через существующий platform resolver.

Не использовать shell interpolation пользовательских путей.

### 7.11 Settings

Покрыть:

- открыть/закрыть settings;
- scroll;
- theme/accent controls;
- telemetry control;
- tool path fields;
- graphics diagnostics;
- managed tool status UI;
- cancel/back.

Настройки пишутся только во временный config.

### 7.12 API client и API mock

Не обращаться во внешний интернет.

Поднять локальный loopback fixture server:

- GET JSON;
- POST JSON;
- gzip response;
- multipart upload;
- timeout/cancel;
- error response;
- OpenAPI load;
- API mock start/stop.

Покрыть:

- request editor;
- headers;
- auth UI без настоящего secret;
- body tabs;
- response rendering;
- history/default persistence во временном root.

### 7.13 Menus, dialogs и context menus

Все внутренние dropdown/context menu должны проходиться через `UiId`.

Для native file dialogs нужен abstraction seam:

- обычный режим вызывает native dialog;
- automation mode получает заранее подготовленный ответ из очереди;
- downstream open/save logic остаётся настоящим;
- отдельный platform smoke-test проверяет, что native dialog вообще открывается.

Полностью автоматический PGO run не должен зависеть от управления OS modal dialog.

### 7.14 Hover, autocomplete и diagnostics

Покрыть:

- hover request;
- hover bridge;
- diagnostic squiggle;
- popup scroll;
- autocomplete open/update/apply;
- detail request;
- keyboard и mouse selection;
- stale request cancellation.

## 8. Scenario weighting

PGO легко переобучить под один длинный сценарий.

Нужны веса, близкие к реальному использованию:

- editor typing/navigation: высокий вес;
- render/scroll: высокий вес;
- tabs/file tree/search: средний-высокий;
- project search/Git/terminal: средний;
- settings/dialogs: низкий;
- startup/shutdown: средний;
- редкие error paths: низкий, но ненулевой.

Не следует тысячу раз повторять только scroll: это может ухудшить layout функций для typing, project search и startup.

Предлагаемый первый набор повторов:

```text
startup/window            3
editor typing/navigation  12
scroll/render             10
file search               5
project search            4
file tree                 5
Git panel                 3
syntax/highlight           6
LSP/autocomplete           5
terminal                  3
API client/mock            2
settings/dialogs           2
shutdown                   3
```

Эти числа нужно корректировать по benchmark report, а не интуитивно.

## 9. Platform-specific запуск окна

### 9.1 Linux Wayland

Основной release PGO run:

```text
WINIT_UNIX_BACKEND=wayland
```

Проверки до запуска:

- `WAYLAND_DISPLAY` задан;
- connection доступен;
- EGL/Wayland context создаётся;
- software renderer не выбран случайно, если это не отдельный сценарий.

Опционально отдельный X11 profile run, если `linux-x11` реально поддерживается release-сборкой. Не смешивать X11 и Wayland случайно без отчёта.

### 9.2 Windows

Требования:

- настоящий Win32 window;
- MSVC target;
- корректный Job Object для child processes;
- отдельный temporary `APPDATA/LOCALAPPDATA`;
- stdout/stderr capture для automation report даже у GUI subsystem;
- DPI awareness остаётся включённой;
- test scenario минимум на 100%, 125%, 150% scaling, если runner может менять окно между monitor/DPI contexts без нестабильного system-wide изменения.

### 9.3 macOS

Требования:

- event loop и native dialogs остаются main-thread;
- OpenGL 4.1 Core;
- temporary `HOME` до запуска process;
- automation controller не вызывает AppKit из worker thread;
- arm64 и x86_64 profiles не смешиваются.

Для Universal 2:

- arm64 slice получает arm64 profile;
- x86_64 slice получает x86_64 profile;
- на Apple Silicon x86_64 training запускается под Rosetta;
- отсутствие profile для одной slice должно быть явным выбором, а не тихим reuse profile другой архитектуры.

## 10. Сбор и именование `.profraw`

Использовать уникальный шаблон:

```text
LLVM_PROFILE_FILE=<profile-dir>/<target>/<scenario>-%m-%p.profraw
```

Требования:

- отдельная директория на target triple;
- отдельное имя scenario;
- `%p` для process id;
- `%m` для module signature;
- никакого overwrite между child process;
- после каждого scenario проверять появление нового ненулевого файла.

После crash:

- сохранить stdout/stderr;
- сохранить последний step;
- не merge incomplete run в release profile по умолчанию.

## 11. Merge и проверка профиля

`llvm-profdata` брать из того же nightly toolchain, которым строился instrumented RRiter.

Pipeline:

1. Найти все ожидаемые `.profraw`.
2. Проверить размер > 0.
3. Проверить, что каждый обязательный scenario создал data.
4. Запустить `llvm-profdata merge`.
5. Запустить `llvm-profdata show --summary`.
6. Проверить, что profile содержит functions и counts.
7. Сохранить summary рядом с `merged.profdata`.
8. Только после этого запускать `profile-use` build.

При PGO build включить warning:

```text
-Cllvm-args=-pgo-warn-missing-function
```

Если доля missing functions резко выросла после изменения source, profile считать stale.

## 12. Fingerprint свежести

Каждый profile bundle должен иметь manifest.

Manifest включает:

- target triple;
- architecture;
- OS;
- rustc version/commit;
- Cargo.lock hash;
- Cargo.toml hash;
- source tree hash без target/vendor/.git;
- complete encoded Rust flags;
- build profile;
- automation scenario version;
- fixture version;
- timestamp;
- список completed scenarios;
- список skipped scenarios;
- llvm-profdata version;
- merged profile hash.

Default `--pgo` поведение по требованию свежих данных:

- всегда новый instrumented build;
- всегда новый training run;
- всегда новый merge;
- никогда не использовать старый profile молча.

Опциональный быстрый режим:

```text
--reuse-pgo-profile
```

Он допустим только при полном совпадении fingerprint.

## 13. Graceful shutdown и timeout

Завершение должно идти через обычный close path RRiter.

Порядок:

1. Scenario ставит `CloseGracefully`.
2. RRiter завершает pending state writes.
3. LSP/terminal/API mock children получают graceful shutdown.
4. Process trees закрываются bounded timeout.
5. Event loop выходит.
6. LLVM runtime flushes profile.
7. Parent проверяет exit code и `.profraw`.

Parent timeout:

- мягкий timeout на scenario;
- запрос controlled shutdown;
- короткий grace period;
- terminate complete process tree;
- run помечается failed;
- incomplete profile не merge.

## 14. Автоматический PGO pipeline

Предлагаемая конечная команда:

```text
python3 scripts/build_windows.py --pgo --test --package
```

или:

```text
python3 scripts/build_macos.py --pgo --test --arch native
```

Внутренние фазы:

```text
preflight
fixture prepare
instrumented build
automation training
profile validation
profile merge
baseline comparison build
PGO release build
baseline/PGO benchmark
unit/integration tests
package/sign/notarize
artifact report
```

Каждая фаза пишет status и duration.

## 15. Benchmark gate после training

Использовать текущие:

- `scripts/pgo_bench_build.rs`;
- `scripts/pgo_bench_compare.rs`.

Первый этап внедрения не должен автоматически отклонять build по каждому micro-metric: GPU scheduling и filesystem дают шум.

Рекомендуемый gate:

- correctness signatures обязаны совпасть;
- ни один обязательный scenario не упал;
- project search median не хуже baseline более чем на согласованный threshold;
- Git graph median не хуже threshold;
- scroll FPS не хуже threshold;
- flush/root hot metrics не имеют крупной регрессии;
- общий weighted score положительный;
- raw CSV сохраняется.

Перед включением жёсткого gate собрать минимум 20 исторических runs на каждой платформе и оценить variance.

## 16. Тесты PGO automation

### 16.1 Unit-tests

Покрыть:

- step transitions;
- timeout;
- wait conditions;
- repeat;
- skip policy;
- report serialization;
- scenario weights;
- profile filename;
- fingerprint;
- missing tool state;
- graceful close request.

### 16.2 Headless logic tests

Без window:

- проиграть controller на fake App state;
- проверить порядок шагов;
- проверить ошибки assertions;
- проверить отсутствие sleep-based dependency.

### 16.3 Real-window smoke tests

На каждой платформе:

- открыть window;
- дождаться renderer;
- открыть один menu;
- выполнить один edit;
- выполнить scroll;
- закрыть;
- получить `.profraw`.

### 16.4 Full training tests

Запускаются отдельно, не в каждом быстром unit-test run.

Проверки:

- весь scenario set завершён;
- нет orphan process;
- user config не изменён;
- fixture changes ограничены temp root;
- profile merge успешен;
- PGO build успешен;
- benchmark report создан.

## 17. Этапы реализации PGO automation

### Этап P1: инфраструктура контроллера

Изменения:

- CLI flag;
- `AutomationController`;
- basic step enum;
- real-window startup/close;
- report;
- timeout;
- unit-tests.

Acceptance:

- Linux Wayland, Windows, macOS открывают window, выполняют 3 шага и закрываются.

### Этап P2: UI registry actions

Изменения:

- wait/click по `UiId`;
- pointer move;
- context menu;
- shortcut/text input;
- assertions.

Acceptance:

- tabs, file tree, settings menu проходят без координат.

### Этап P3: editor/render scenarios

Изменения:

- typing/navigation;
- scroll/minimap;
- search;
- autocomplete/hover;
- telemetry markers.

Acceptance:

- текущий scroll benchmark полностью заменяется общим scenario runner, сохраняя отдельный compatibility mode.

### Этап P4: async subsystems

Изменения:

- project search;
- Git;
- highlighter;
- LSP;
- terminal;
- API client/mock.

Acceptance:

- wait conditions не используют arbitrary sleeps;
- missing tools дают documented skip.

### Этап P5: profile pipeline

Изменения:

- instrumented build;
- environment isolation;
- `.profraw` collection;
- merge;
- validation;
- fingerprint;
- profile-use build.

Acceptance:

- одна команда создаёт свежий профиль и финальный binary.

### Этап P6: benchmark gate

Изменения:

- автоматический запуск Rust compare tools;
- weighted report;
- variance-aware thresholds;
- artifact retention.

Acceptance:

- release report показывает реальный выигрыш/регрессии по аспектам.

# Часть II. План нового интерфейса build-скриптов Windows и macOS

## 18. Цели refactor build-скриптов

Текущие скрипты функциональны, но их интерфейс основан на множестве flags.

Новый интерфейс должен:

- При запуске без параметров открывать понятное интерактивное меню.
- Сохранять все текущие flags для CI и продвинутого использования.
- Не менять существующую package/sign/notarize реализацию без необходимости.
- Явно разделять tests, build, package, PGO и run.
- Перед стартом показывать итоговый plan.
- Не запускать дорогую фазу случайно.
- Работать только на Python standard library.
- Иметь ASCII fallback без ANSI/Unicode.
- Не зависать в CI при отсутствии TTY.

## 19. Правило запуска без параметров

Поведение:

```text
python scripts/build_windows.py
```

при interactive TTY:

- открыть menu.

```text
python scripts/build_macos.py
```

при interactive TTY:

- открыть menu.

При no args, но stdin/stdout не TTY:

- не ждать input;
- вывести краткую help;
- вернуть exit code 2;
- предложить явные flags.

Это сохраняет требование меню для обычного пользователя и защищает CI от hang.

## 20. Backward compatibility

Любой явный flag переводит script в non-interactive mode.

Примеры должны продолжить работать:

```text
scripts/build_windows.py --test --installer
scripts/build_windows.py --debug --no-package
scripts/build_macos.py --arch universal --test
scripts/build_macos.py --notary-profile RRiter
```

Новые flags:

```text
--menu
--tests-only
--build-only
--pgo
--reuse-pgo-profile
--no-benchmark
--yes
--print-plan
```

`--menu` принудительно открывает menu при TTY.

`--yes` разрешает non-interactive запуск без confirmation.

## 21. Общий `BuildPlan`

Меню не должно напрямую запускать subprocess.

Сначала choices преобразуются в immutable plan.

Предлагаемая модель:

```text
BuildPlan
  action
  platform
  target/architecture
  profile
  run_tests
  pgo_mode
  package_mode
  sign_mode
  notarize_mode
  install_targets
  launch_after_build
  keep_artifacts
  benchmark_mode
```

Преимущества:

- один executor для menu и CLI;
- self-test проверяет plan без реальной сборки;
- проще показать summary;
- проще исключить конфликтующие options;
- меньше ветвления внутри `main`.

## 22. Общий helper module

После реализации планируется вынести только реально общую часть.

Предлагаемые файлы:

```text
scripts/build_common.py
scripts/build_windows.py
scripts/build_macos.py
```

`build_common.py` отвечает за:

- TTY detection;
- ANSI capability;
- menu rendering;
- numbered choice;
- yes/no;
- back/cancel;
- `BuildPlan` base types;
- phase runner;
- duration;
- summary;
- safe command display;
- status icons/fallback;
- common PGO pipeline interface.

Platform scripts сохраняют:

- MSVC discovery;
- Windows resources;
- Inno Setup;
- signtool;
- macOS bundle;
- codesign;
- notarytool;
- DMG;
- architecture-specific logic.

Не переносить platform code в common module только ради сокращения строк.

## 23. Вид главного меню

Пример:

```text
RRiter Build — Windows

1. Только тесты
2. Быстрая локальная сборка
3. Release сборка
4. Release + тесты
5. PGO release из свежего профиля
6. Собрать portable ZIP
7. Собрать installer
8. Настроить параметры вручную
9. Показать доступные toolchains
0. Выход
```

macOS:

```text
RRiter Build — macOS

1. Только тесты
2. Native release
3. Native release + тесты
4. Universal 2
5. PGO native release из свежего профиля
6. Signed app + DMG
7. Signed + notarized DMG
8. Настроить параметры вручную
0. Выход
```

## 24. Advanced menu

Advanced flow должен быть пошаговым, а не одной стеной options.

Шаги:

1. Что сделать.
2. Target/architecture.
3. Debug/release/max.
4. Tests до build или tests only.
5. PGO mode.
6. Benchmark после PGO.
7. Package mode.
8. Signing/notarization.
9. Launch after build.
10. Summary + confirmation.

На каждом шаге:

- текущий выбор виден;
- `b` возвращает назад;
- `q` отменяет;
- default отмечен;
- invalid input не роняет script.

## 25. Красивый, но надёжный terminal UI

Использовать ANSI только если:

- stdout TTY;
- `NO_COLOR` не задан;
- terminal поддерживает color.

Windows:

- попытаться включить Virtual Terminal Processing;
- при ошибке использовать plain text.

Символы:

- Unicode icons только при совместимой encoding;
- ASCII fallback: `[OK]`, `[WARN]`, `[FAIL]`, `->`.

Никаких внешних TUI libraries.

## 26. Summary перед запуском

Перед дорогой операцией показать:

```text
Action:        Release build
Platform:      Windows x86_64-pc-windows-msvc
Tests:         yes
PGO:           fresh profile + full automation
Benchmark:     baseline vs PGO
Package:       portable ZIP + installer
Signing:       certificate SHA-1 from CLI/env
Launch:        no
```

Затем:

```text
Start? [Y/n]
```

Для presets `tests only` confirmation можно не требовать.

Для signing/notarization и full PGO confirmation обязательно, кроме `--yes`.

## 27. Windows-specific choices

Меню Windows должно поддержать:

- target triple;
- debug/release;
- tests only;
- tests before build;
- install missing target;
- normal build;
- fresh PGO build;
- reuse matching PGO profile;
- portable ZIP;
- installer;
- no package;
- PFX signing;
- certificate SHA-1 signing;
- timestamp URL;
- launch.

Validation до build:

- Windows 11;
- Visual Studio/MSVC environment;
- required SDK tools;
- Rust target;
- Inno Setup только если installer выбран;
- signtool/certificate только если signing выбран;
- PGO prerequisites только если PGO выбран.

Password нельзя показывать в summary или log.

## 28. macOS-specific choices

Меню macOS должно поддержать:

- native;
- arm64;
- x86_64;
- Universal 2;
- minimum system;
- debug/release;
- tests only;
- tests before build;
- install targets;
- fresh PGO;
- ad-hoc sign;
- Developer ID sign;
- hardened runtime;
- DMG;
- notarization profile;
- launch.

Validation:

- `xcrun`;
- `codesign`;
- `sips`;
- `iconutil`;
- `lipo` для Universal 2;
- notary profile только с Developer ID;
- hardened runtime обязателен для notarization;
- PGO profile target-specific.

## 29. PGO choice в меню

Варианты:

```text
1. Без PGO
2. Свежий PGO профиль — полный automation run
3. Использовать существующий профиль только при совпадении fingerprint
4. Только собрать instrumented binary
5. Только прогнать training на уже собранном binary
6. Только сравнить baseline и PGO
```

Default для обычной release-сборки: без PGO.

Default для пункта `PGO release`: свежий полный pipeline.

## 30. Tests-only path

Сейчас scripts связывают tests с build через `--test`.

Нужно добавить отдельный action:

```text
--tests-only
```

Он:

- проверяет platform/toolchain;
- запускает только platform test command;
- не готовит icons/resources/package;
- не создаёт release artifact;
- возвращает test exit code.

Это важно для быстрого локального контроля.

## 31. Build-only и package separation

Внутренние фазы сделать явными:

```text
prepare
optional tests
build
optional PGO training
optional benchmark
bundle/package
sign
notarize
launch
```

`--build-only` останавливается после executable/app bundle согласно platform semantics.

Package не должен неявно повторять build, если ему передан уже валидный artifact в рамках того же plan.

## 32. Phase runner

Каждая фаза получает:

- name;
- callable;
- required/optional;
- started_at;
- status;
- artifact list.

Вывод:

```text
[1/7] Preflight .......... OK   0.4s
[2/7] Tests .............. OK  31.2s
[3/7] PGO training ....... OK  58.7s
[4/7] PGO merge .......... OK   0.8s
[5/7] Release build ...... OK  42.1s
[6/7] Benchmark .......... OK  75.4s
[7/7] Package ............ OK   3.2s
```

При ошибке:

- показать failed phase;
- command;
- exit code;
- log path;
- не продолжать signing/package после failed build;
- не скрывать уже созданные artifacts.

## 33. Logging

Human output остаётся в terminal.

Дополнительно писать machine-readable summary:

```text
target/build-reports/<platform>-<timestamp>.json
```

Содержимое:

- BuildPlan без secret;
- tool versions;
- phase durations;
- result;
- artifacts;
- hashes;
- PGO manifest/report paths;
- test result.

Secrets и signing password не записывать.

## 34. Argument conflict validation

Примеры ошибок до запуска subprocess:

- `--tests-only` + package flags;
- `--debug` + release-only PGO policy, если debug PGO не поддерживается;
- `--no-package` + `--installer`;
- macOS notarization + ad-hoc identity;
- macOS notarization + disabled hardened runtime;
- Universal 2 PGO без двух target profiles;
- Windows signing options без package/build action;
- `--reuse-pgo-profile` без PGO.

Error должен предлагать точное исправление.

## 35. Self-tests build scripts

Расширить текущие `--self-test`.

Общие tests:

- no args + fake TTY открывает menu;
- no args + no TTY не зависает;
- explicit args обходят menu;
- каждый preset создаёт ожидаемый `BuildPlan`;
- back/cancel;
- invalid input;
- ASCII fallback;
- `NO_COLOR`;
- summary hides secrets;
- conflict validation;
- tests-only command;
- PGO phase order;
- failed phase stops pipeline.

Windows tests:

- existing MSVC capture tests сохранить;
- preset installer;
- signing plan;
- target install plan;
- portable-only plan.

macOS tests:

- existing plist/bundle tests сохранить;
- native/universal plan;
- PGO per-arch validation;
- notarization validation;
- no-DMG plan.

## 36. Этапы реализации build menu

### Этап B1: `BuildPlan`

- Выделить immutable plan.
- CLI parser создаёт plan.
- Старое поведение команд сохраняется.
- Self-tests сравнивают plan.

### Этап B2: phase executor

- Разделить tests/build/package/sign/run.
- Добавить tests-only.
- Добавить structured summary.

### Этап B3: common menu helper

- TTY detection.
- colors/fallback.
- choices/back/cancel.
- summary/confirmation.

### Этап B4: Windows menu

- presets;
- advanced choices;
- validation;
- self-tests.

### Этап B5: macOS menu

- presets;
- architecture flow;
- signing/notarization flow;
- self-tests.

### Этап B6: PGO integration

- fresh profile action;
- reuse fingerprint action;
- benchmark action;
- report paths.

## 37. Acceptance criteria всего проекта

PGO automation считается готовой, когда:

- одна команда запускает real RRiter window;
- Linux Wayland, Windows и macOS проходят smoke scenario;
- полный scenario set работает без внешних coordinate tools;
- пользовательский config не меняется;
- все child process закрываются;
- profile manifest создан;
- merge валиден;
- PGO build успешен;
- baseline/PGO CSV создан;
- correctness signatures совпадают.

Build menu считается готовым, когда:

- запуск без args в terminal открывает menu;
- CI без TTY не зависает;
- старые explicit flags работают;
- tests-only не строит artifact;
- build/test/PGO/package/sign phases выбираются явно;
- Windows и macOS `--self-test` проходят на любой host, где это предусмотрено текущим design;
- secrets не попадают в output/report;
- итоговый plan виден до запуска дорогой операции.

## 38. Рекомендуемый порядок следующей задачи

1. Реализовать P1 и P2: controller + `UiId` actions + real-window smoke.
2. Подключить editor/render сценарии и заменить ручной scroll-only training.
3. Добавить async subsystem scenarios.
4. Реализовать profile pipeline и fingerprint.
5. Стабилизировать benchmark variance.
6. Только затем refactor build scripts вокруг готового PGO API.
7. В конце включить menu presets `PGO release` на Windows/macOS.

Такой порядок не заставит build menu зависеть от временного или постоянно меняющегося PGO interface.
