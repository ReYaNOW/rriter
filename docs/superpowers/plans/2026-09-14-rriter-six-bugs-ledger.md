# SDD ledger — plan: /home/reyan/projects/rriter/docs/superpowers/plans/2026-09-14-rriter-six-bugs.md

RECOVERED 2026-09-15 from controller memory: scratchpad was wiped on session restart (snapshots, briefs, reports, rriter-verify, original ledger lost). Repo working tree intact.

## Rulings (recovered, in order)
- Ruling: No branch/worktree/commits; work in master working tree — AGENTS.md §4 forbids mutable git — cost: user must branch/commit manually.
- Ruling: Workspace in session scratchpad — cost: lost on scratchpad wipe (happened; recovered from memory).
- Ruling: Per-task review package = snapshot diff (T1–T5); T6+ after wipe = `git diff HEAD` over task files — cost: minor cross-task noise in package.
- Ruling: Implementers run focused tests only; full verification once at the end — cost: cross-task breakage found late.
- Ruling: Models per Model Selection; `sonnet` and `haiku` aliases resolve to unavailable xiaomi/mimo models here → all roles on opus — cost: token spend.
- Ruling: T6 viewport width via ONE shared helper for layout reserve and geometry — cost: small deviation from plan text.
- Ruling: markdown_code_scroll.rs docs entry done in T6; T8 verifies docs + runs verification — cost: none.
- Ruling: Focused test command `make test TEST_FILTER='<substring>'` (Makefile test-one uses --exact) — cost: none.
- Ruling: (superseded) tests ran in scratch copy rriter-verify with pdfium import removed.
- Ruling: Per user request, controller removed non-compiling `use pdfium_render` stub + comment from src/main.rs — cost: user wanted a dependency instead.
- Ruling: T4 Important (plan-mandated) core_text.rs over 1600-line cap → moved only max_scroll_x_tests to src/renderer/renderer_primitives_tests.rs; production lines stay — cost: core_text.rs remains 1986 lines.
- Ruling: After wipe, tests run directly in the repo (main.rs now compiles; rriter-verify lost) — cost: none.
- Note: user edited AGENTS.md (§4 mutable-only git/network; §5 "In SuperPowers workflow you can run tests how you like, can ignore later required make codex_test"; removed §7 Communication Style).

## Deferred minors (for final review)
- T1: layer-order test structural only; Glyph pass recomputes cx/next_cx; wide-glyph spacer logical index from ronsole not ported.
- T2: suspended-branch test is source-text based; 10 Hz wake while unfocused during long DB jobs.
- T3: Read-branch test indentation-sensitive include_str; events.rs cursor-icon site never sees Read mode (dead duplicate); mixed module paths; ignored args in Some branch.
- T4: O(hinted lines × hints) on hint arrival; hint at next-line first byte double-counted; no early-return test; column counting treats \r/wide glyphs as 1 col; renderer_primitives_tests.rs mixes prod+tests; core_text.rs 1986 > cap.
- T5: Reader never-arms-hover covered only by boolean helper test; stale hover clear + Blocked wheel requests no redraw; stale rect/diag_rect may consume one wheel if pointer unmoved (pre-existing).
- Pre-existing failing test (not ours, verified by reverting): reviewer_reader_v1_all_block_selection_seating_uses_real_vertices.

## Progress
- Task 1: complete (review clean)
- Task 2: complete (review clean)
- Task 3: complete (review clean)
- Task 4: complete (1 fix round, review clean)
- Task 5: complete (review clean)
- Task 6: implementer DONE (new markdown_code_scroll.rs 682 lines + markdown_read.rs, markdown_read_interaction.rs, app/markdown.rs, app.rs, ui_system.rs, ui_handlers.rs, markdown_scroll_review_tests.rs, AGENTS.md §9, PROJECT_GUIDE.md). Reported: markdown_code_scroll 9/9, markdown_scroll 38, app::markdown 42, ui_system 26, render_view:: 515/1 (known). Concerns: code-scroll MarkdownTabState methods in new chunk; chunk 682 lines; stale byte-offset keys (clamped); thumb via register_blocker instead of register_rect. Implementer report file LOST in wipe.
- Task 6: review dispatch 1 failed (rate limit); re-dispatching.
- User confirmed: AGENTS.md §4/§5/§7 edits are the user's own.
- Task 6: review (opus) — spec ✅, quality Approved. register_blocker deviation judged correct (same Rect element; arrow cursor as brief asks).
- Task 6: minor (deferred): no test asserting scrolled glyph/selection vertex x offset; scissor test doesn't check intersected rect.
- Task 6: minor (deferred): draw_mono_source_highlights walks full remainder of long overflowing lines when selection/search active (per-frame cost ∝ line length).
- Task 6: minor (deferred): byte-offset keyed code scroll state not cleared on edit (wrong block can inherit scroll; clamped).
- Task 6: minor (deferred): MarkdownTabState code-scroll methods in renderer chunk, no pointer comment in app/markdown.rs.
- Task 6: minor (deferred): cursor-kind mapping test at markdown_read.rs:2097 lacks new id case (GL test covers).
- Task 6: minor (deferred): code_scroll_geometry computed twice per visible code block per frame; chunk 682 lines > plan 600 target.
- Task 6: complete (git diff HEAD package, review clean)
- Ruling: Ledger mirrored to docs/superpowers/plans/2026-09-14-rriter-six-bugs-ledger.md after each bookkeeping step — scratchpad was wiped once — cost: one untracked .md in repo (user may delete).
- Task 7: implementer (opus) DONE_WITH_CONCERNS — markdown_code_scroll.rs (953 lines), app/markdown.rs (1660→1732), wheel.rs, input.rs, cursor.rs, about.rs. markdown_code_scroll 11/11, app::markdown 42, app::mouse 141, about 28, markdown_read 147/148 (known). Concerns: app/markdown.rs over cap grew +72; tick + cursor branch untested (cursor.rs:402 window unwrap blocks fixture); mode switch doesn't end code-scroll drag (release/cancel does).
- Task 7: review (opus) — spec ✅; Important (plan-mandated): src/app/markdown.rs 1660→1732 vs AGENTS.md 1600 cap.
- Ruling: Move the new `impl App` code-scroll wrapper block (scroll_markdown_code_block_x_at / begin_markdown_code_scrollbar_drag_at / drag_markdown_code_scrollbar_to + any private helper) from src/app/markdown.rs into src/render_view/markdown_code_scroll.rs (same feature chunk, ~1021 lines after move), keep signatures; update the §9 AGENTS.md / PROJECT_GUIDE.md wording to mention App input methods — keeps feature cohesive, no <200-line new file, doesn't dilute markdown_scroll_transition.rs ("must not become a second scroll owner") — cost if wrong: an `impl App` block lives in a render_view include chunk (unprecedented location; move later if disliked).
- Task 7: minor (deferred): wheel.rs 1602 lines (+17 pushed over cap).
- Task 7: minor (deferred): redundant Read-mode/frame/renderer lookups in scroll_markdown_code_block_x_at.
- Task 7: minor (deferred): set_markdown_mode / tab switch doesn't end code drag (stale until release; scroll_y behaves same).
- Task 7: minor (deferred): cursor.rs branch + about.rs tick untested (source-contains checks possible).
- Task 7: minor (deferred): begin/drag clamp(0, g.max_scroll) without finiteness guard (f32::clamp panics on NaN bound).
- Task 7: minor (deferred): test helper wheel() calls clear_hover_popup(None) needlessly.
- Task 7: fix round 1/5 started (FIX_BASE snap-7b).
- Task 7: fix round 1 implementer DONE — impl App wrappers moved to markdown_code_scroll.rs (1026 lines); app/markdown.rs back to 1660; docs entries updated. markdown_code_scroll 11/11, app::markdown 42, app::mouse 141.
- Task 7: fix round 1/5 (1 addressed, 0 open — impl App wrappers relocated; snap-7b..tree)
- Task 7: minor (deferred, out-of-scope obs): impl App in render_view chunk mixes layers (per ruling).
- Task 7: complete (snapshot snap-7 → tree, 1 fix round, review clean)
- Task 8: started — final whole-branch review (opus) + full make codex_test (background).
- Task 8: final review dispatched (opus, background, package review-final.diff); make codex_test running (background, log codex_test.log).
- Task 8: make codex_test EXIT=2 — `make test`: 2384 passed, 1 failed (panic at src/render_view/markdown_scroll_review_tests.rs:984); `make fast` half never ran (make stops after test failure).
- Ruling: Do not run `make fast` (user CLAUDE.md forbids); verify non-test cfg compiles with `cargo +nightly check --release` (no binary) instead — cost: release link/LTO issues not covered.
- Task 8: verifying failing test against pristine HEAD via `git archive HEAD` extract (read-only git) with only the pdfium import removed.
- Task 8: sole failure = render_view::markdown_read::reader_stage1_review_v1::reviewer_reader_v1_all_block_selection_seating_uses_real_vertices ('dpi=2 kind=table delta=-3 limit=2.5' x2) — same message as T3/T6 baselines; confirming on pristine HEAD extract (background).
- Task 8: cargo +nightly check --release (non-test cfg) EXIT=0, 11 dead-code/unused warnings; candidates ours: method code_block_max_scroll_x never used (markdown_code_scroll.rs:136), unused import MarkdownDocument (markdown_read.rs:7) — checking against HEAD.
- Task 8: warning attribution — `MarkdownDocument` unused import is pre-existing (identical at HEAD, not in diff). `code_block_max_scroll_x` (markdown_code_scroll.rs:136) is OURS: plan-mandated T6 interface that T7 never used (only its own test calls it) → new dead-code warning introduced by this change.
- Ruling: Include removal of `code_block_max_scroll_x` + its test in the single final fix dispatch (AGENTS.md "Remove only unused code created by change") rather than wiring it in — T7 uses `markdown_code_scroll_geometry` for the same bound — cost: Task 7-style callers lose a layout-only helper (trivially re-addable).
- Task 8: CONFIRMED pre-existing — seating test fails on pristine HEAD extract (git archive HEAD, only pdfium import removed) with identical message ('dpi=2 kind=table delta=-3 limit=2.5' x2, same SEATING lines). Not caused by this change.
- Task 8: final review (opus) — Ready to merge: With fixes. Important: (1) wheel.rs 1584→1602 over cap (only file this change pushed over); (2) events.rs:815-826 & 1410-1422 duplicated search-panel scrollbar_x block. Minor: (1) scroll_code_block_x doesn't clamp_current after max_scroll shrinks; (2) code-scroll drag survives set_markdown_mode/tab switch; (3) Task 2 unfocused window repaints only on refocus (plan-level, accepted); (4) growth in pre-oversized files (accepted). Deferred-minor triage: all Defer except T3 events dup and T7 wheel.rs cap (→ fix).
- Ruling: ONE final fix dispatch covering Important #1, Important #2, removal of dead `code_block_max_scroll_x` + its test, and Minor #1/#2 one-liners (reviewer-recommended, trivially low risk) — cost if wrong: minors widen the fix diff slightly.
- Ruling: Minor #3 (unfocused window stale paint until refocus) accepted as plan-level; the reported bug (needing collapse/expand) is fixed because state is applied before the focus click; will be stated honestly to user — cost: user may want live repaint while unfocused (separate change to suspension semantics).
- Task 8: final fix wave started (FIX_BASE snap-final).
- Task 8: final fix wave DONE_WITH_CONCERNS — wheel.rs 1588 (<cap), events.rs 1676→1683 (helper longer than copies; pre-existing over cap), markdown.rs 1662, markdown_code_scroll.rs 1094, app_ide_tab_methods.rs (drag ended in sync_active_tab before markdown swap — covers all tab paths). code_block_max_scroll_x removed; clamp_current added in wheel + drag paths. Tests: markdown_code_scroll 13, app::mouse 141, app::markdown 42, search 109, app::events 65; cargo check --release EXIT 0, our dead-code warning gone.
- Task 8: full make codex_test re-run started (background) on post-fix tree.
- Task 8: scoped re-review of final fix wave dispatched (opus, background, review-final-fix.diff).
- Task 8: post-fix make codex_test EXIT=2 — make test 2386 passed, 1 failed = pre-existing seating test (identical message, fails on pristine HEAD); make fast half not reached (make stops on test failure); non-test cargo check --release EXIT 0 per fix report.
- Task 8: final fix wave re-review — all 5 findings ADDRESSED, no new Critical/Important. Minor (parked): sync_active_tab also runs from save_tabs_state/autosave (ends code drag; judged unreachable mid-drag in practice); reverse-unresolved set_markdown_mode branch untested; finding-4 RED showed only wheel assertion. Out-of-scope: events.rs 1683 > cap (pre-existing oversize, +7); target.round() after clamp can exceed fractional max by 0.5 (render clamps).
- Task 8: complete (final review with fixes applied, re-review clean; make codex_test: 2386 pass / 1 pre-existing failure)

## Follow-up (2026-09-15): user-reported bugs after delivery
- Bug F1: Reader code-block horizontal drag unreliable ("только кликами нормально, через drag плохо, часть не подхватывается").
- Bug F2: Database panel shows "Ожидание загрузки…" when opened after startup (not open at start); collapse/expand needed. Opening editor with DB panel already open works.
- F2 ROOT CAUSE (confirmed by code): sidebar slot press only creates ide_panel.drag (input.rs:1418); release without threshold toggles panel at input.rs:~1848 — that path never calls reconcile_expanded_database_connections (only the SidebarSlot handle_ui_click arm at ui_handlers.rs:1412 does, which real clicks don't reach).
- F1 investigation: rejected — autosave-on-press (only on focus loss), stop_click_scroll_anims (doesn't touch code scroll), frame width mismatch (body rect w == draw content_w). Live hypothesis: final fix wave added end_code_scroll_drag() in sync_active_tab; fs watcher events (about.rs:519-526 start_external_changes_check) and poll_external_changes call sync_active_tab → kills drag mid-gesture. Also candidate: thin hit band (track 4px + 2×3px pad at scale 1).
- F1 user answer: drag almost never moves content ("очень редко drag-тся, криво" + "вообще не двигается при drag"); track clicks work. → systematic, not occasional: rules out fs-watcher/sync_active_tab (H1) as primary, thumb lag (H3).
- F1 ruled out by code: popup move gate (EPS 0.5px), cursor.rs early returns (all state-gated), clamp_current (doesn't touch is_dragging), MouseInput/CursorMoved routing (direct), draw/tick mutations (none), frame width mismatch, autosave-on-press, stop_click_scroll_anims.
- Ruling: F1 next step = temporary env-gated (RRITER_DEBUG_CODE_SCROLL) diagnostics on the real event path, reproduced by the user in the real app (fixture can't drive cursor.rs due to window unwrap) — brief docs/superpowers/plans/followup-f1-diagnostics-brief.md; dispatched after F2 agent finishes (shared build/target lock) — cost: user must run one diagnostic session.
- F2: implementer dispatched (opus, background) with docs/superpowers/plans/followup-f2-brief.md; snapshot scratchpad/sdd/followup/snap-f2.
- F2 implementer NEEDS_CONTEXT: real press/release path unwraps window in IDE mode (input.rs:1139, :1972); headless test impossible.
- Ruling: F2 option 2 — extract the window-free part of the sidebar click-toggle (toggle + Explorer/Search side effects + same-group exclusion + NEW Database reconcile when opened) into one App helper called from the release path; caller keeps the window-dependent scroll clamp; behavioral test on the helper — surgical, no unwrap changes in hot input path — cost if wrong: test doesn't exercise press/release plumbing itself (structure unchanged there).
- F2 implementer DONE — input.rs only: helper toggle_sidebar_panel_from_click (:434) with DB reconcile (:463), release path calls it (:1867). sidebar_click_ 2/2 (RED: stayed ExpandedUnloaded), app::database 228, app::mouse 143, database_panel 65. Concern: input.rs 2695 lines (pre-existing over cap; growth = tests).
- F1 diagnostics agent dispatched (opus, background) per followup-f1-diagnostics-brief.md.
- F2 task review dispatched (opus, background, docs/superpowers/plans/review-f2.diff).
- F2 review (opus): spec ✅, quality Approved.
- F2 minor (deferred): automation open_panel_semantic (automation.rs:1841) opens Database without reconcile (PGO path only).
- F2 minor (deferred): input.rs 2614→2695 (tests only; pre-existing over cap); release path still lacks save_panel_state/Terminal handling vs SidebarSlot handler (pre-existing, preserved); duplicated close-check in 2nd test.
- F2: complete (review clean). make codex_test pending (after F1 diagnostics build settles).
- F1 diagnostics DONE — markdown_code_scroll.rs (63 lines) + cursor.rs (6 lines), all // TEMP F1 DIAG, env RRITER_DEBUG_CODE_SCROLL; tests markdown_code_scroll 13, app::mouse 143; release check clean. Awaiting user log.
- User: F2 (DB "Ожидание загрузки") confirmed FIXED in real app on 14:30 build.
- F1 ROOT CAUSE (confirmed by diagnostics log docs/superpowers/plans/followup-f1-drag.log): all 12 drag terminations = about_to_wait (about.rs:526, fs watcher fs_changed) → start_external_changes_check (app_window_external_methods.rs:992) → sync_active_tab (app_ide_tab_methods.rs:660) → end_code_scroll_drag. Drags begin and follow pointer correctly (begin result=true, 57 drag moves with rising target), then are killed 2–100 ms later. The end_code_scroll_drag in sync_active_tab was added by the final fix wave (Minor #2 "drag survives tab switch"); sync_active_tab is swap bookkeeping used by many non-tab-switch paths (external checks, saves), so it must not end drags. Regression introduced by controller-approved fix; re-reviewer judged this path "unreachable in practice" — wrong (fs watcher fires right after drag begins).
- Snapshot for F1 fix review: scratchpad/sdd/followup/snap-f1.
- Ruling: F1 fix = remove `self.markdown.end_code_scroll_drag()` from `sync_active_tab` (app_ide_tab_methods.rs:660); keep drag end in set_markdown_mode, on left release, and in `cancel_pointer_interactions` (which `switch_to_tab` already calls at :799). Regression test: an active code drag survives `start_external_changes_check()` (sync_active_tab round-trip). Replace test `tab_swap_ends_code_scrollbar_drag` (open_new_tab path relied on sync_active_tab) with a `switch_to_tab` drag-end test. Remove all TEMP F1 DIAG lines in the same dispatch — cost if wrong: opening/closing/diff/db tabs via keyboard mid-drag can leave a stale drag in the old tab until next left release (pre-existing class, same as scroll_y).
- F1 fix dispatched (opus, foreground).
- F1 fix DONE — diagnostics removed (63+6 lines), `end_code_scroll_drag` removed from sync_active_tab; new test external_changes_check_keeps_code_scrollbar_drag (RED left None right Some(7)); tab_swap test replaced by tab_switch_ends_code_scrollbar_drag. markdown_code_scroll 14, app::mouse 143, app::markdown 42, app::events 65, app_file_behavior_tests 107; release check clean.
- F1 minor (parked): open_new_tab (and other non-switch_to_tab active-tab changes) mid-drag leaves stale drag in old tab's stored state — pre-existing class; accepted per ruling.
- Full make codex_test started (background) on post-F1/F2 tree.
- Follow-up make codex_test EXIT=2: make test 2389 passed, 1 failed = pre-existing seating test (fails on pristine HEAD); make fast half not reached. Awaiting F1 review.
- F1 review (opus): spec ✅, quality Approved.
- F1 minor (deferred): regression test lacks precondition `assert!(app.external_changes_rx.is_none())` (could become vacuous if fixture changes); tab-switch test uses synthetic drag on a doc without a code block and already passed pre-fix (guards the path, not the fix).
- F1: complete (review clean). Awaiting user confirmation in real app (fresh `make fast` build without diagnostics).

## Follow-up 2 (user): fix failing seating test (or delete if broken / fix bug if real) + remove all unused warnings in release build
- Warnings: 10 release warnings (1 unused import markdown_read.rs:7, 9 dead items in automation_database.rs, database_query.rs ×5, git_panel_types.rs, git_commit_runtime.rs, root_helpers.rs). Batch implementer dispatched (opus, background); rules: test-only → #[cfg(test)], unused → delete, no blanket allow.
- Seating test ROOT CAUSE: real Reader table layout bug — table text baseline = (line_h*0.82).round() puts descenders on line-box bottom at all DPIs (ink bottom == line bottom), off-center worse at high DPI (dpi2 delta -3 > limit 2.5). Headings use font-metrics baseline and pass. HEAD commit didn't touch table code. Test is correct.
- Ruling: fix the bug (metrics-based table baseline stored in TableBlock, used by draw + all table baseline sites + test harness), not the test; keep limits; code/body text out of scope — brief followup-seating-brief.md; dispatch after warnings agent (shared markdown_read.rs) — cost if wrong: table text shifts up 2–4px at high DPI vs current look.
- Warnings batch DONE: all 10 items test-only → #[cfg(test)] (import moved into test module); release check 10→0 warnings; test build 0→0; database_query 71, git_panel 83, automation_database 7, empty_ide_still_draws_file_tree_overlay pass; markdown_read 150/1 (known seating failure).
- Seating fix: snapshot snap-seat taken; dispatching implementer + warnings reviewer.
- Seating fix implementer dispatched (opus, background); warnings batch review dispatched (opus, background, review-warnings.diff).
- Warnings review (opus): spec ✅, Approved — all 10 items have only test references on all targets; no newly dead helpers; no allow attrs; import move compiles.
- Warnings minor (deferred): test-module `use` placement at markdown_read.rs:1857 (pre-existing position).
- Warnings batch: complete (review clean). Awaiting seating fix.
- Seating fix DONE — TableBlock.baseline_offset from text_metrics.heading_baseline_offset(0.82, line_h) in append_table; draw + test harness use it; markdown_scroll_review_tests.rs:956 formula only. Seating test GREEN (table deltas +1..+1.5 at all DPI, were -0.5..-3); new table_baseline_uses_font_metrics_not_line_box_fraction_across_scales (RED 18 vs 16). markdown_read 152, markdown_scroll 38, table 160; release check 0 warnings.
- Seating minor (deferred): shared helper still named heading_baseline_offset though tables use it.
- Final make codex_test started (background); seating review next.
- Seating review (opus): spec ✅, Approved. Minors (deferred): table text scale 0.82 split between layout local and draw/hit-test literals (suggest TABLE_TEXT_SCALE const); helper name heading_baseline_offset now shared; new layout test pins wiring only (real-ink guarded by seating test).
- Seating fix: complete (review clean).
- FINAL make codex_test EXIT=0: 2391 passed, 0 failed; make fast built target/x86_64-unknown-linux-gnu/release/rriter with no warnings.
