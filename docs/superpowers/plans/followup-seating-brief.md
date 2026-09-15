# Follow-up: failing test `reviewer_reader_v1_all_block_selection_seating_uses_real_vertices` — real table seating bug

## Evidence (make codex_test log docs/superpowers/plans/codex_test_followup.log)
Only table rows fail: `dpi=1.75 kind=table delta=-2.5` (limit 2.5), `dpi=2 kind=table delta=-3` (limit 2.5).
Table SEATING lines at EVERY dpi show ink bottom == line-box bottom (e.g. dpi=2 line=[1160,1204] ink=[1170,1204]; dpi=1 line=[580,602] ink=[585,602]):
descenders sit on the row line-box bottom with a 3–10px gap above → Reader table text is visibly below the vertical center of its line/selection, growing with DPI.
Headings pass because they use a font-metrics baseline `LayoutTextMetrics::heading_baseline_offset(text_scale, line_height)` (src/render_view/markdown_read.rs:294-303); the heading regression test at markdown_read.rs:~2384 asserts headings must use final font metrics "rather than 0.82 of enlarged line box".
The test was added in 468fd8c; HEAD 4b36e6b did not touch Reader table layout/draw. Verdict: real layout bug, test is correct — do NOT weaken or delete it.

## Root cause
Table cell text baseline is a fixed fraction of the table line box:
- layout `LayoutBuilder::append_table` (markdown_read.rs:~742-865): `line_h = (22.0 * scale).round()`, text drawn at scale 0.82;
- draw `draw_markdown_block` Table arm (markdown_read.rs:~1615): `let baseline_offset = (line_height * 0.82).round();` then `baseline = (line_top + baseline_offset).round()`.
For 0.82-scale glyphs in a 22·s line box that pushes the ink to the bottom.

## Required fix (surgical)
1. Compute the table text baseline offset ONCE in layout from font metrics, the same way headings do: `self.text_metrics.heading_baseline_offset(<table text scale 0.82>, line_h)`, and store it in `TableBlock` (new field, e.g. `baseline_offset: f32`). Prefer a named const for the table text scale if one exists; don't introduce duplication.
2. Use the stored `table.baseline_offset` everywhere the table baseline is derived: the draw Table arm (replace `(line_height * 0.82).round()`), and every other production site that recomputes the table baseline (rg `line_height \* 0.82`, `table.line_height`, `0.82` in src/render_view/markdown_read*.rs, markdown_scroll*.rs — check markdown_read_interaction.rs ~1376 table search/selection geometry and any source-anchor/baseline projection for tables). Code-block (`code.line_height * 0.82`) and body text are OUT of scope — don't change them.
3. Tests: the failing test's table sample baseline currently re-derives `(top + (table.line_height * 0.82).round()).round()` (markdown_scroll_review_tests.rs:~950) and markdown_read.rs:~2011 test code does the same — update test harness code to use the stored `table.baseline_offset` so tests mirror production instead of duplicating the old formula. Keep the test's assertion logic and limits unchanged.
4. Add one focused layout regression test: for scales [1.0, 1.25, 1.5, 1.75, 2.0], a parsed table's `baseline_offset` equals `text_metrics.heading_baseline_offset(0.82, line_height)` and is strictly less than `(line_height * 0.82).round()` at scale 2.0 (documents the moved baseline), mirroring the heading test pattern at markdown_read.rs:~2384.

## Verification
- RED already exists: `make test TEST_FILTER='reviewer_reader_v1_all_block_selection_seating_uses_real_vertices'` fails (capture it before editing).
- GREEN: same test passes; `make test TEST_FILTER='markdown_read'`, `make test TEST_FILTER='markdown_scroll'`, `make test TEST_FILTER='table'` pass (report any other failures with evidence).
