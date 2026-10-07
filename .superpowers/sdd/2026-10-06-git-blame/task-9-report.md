# Task 9 — Gutter line-number context menu

- Правый клик по области номеров строк открывает контекстное меню; hit-rect не включает колонку Git blame и не регистрируется в Git diff (`src/render_view/root_frame_editor_text_renderer.rs:427`, `src/app/git_blame.rs:72`).
- Состояние меню хранится в `GitBlameState`; пункт получает точный текст «Показать Git blame» / «Скрыть Git blame» и переключает колонку (`src/editor/git_blame_state.rs:24`, `src/editor/git_blame_state.rs:46`, `src/render_view/root_frame_overlay_helpers.rs:155`, `src/app/git_blame.rs:89`).
- Действие проходит через `GitBlameColumnMenuItem` и существующую маршрутизацию UI; рендер использует общий анимированный context-menu (`src/ui_system/ui_ids.rs:457`, `src/app/ui_handlers/ui_git.rs:215`, `src/render_view/root_frame_overlay_helpers.rs:157`).
- Notice выводится из `git_head`, `path_in_head`, `head_oid` и `failed_key`: тексты для файла вне репозитория, отсутствующего в HEAD и ошибки git заданы в `src/app/git_blame.rs:103`; асинхронная ошибка показывает соответствующий notice в `src/app/git_blame.rs:355`.
- Baseline пункта меню округляется в общем renderer (`src/render_view/ide_panels/ide_panel_dialog_renderer.rs:1136`).
- Добавлены тесты на оба текста/переключения, восстановление ширины gutter, отсутствие меню по правому клику на тексте и notice для untracked-файла (`src/headless/ui_tests_git_blame_column.rs:166`, `src/headless/ui_tests_git_blame_column.rs:212`).
- RED: красного прогона не было; первый и финальный прогоны зелёные (`make test TEST_FILTER=git_blame` → `49 passed; 0 failed; 0 ignored; 3461 filtered out`, `/tmp/rriter-t9-test.log:822`; финальный вывод — `/tmp/rriter-t9-test-final.log:822`).
- Финальная проверка компиляции тестов прошла; она выдала 67 предупреждений проекта (`make check-tests` → `Finished release profile`, `/tmp/rriter-t9-check-final.log`).
- Линтер не вывел сообщений (`python3 scripts/lint_changed.py` → `/tmp/rriter-t9-lint.log`, 0 bytes).
- `git diff --check` прошёл без вывода; `make codex_test` не запускался согласно ограничениям Task 9.
- Concern: для пути «нет snapshot» и ошибки git добавлена классификация/обработка по уже существующему состоянию, но отдельные headless сценарии для этих двух результатов не добавлялись (`src/app/git_blame.rs:103`, `src/app/git_blame.rs:355`).
