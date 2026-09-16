# RRiter Four Bugfixes Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Исправить перехват wheel невидимым hover, позиционирование search в центральных 30% viewport, panic после `Ctrl+/` и неверный Ctrl+wheel multiplier.

**Architecture:** Работа разделена на три независимо проверяемых problem domains. Hover и wheel объединены, потому что оба проходят через `src/app/mouse/wheel.rs`; search и comment/highlighter sync остаются отдельными доменами.

**Tech Stack:** Rust, winit mouse input, RRiter renderer/UI state, tree-sitter highlighter, встроенные unit/integration tests.

**Spec:** `docs/superpowers/plans/2026-09-16-rriter-four-bugfixes-design.md`

## Global Constraints

- Search anchor обязан находиться в полосе `0.35..=0.65` viewport с минимальным изменением текущего scroll target.
- Ctrl+wheel multiplier применяется ровно один раз к `LineDelta` и `PixelDelta`.
- Wheel потребляется hover popup только внутри реально отрисованного `interaction_rect`.
- Comment toggle обязан сохранить editor/highlighter byte replicas идентичными до worker sync.
- Никаких новых зависимостей, disk I/O или крупных allocations в render/input hot paths.

---

### Task 1: Hover wheel routing и Ctrl+wheel multiplier

**Files:**
- Modify: `src/app/mouse/hover_state_core.rs`
- Modify: `src/app/mouse/hover_mouse_logic.rs`
- Modify: `src/app/mouse/hover_visibility_tests.rs`
- Modify: `src/app/mouse/wheel.rs`
- Modify: `src/app/events/about/about_helpers.rs`
- Modify: `src/render_view/hover_overlays.rs`
- Modify: `src/render_view/ui/hover_widget.rs`

**Interfaces:**
- Consumes: renderer `frame_surface.outer_rect` и существующие type/diagnostic scroll targets.
- Produces: `HoverState::interaction_rect: Option<(f32, f32, f32, f32)>`; `wheel_delta(delta, line_height, multiplier)` с одинаковой multiplier semantics для line/pixel input.

- [ ] **Step 1: Добавить regression tests для невидимого hover и PixelDelta**

Проверить zero-progress pass-through, exterior pass-through для partial combined popup, consumption внутри visible surface, reset `interaction_rect`, а также фактический `5x` для обоих delta variants.

- [ ] **Step 2: Запустить tests в RED-состоянии**

Run: `make test TEST_FILTER=wheel`

Expected: новые assertions для невидимого popup или `PixelDelta` падают до production fix.

- [ ] **Step 3: Реализовать минимальный renderer-owned interaction rect и единый multiplier**

Renderer очищает rect перед draw и записывает точный видимый outer frame. Wheel router возвращает early только после реального локального consumption; простая очистка stale hover продолжает downstream routing. `wheel_delta` масштабирует line и pixel delta ровно один раз.

- [ ] **Step 4: Проверить GREEN**

Run: `make test TEST_FILTER=wheel`

Expected: все wheel/hover regression tests проходят.

### Task 2: Минимальное search positioning в центральной полосе

**Files:**
- Modify: `src/app/app_file_tab_methods.rs`
- Modify: `src/render_view/markdown_scroll_transition_review_tests.rs`

**Interfaces:**
- Consumes: match start anchor, текущий destination scroll target, viewport height, max scroll, fold-aware Markdown Edit projection и Markdown Reader source geometry.
- Produces: `search_anchor_central_band_target(anchor_y, current_target, visible_h, max_scroll) -> (f32, Option<f32>)`.

- [ ] **Step 1: Добавить regression tests для обеих границ и in-band no-op**

Проверить обычный editor, folded Markdown Edit, Markdown Reader и pending transition destination coordinates.

- [ ] **Step 2: Запустить tests в RED-состоянии**

Run: `make test TEST_FILTER=search`

Expected: прежнее hard-centering не удовлетворяет nearest-edge/in-band assertions.

- [ ] **Step 3: Реализовать allocation-free central-band target**

Если anchor выше `35%`, target ставит его на `35%`; если ниже `65%` — на `65%`; внутри полосы target не меняется. Результат округляется и clamp-ится. Folded и pending Markdown используют визуальные/destination coordinates.

- [ ] **Step 4: Проверить GREEN**

Run: `make test TEST_FILTER=search`

Expected: search regressions проходят во всех режимах.

### Task 3: Синхронизация highlighter после `Ctrl+/`

**Files:**
- Modify: `src/app/keyboard/editor_keys.rs`

**Interfaces:**
- Consumes: новые `EditorSyncEdit::Insert/Delete`, созданные `Editor::toggle_line_comment`.
- Produces: `toggle_editor_line_comment_and_sync_highlighter`, сохраняющий highlighter backing text синхронным с editor до `apply_edits`.

- [ ] **Step 1: Добавить regression test исходного panic path**

Тест вызывает shortcut helper, применяет worker edits, выполняет sync highlight и проверяет корректный comment span без panic.

- [ ] **Step 2: Запустить test в RED-состоянии**

Run: `make test TEST_FILTER=ctrl_slash_comment_toggle_keeps_highlighter_sync_replica_current`

Expected: без replay sync edits highlighter replica расходится с editor.

- [ ] **Step 3: Проиграть только новые sync edits в highlighter replica**

Запомнить исходную длину `sync_edits`, вызвать comment toggle и последовательно применить новый suffix через `shift_insert`/`shift_delete`; существующий общий worker-send path не дублировать.

- [ ] **Step 4: Проверить GREEN**

Run: `make test TEST_FILTER=line_comment`

Expected: shortcut regression и существующие comment-toggle tests проходят.

### Task 4: Интегрированный review и verification

**Files:**
- Review: все production/test файлы Tasks 1-3.

**Interfaces:**
- Consumes: task-level review verdicts и интегрированный working-tree diff.
- Produces: clean whole-change review и свежий полный verification result.

- [ ] **Step 1: Выполнить scoped re-review fix rounds Tasks 1 и 2**

Expected: прежние Important findings подтверждены как addressed, новый Critical/Important breakage отсутствует.

- [ ] **Step 2: Выполнить broad final code review**

Expected: требования четырёх багфиксов выполнены, scope хирургический, hot paths allocation-light.

- [ ] **Step 3: Выполнить финальную проверку**

Run: `make codex_test`

Expected: `2401 passed; 0 failed` и release binary собран.

