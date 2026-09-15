# Follow-up report: Reader table text seating

## Summary
Table cell text in Reader mode set its baseline at a fixed fraction of the row line box: `(line_height * 0.82).round()`. The glyphs are drawn at scale 0.82, so the descenders sat right on the bottom edge of the line box. Layout now works out the table baseline once, from font metrics, the same way headings do: `LayoutTextMetrics::heading_baseline_offset(0.82, line_h)`. It is stored in the new `TableBlock.baseline_offset` field, and draw code and test harnesses read that stored value. Code-block and body-text baselines are unchanged.

## Changed sites
- `src/render_view/markdown_read.rs:262`: new `TableBlock` field `baseline_offset: f32`.
- `src/render_view/markdown_read.rs:783-786` (`LayoutBuilder::append_table`): adds a local `text_scale = 0.82` and `baseline_offset = self.text_metrics.heading_baseline_offset(text_scale, line_h)`. The existing named consts are `BODY_SCALE` (0.96) and `CODE_LANGUAGE_SCALE` (0.80); there is no const for the table scale, so I did not add one.
- `src/render_view/markdown_read.rs:813`: the wrap-advance call uses `text_scale` instead of a literal `0.82`, so one value controls this function.
- `src/render_view/markdown_read.rs:866`: stores `baseline_offset` in `TableBlock`.
- `src/render_view/markdown_read.rs:1621` (`draw_markdown_block` Table arm): `let baseline_offset = table.baseline_offset;` replaces `(line_height * 0.82).round()`. There is no per-frame computation or allocation, and the baseline stays `(line_top + baseline_offset).round()`.
- `src/render_view/markdown_read.rs:2017` (test `ReadBlockKind::Table` pixel-stability check): uses `table.baseline_offset`.
- `src/render_view/markdown_read.rs:2399`: new test `table_baseline_uses_font_metrics_not_line_box_fraction_across_scales`.
- `src/render_view/markdown_scroll_review_tests.rs:956`: the seating test's table sample baseline is now `(top + table.baseline_offset).round()`. Its assertions, limits and failure-message logic are unchanged.
- Kept as is: the `MarkdownDocument` import stays inside the test module (`markdown_read.rs:1863`).

## Every table-baseline site found (rg `0.82`, `table.line_height`, `baseline_offset` in markdown_read*.rs, markdown_scroll*.rs, markdown_code_scroll.rs, app/markdown*.rs)
| Site | Derives a table baseline? | Handling |
|---|---|---|
| markdown_read.rs:1621 draw Table arm | yes | now reads `table.baseline_offset` |
| markdown_read.rs:2017 test pixel-stability check | yes (test) | now reads `table.baseline_offset` |
| markdown_scroll_review_tests.rs:956 seating test | yes (test) | now reads `table.baseline_offset` |
| markdown_read_interaction.rs:619-644 table hit-test | no. It uses the line-box top (`row.y + cell_padding`) and height through `uniform_line_box_index`, plus 0.82 as the measurement scale | unchanged (correct) |
| markdown_read_interaction.rs:1376 `table_inline_code_hit_test_...` | no. 0.82 is the horizontal text scale in a unit test | unchanged |
| markdown_scroll.rs:207-218 source-line and anchor geometry | no. It uses line top and top + line_height | unchanged |
| markdown_read_interaction.rs:1796-1824 tests | no. They use line top and line height | unchanged |
| markdown_scroll_review_tests.rs:1002-1009 wrapped hit-test test | no. It uses line top and 0.82 as the width scale | unchanged |
| markdown_read.rs:1650/1669 draw measure/draw text scale 0.82 | no. Text scale only | unchanged |
| markdown_read.rs:703/713/723 code `line_h * 0.82`; markdown_read_interaction.rs:1555 `code.line_height * 0.82` | code blocks | out of scope, unchanged |
| markdown_read.rs:628 body `(line_h * 0.82).round()`; 2032 body pill test | body text | out of scope, unchanged |
| markdown_read.rs:296 `heading_baseline_offset` fallback when font_size <= 0 | shared helper fallback | unchanged |

## TDD evidence
### RED 1: existing failing test, before any edit
Command: `make test TEST_FILTER='reviewer_reader_v1_all_block_selection_seating_uses_real_vertices'`
```
SEATING dpi=1 kind=table line=[580,602] baseline=598 selection=[582,604] ink=[585,602] delta=-0.5
SEATING dpi=1 kind=table line=[618,640] baseline=636 selection=[620,642] ink=[623,640] delta=-0.5
SEATING dpi=1.25 kind=table line=[729,757] baseline=752 selection=[731,759] ink=[736,757] delta=-1.5
SEATING dpi=1.25 kind=table line=[777,805] baseline=800 selection=[779,807] ink=[784,805] delta=-1.5
SEATING dpi=1.5 kind=table line=[871,904] baseline=898 selection=[873,906] ink=[878,905] delta=-2
SEATING dpi=1.5 kind=table line=[928,961] baseline=955 selection=[930,963] ink=[935,962] delta=-2
SEATING dpi=1.75 kind=table line=[1017,1056] baseline=1049 selection=[1019,1058] ink=[1026,1056] delta=-2.5
SEATING dpi=1.75 kind=table line=[1084,1123] baseline=1116 selection=[1086,1125] ink=[1093,1123] delta=-2.5
SEATING dpi=2 kind=table line=[1160,1204] baseline=1196 selection=[1162,1206] ink=[1170,1204] delta=-3
SEATING dpi=2 kind=table line=[1236,1280] baseline=1272 selection=[1238,1282] ink=[1246,1280] delta=-3
thread 'main' panicked at src/render_view/markdown_scroll_review_tests.rs:984:9:
Reader seating exceeds heading/actual Editor reference: ["dpi=2 kind=table delta=-3 limit=2.5", "dpi=2 kind=table delta=-3 limit=2.5"]
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 2389 filtered out
```
The dpi=1.75 rows sit exactly at the limit, so only dpi=2 appears in the failure list. The codex_test log reports -2.5 at dpi=1.75 as failing, which suggests a limit rounding difference in that environment; in both cases the dpi=2 rows fail.

### RED 2: new layout regression test
First I added the field, stored it with the old formula `(line_h * text_scale).round()`, and added the new test. Nothing else changed yet.
Command: `make test TEST_FILTER='table_baseline_uses_font_metrics_not_line_box_fraction_across_scales'`
```
thread 'main' panicked at src/render_view/markdown_read.rs:2411:13:
assertion `left == right` failed: scale 1: table baseline must come from font metrics
  left: 18.0
 right: 16.0
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 2390 filtered out
```

### GREEN: after switching layout to `heading_baseline_offset`
Command: `make test TEST_FILTER='reviewer_reader_v1_all_block_selection_seating_uses_real_vertices'`
```
SEATING dpi=1 kind=table line=[580,602] baseline=596 selection=[582,604] ink=[583,600] delta=1.5
SEATING dpi=1 kind=table line=[618,640] baseline=634 selection=[620,642] ink=[621,638] delta=1.5
SEATING dpi=1.25 kind=table line=[729,757] baseline=749 selection=[731,759] ink=[733,754] delta=1.5
SEATING dpi=1.25 kind=table line=[777,805] baseline=797 selection=[779,807] ink=[781,802] delta=1.5
SEATING dpi=1.5 kind=table line=[871,904] baseline=895 selection=[873,906] ink=[875,902] delta=1
SEATING dpi=1.5 kind=table line=[928,961] baseline=952 selection=[930,963] ink=[932,959] delta=1
SEATING dpi=1.75 kind=table line=[1017,1056] baseline=1045 selection=[1019,1058] ink=[1022,1052] delta=1.5
SEATING dpi=1.75 kind=table line=[1084,1123] baseline=1112 selection=[1086,1125] ink=[1089,1119] delta=1.5
SEATING dpi=2 kind=table line=[1160,1204] baseline=1192 selection=[1162,1206] ink=[1166,1200] delta=1
SEATING dpi=2 kind=table line=[1236,1280] baseline=1268 selection=[1238,1282] ink=[1242,1276] delta=1
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 2390 filtered out
```
Command: `make test TEST_FILTER='table_baseline_uses_font_metrics_not_line_box_fraction_across_scales'` gives `test result: ok. 1 passed; 0 failed`.

## Neighbor filters
- `make test TEST_FILTER='markdown_read'`: ok. 152 passed; 0 failed
- `make test TEST_FILTER='markdown_scroll'`: ok. 38 passed; 0 failed
- `make test TEST_FILTER='table'`: ok. 160 passed; 0 failed

## Release check
`CARGO_BUILD_JOBS=4 RUSTFLAGS="-C target-cpu=native -C llvm-args=-fp-contract=fast -C link-arg=-fuse-ld=lld" cargo +nightly check --target x86_64-unknown-linux-gnu --release` finished with 0 warnings and 0 errors (`Finished release profile`).

## Concerns
- The table baseline moved up 2-4 px, depending on DPI. Deltas are now +1 to +1.5 at every DPI (they were -0.5 to -3), within the 2.5 limit.
- The helper keeps its name `heading_baseline_offset` even though tables now use it too. I did not rename it, to keep the change small.
- `make codex_test` was not run, as the task instructions required. `make fast` was not run.
