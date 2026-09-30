# Headless PGO с полным покрытием и `make max` через PGO — дизайн

Дата: 2026-09-30. Статус: утверждён в brainstorming, ждёт review спеки.

## Цель

`make max` собирает финальный бинарь с PGO-профилем, снятым headless (без Wayland) сценарием,
который покрывает всю пользовательскую функциональность, включая добавленную недавно (PDF, API Mock,
git diff/commit, старт `--ide`). Покрытие проверяемо: после merge пайплайн печатает модули, в
которых не сработала ни одна функция, — следующий забытый функционал виден без ручной инвентаризации.

Критерии успеха:
1. `make max` на машине без `WAYLAND_DISPLAY` проходит целиком: instrumented-сборка → три
   headless-прогона → merge → `-Cprofile-use`-сборка → бинарь в `target/<triple>/release/rriter`.
2. Все функции-маркеры функциональных групп (раздел 3, «Покрытие») имеют ненулевой счётчик в
   merged-профиле; `target/pgo-profiles/coverage.txt` содержит сводку по модулям.
3. Предупреждения `-pgo-warn-missing-function` и hash mismatch («function control flow change
   detected») финальной сборки пайплайн считает и пишет в лог;
   функция горячего модуля (`render_view`, `renderer`, `editor*`, `highlighter*`, `scroll`,
   `app::mouse`, `app::keyboard`) среди них — дефект профиля, разбирается до приёмки.
4. `make pgo-bench-build` снова собирается (строка `--rustflag=-Clto=fat` удалена); запуск
   сравнения не требуется.
5. `make codex_test` зелёный.

## Решения пользователя (brainstorming, 30.09)

- Тренировка только headless. GUI-путь тренировки (Wayland-проверка, `WINIT_UNIX_BACKEND`, запуск
  `--ide --pgo-train` через winit) из `scripts/pgo_pipeline.py` удаляется.
- `make max` = свежий PGO каждый раз (instrumented-сборка, тренировка, финальная сборка); падает с
  понятной ошибкой, если тренировка не прошла, без тихого отката на сборку без профиля.
- Прежний `max` без профиля — цель `max-nopgo`.
- Итоговый бинарь копируется по старому пути `make max`: `target/<triple>/release/rriter`.
- Быстрый вариант со старым профилем — `make pgo-use` (режим `reuse` пайплайна), остаётся.
- `max` и все `pgo*` зависят от цели `pdfium`; нет библиотеки — ошибка, а не молча пропущенный PDF.
- Сводка покрытия по модулям после merge.
- `pgo-bench-*` остаются, чинится только `Makefile:221`; сравнение со старым бинарём не запускаем.
- Расхождение panic (instrumented `panic=abort`, use `immediate-abort`, `pgo_pipeline.py:435`)
  оставляем осознанно.
- Native pickers и protected save в сценарий не входят: диалоги ОС headless не открывает,
  protected save идёт через elevation-хелпер и редок.

## Архитектура

### 1. Headless-раннер тренировки (Rust)

Сейчас `AutomationController` тикается из `about::about_to_wait` (`src/app/events/about.rs:164-173`),
а headless-`step_frame` вызывает тот же `about_to_wait` (`src/headless/frame.rs:71-81`). От event
loop контроллер получает только `&HostLoop` (`Native | Headless`, `src/app/events/host_loop.rs:23-26`),
окно — `WindowHost::{Native,Headless}` (`src/platform/window_host.rs:7-10`). Значит, отдельного
контроллера не нужно — нужен вход и цикл.

Новый запуск:

```
rriter --headless --pgo-train --pgo-scenario <full|startup|welcome> \
       --pgo-workspace W --pgo-report R --pgo-timeout-seconds T --profile P
```

- Разбор флагов — `src/headless/profile.rs::parse_args` (рядом с `--allow-writes`, `:94`).
  `--pgo-train` в headless подразумевает `--allow-writes` (иначе `SaveCurrentFile` блокирует
  `headless_write_blocked`, `app_window_external_methods.rs:685-690`).
- `AppInitOptions::headless()` (`src/app/app_bootstrap.rs:84`) получает automation-опции вместо
  жёсткого `None`.
- Старт: как `HeadlessSession::workspace` (`src/headless/mod.rs:494-506`) — `enter_ide_mode_deferred`
  + `finish_startup`, иначе `WaitReady` (`automation_controller_steps.rs:11`) ждёт `is_ide_mode`
  вечно. Для сценария `welcome` IDE-режим не включается — стартует экран welcome, открытие проекта
  делает сам сценарий.
- Telemetry-флаг `--pgo-train` сейчас ставится в `main.rs:267-270`, после headless-ветки
  (`main.rs:137-139`); выставляется и для headless.
- Цикл с фиксированным темпом 60 кадров/с: `step_frame`, затем сон до следующего 16.67-мс дедлайна
  (кадр дольше дедлайна — следующий сразу, без догоняющей пачки). Automation держит `needs_redraw`
  постоянно (`about.rs:199`), каждый кадр кончается `glFinish` (`frame.rs:109`); без темпа число
  кадров, а значит вес рендера в профиле, зависело бы от скорости GPU и от того, сколько ждут
  git/LSP/БД. 60 Гц — как GUI с vsync; timed-скролл идёт по реальному времени
  (`automation.rs:512-530`), число его кадров совпадает с GUI. Выход по
  `loop_state.exit_requested` (`host_loop.rs:33-38`) или таймауту. `run_loop` со stdin
  (`mod.rs:325-357`) не используется.
- Окно по умолчанию 2560×1440, scale 1.333 (экран пользователя).
- `ResizeWindow` в headless меняет только размер `HeadlessWindow` (`window_host.rs:130-137`), GL
  pbuffer и renderer не пересоздаются (это делает команда `resize`, `mod.rs:524-535`). Починка
  общая: headless-кадр сверяет размер окна с `gl.size()` и вызывает тот же resize — чинит любой
  resize, пришедший не из команды драйвера.
- Выход: `std::process::exit(headless::run(..))` (`main.rs:139`) вызывает libc `exit`, atexit LLVM
  пишет `.profraw`. Успех и ошибка контроллера оба дают `AutomationTick::Exit`
  (`automation.rs:605`), поэтому раннер берёт результат из отчёта контроллера: успех — код 0,
  ошибка шага или таймаут — код 1 (atexit и `.profraw` при этом всё равно пишутся). Паника в
  instrumented-сборке (`panic=abort`) = потеря профиля; пайплайн проверяет код выхода и что
  `.profraw` не пустой, иначе падает с хвостом stderr. В отчёт каждого прогона пишется число кадров.
- Секреты: в headless системное хранилище секретов отключено (`secret_store.rs:44-58`). Шаг
  `SaveApiAuth` (`automation_controller_steps.rs:729-751`) идёт штатным путём headless-хранилища;
  если тот отказывает, шаг исключается из headless-сценария (с комментарием почему). Обходить
  хранилище файлом нельзя: это нарушает контракт `secret_store.rs:34`.

### 2. Сценарии

`SCENARIO_VERSION` 17 → 18 синхронно в `src/app/automation.rs:14` и `scripts/pgo_pipeline.py:38`.

`--pgo-scenario`:
- `full` — существующий `full_pgo_scenario` (`src/app/automation_fixtures.rs:158-390`) плюс новые
  группы ниже. Оставляет в profile-каталоге открытые вкладки для `startup`.
- `startup` — тот же profile-каталог: старт IDE с восстановлением вкладок
  (`src/app/app_ide_startup_methods.rs:89-103`), chrome-first кадр, ожидание подсветки активной
  вкладки, короткий скролл, выход.

Восстановление сессии под automation сейчас выключено с обеих сторон: загрузка сохранённых вкладок
запрещена (`app_ide_startup_methods.rs:89`), штатное сохранение при выходе пропускается
(`window_runtime.rs:354`). Это защищало пользовательское состояние при GUI-тренировке. В headless
profile-каталог изолирован, поэтому исключение делается по сценарию: `full` сохраняет сессию при
выходе штатным путём, `startup` штатно её загружает; `welcome` и прочая automation — как сейчас.
Путь сохранения и загрузки — тот же, что у пользователя, не отдельный.
- `welcome` — старт без IDE-режима на экране welcome, открытие workspace, `WaitReady`, выход.

Новые группы шагов `full` — каждая в своём файле `src/app/automation_<группа>.rs` по образцу
`automation_markdown.rs` / `automation_database.rs` (лимит 1600 строк; `automation_fixtures.rs` и
`automation_controller_steps.rs` не растут больше чем на строку вызова группы):

| Группа | Шаги | Горячий путь |
|---|---|---|
| PDF | фикстура `crate::pdf::fixture::write_fixture_pdf` (сейчас только `cfg(test)`, `pdf/mod.rs:4` — гейт снимается, модуль компилируется всегда) в workspace, открыть вкладку, дождаться растеризации, скролл, zoom, поиск, переход по страницам | `render_view/pdf_view.rs:6`, `pdf/worker.rs:39,179` |
| API Mock | создать mock, запустить сервер, запрос к нему, горячее обновление ответа, остановить | `app/api_mock*` |
| Git | изменить файл → diff-вид, скролл hunks, stage hunk, commit; удалить открытый файл → вкладка удалённого файла | `app/git_diff.rs:224`, `root_frame_helpers.rs:291` |
| Редактор | multi-cursor (Alt+click ×N, набор), goto definition (Python-фикстура), undo/redo, copy/paste | `editor_text_layer.rs:989-997` |
| Скролл вводом | wheel-события, PageDown/PageUp, перетаскивание скроллбара редактора | `app/mouse/wheel.rs`, `src/scroll.rs` |
| Терминал | вторая вкладка терминала, переключение, поиск по выводу | `app/terminal.rs` |

Для групп вводятся общие шаги сырого ввода: клавиша с модификаторами, wheel в точке, клик и
перетаскивание по `UiId` или координатам. Они вызывают `handle_main_key_input` /
`handle_main_mouse_input` (как headless-драйвер, `src/headless/mod.rs:359-436`), чтобы в профиль
попала маршрутизация событий, а не только прямые вызовы `App`. Существующие шаги не меняются.

Точные UI-пути (какой `UiId`, какая клавиша) берутся из работающих headless-тестов:
`ui_tests_pdf.rs`, `ui_tests_api_mock*.rs`, `ui_tests_git_diff.rs`, `ui_tests_git_commit.rs`,
`ui_tests_deleted_tab.rs`, `ui_tests_multi_cursor.rs`, `ui_tests_goto_definition.rs`,
`ui_tests_welcome.rs`, `ui_tests_ide_startup.rs`, `ui_tests_terminal.rs`, `ui_tests_editor.rs`,
`ui_tests_scrollbars_panels.rs`.

Каждый шаг ждёт результата условием (как `tests_support::wait_until`), а не фиксированной паузой,
кроме задержек по смыслу (hover dwell, анимация).

### 3. Пайплайн (`scripts/pgo_pipeline.py`)

- Удаляются: `validate_training_environment` с требованием `WAYLAND_DISPLAY` (`:1041-1055`, вызовы
  `:1396`, `:1417`), `WINIT_UNIX_BACKEND=wayland` (`:988-989`), GUI-argv тренировки (`:1098-1108`).
- Изолированное окружение (HOME/XDG, `LLVM_PROFILE_FILE`, `RRITER_PGO_AUTOMATION=1`) сохраняется;
  app-каталоги headless берёт из `--profile` (`src/headless/integration.rs:578-602`), каталог — внутри
  state-каталога тренировки.
- PostgreSQL-фикстура и `LocalApiServer` (`:1108-1125`) поднимаются один раз на все три прогона.
- `RRITER_PDFIUM_PATH` = содержимое `target/pdfium.path` (пишет `scripts/fetch_pdfium.py`); файла
  нет или путь не существует — ошибка с подсказкой `make pdfium`.
- Три прогона последовательно `full` → `startup` → `welcome`, каждый со своим `--pgo-report`;
  общий `LLVM_PROFILE_FILE`-шаблон с `%p`, все `.profraw` в один merge
  (`llvm-profdata merge` без `-sparse`, `:1252-1286`: `-sparse` выбрасывает нулевые записи, и
  непройденные функции в сводке не видны). Прогон упал или его `.profraw` пуст — пайплайн
  падает, не мержит частичный профиль.
- `--run-only --run-executable` (`make pgo-script`) гоняет те же три прогона headless на готовом
  бинаре без merge.
- Покрытие, два уровня. Источник — `llvm-profdata show --all-functions --counts` по merged-профилю,
  имена через `llvm-cxxfilt` (llvm-tools rustup, там же, где `llvm-profdata`; не найден — ошибка,
  как у `llvm-profdata`). Функция сработала, если у неё есть ненулевой счётчик; знаменатель —
  инструментированные функции, не все исходные.
  - Сводка по модулям (информационная) → `target/pgo-profiles/coverage.txt`: путь модуля из
    деманглированного имени (`rriter::a::b::…`; для `<rriter::a::T as …>::f` и `<rriter::a::T>::f` —
    модуль типа). Методы `impl App` из `include!`-файлов попадают в `rriter::app` — сводка
    грубая, в конце лога модули `rriter::*` с нулём сработавших функций.
  - Маркеры групп (обязательные): таблица в `pgo_pipeline.py` рядом с `SCENARIO_VERSION` —
    группа сценария → 1–3 подстроки деманглированных имён функций, которые выполняются только на
    пути этой группы (например, для PDF — функция отрисовки PDF-вкладки, для startup — функция
    восстановления вкладок). Маркер с нулевым счётчиком или не найденный в профиле — пайплайн
    падает с именем группы: сценарий молча перестал её проходить. Маркеры выбирает implementer по
    коду, по одному-три на каждую группу из таблицы сценария и на `startup`/`welcome`.
- Функция копирования итогового бинаря в `target/<triple>/release/rriter` (флаг пайплайна,
  включается целью `max`).

### 4. Makefile

- `max` → `pgo_pipeline.py --mode fresh` с флагами нынешнего `pgo-auto` (`Makefile:286-297`) +
  копирование бинаря; пререквизит `pdfium`.
- `max-nopgo` — нынешнее тело `max` (`Makefile:56-65`).
- `pgo`, `pgo-auto` → синонимы `max`. `pgo-train`, `pgo-use` остаются, получают пререквизит `pdfium`.
- `pgo-script`, `pgo-gen-fast` и ручные `pgo-gen`/`pgo-run`/`pgo-merge`/`pgo-max` (`:250-284`) —
  тренировка через headless; тексты `@echo` про GUI обновляются.
- `pgo-bench-build`: удалить `--rustflag=-Clto=fat` (`Makefile:221`); LTO уже задаёт
  `scripts/pgo_bench_build.rs:270`. Остальные `--rustflag` нужны (первый `--rustflag` сбрасывает
  дефолты, `pgo_bench_build.rs:142-146`).
- `bloat-max`, `fast`, тестовые цели не меняются.

## Обработка ошибок

| Ситуация | Поведение |
|---|---|
| Нет pdfium | `make max` падает: «pdfium не найден, выполните `make pdfium`» |
| Нет EGL / headless не стартовал | прогон падает, пайплайн печатает хвост stderr и падает |
| Сценарий превысил таймаут | SIGTERM, затем SIGKILL на группу (как сейчас, `pgo_pipeline.py:258-296`), `.profraw` не мержится, ошибка |
| Шаг сценария не дождался условия | отчёт контроллера с именем шага, код выхода ≠ 0, пайплайн падает |
| `.profraw` пуст или отсутствует | ошибка с именем прогона |
| PostgreSQL/`LocalApiServer` не поднялись | как сейчас — ошибка до прогонов |
| Нет `llvm-cxxfilt` | ошибка до тренировки, как при отсутствии `llvm-profdata` |
| Маркер группы с нулём | пайплайн падает с именем группы и маркера, финальная сборка не идёт |
| Прогон завершился кодом 1 (ошибка шага) | пайплайн падает с именем шага из отчёта |

## Тестирование

- Новые шаги сырого ввода и раннер — headless-тест, запускающий короткий сценарий (несколько шагов
  каждой новой группы) через `--pgo-train` на тестовом бинаре и проверяющий код выхода и отчёт.
- Итерации над сценарием — `make pgo-script` (headless на быстром бинаре, без PGO-сборок).
- Resize pbuffer — headless-тест: `ResizeWindow` из сценария, затем `dump`/кадр нового размера.
- `pgo_pipeline.py`: существующий self-test, если есть, плюс проверка сборки argv и разбора
  `coverage.txt` на фикстурном выводе `llvm-profdata show`.
- Финал: `make max` целиком, `coverage.txt` по критерию 2, предупреждения
  `-pgo-warn-missing-function` по критерию 3, `make pgo-bench-build` собирается, `make codex_test`.

## Документация

`docs/headless.md` (режим `--pgo-train`), `PROJECT_GUIDE.md` §4 (новые файлы `automation_*.rs`),
комментарии целей в `Makefile`. `PGO_AUTOMATION_PLAN.md` — пометка, что GUI-тренировка заменена
headless.

## Вне scope

Native pickers, protected save, сравнение PGO со старым бинарём, изменение panic-стратегии сборок,
PGO для Windows/macOS.
