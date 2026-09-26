# code-review-graph MCP — usage manual

Optional tool. RRiter default search is `rg`-first (see `AGENTS.md` §0).
Use this manual when a structural question is easier for the graph: callers/callees of a widely used
function, test ownership, affected flows or review triage of a multi-file change.


This project has `code-review-graph` MCP. It is optional; use it for structural questions.

Purpose:

* Avoid broad repo scans.
* Find exact symbols/files fast.
* Check callers/callees before editing.
* Check affected flows and review risk before risky changes.
* Read only source files that matter.

Use `PROJECT_AI_MAP.txt` only when MCP is unavailable or broken.

Do not use graph/map as source code.
Do not create exact patches from graph/map only.
Always read exact source before editing.

### 0.1.1 Graph workflow (when you use the graph)

When using the graph for an edit/review/debug task:

1. Call `get_minimal_context_tool` first with the exact task and `repo_root`.
2. Use its risk and suggestions to choose only the next queries that can change the decision; do not call every graph tool mechanically.
3. Find the relevant node by exact file path or exact symbol name, then inspect only required callers, callees, tests, or flows.
4. Keep the initial graph pass to at most 5 calls and use `detail_level="minimal"` unless a high-risk result needs expansion.
5. Pick the minimum exact source files and read them before proposing or applying an exact patch.
6. Patch surgically.
7. If ANY file related to RRiter changed, run `make codex_test`.
8. Summarize useful graph evidence, files changed, residual risk/test gaps, and verification result; do not dump raw graph output.

### 0.1.2 Tool usage rules

Use these tools by intent:

* `get_minimal_context_tool`

  * Good first graph call for a task, changed files, or target area.
  * Good for summary, risk, key entities, communities, flow hints.
  * Treat output as triage, not exact code.
  * Pass the exact repository root and a narrow task description.

* `detect_changes_tool`

  * Primary tool for current-change/review triage after `get_minimal_context_tool`.
  * Use `detail_level="minimal"` for low risk and `detail_level="standard"` only for medium/high risk.
  * Prefer an explicit changed-file list when unrelated dirty files exist.

* `semantic_search_nodes_tool`

  * Find files/symbols by exact names.
  * Prefer precise queries:

    * `handle_main_mouse_input`
    * `Renderer.draw_editor_visible_text`
    * `src/render_view/editor_text_layer.rs`
    * `UiRegistry`
    * `start_active_api_request`
  * Avoid broad prose queries like `render frame draw hot path ui registry mouse input project search api client`; they may return 0.
  * If broad query returns 0, retry with exact symbol/file names from AGENTS.md, PROJECT_GUIDE.md, or user task.

* `query_graph_tool`

  * Use for structural facts.
  * Preferred patterns:

    * `file_summary` for file contents overview and line ranges.
    * `callers_of` before changing public/shared/hot functions.
    * `callees_of` before changing a function with many dependencies.
    * `tests_for` before/after patch planning.
    * `children_of` only when target format is known to work; if 0, use `file_summary`.
  * Prefer fully qualified targets when available:

    * `/abs/path/src/file.rs::FunctionName`
    * `/abs/path/src/file.rs::Type.method`
  * If relative target returns 0, retry with absolute path from graph search result.

* `get_affected_flows_tool`

  * Use when changing one or more files.
  * Use especially for render/input/API/LSP/editor hot paths.
  * Report affected flows only if useful to task.
  * Do not over-expand source reads just because graph shows many impacted nodes; choose files by risk and direct relevance.
  * A result of `0 flows` means no named flow matched; it is not proof that the change has no runtime impact.

* `get_review_context_tool`

  * Comprehensive, higher-cost follow-up for risky multi-file changes or unresolved questions after `detect_changes_tool`.
  * Start with `detail_level="minimal"` and `include_source=false`; request source snippets only when they replace a separate exact-source read.
  * Do not call it together with `get_impact_radius_tool` by default; their output overlaps.

* `refactor_tool`

  * Use before renames/moves/dead-code cleanup.
  * Do not delete unrelated dead code.
  * If dead code unrelated to current task appears, mention it only.

* `run_postprocess_tool`

  * Use only when graph appears stale or user asks to refresh analysis.
  * Do not run expensive postprocess repeatedly during normal edits.

* `embed_graph_tool`

  * Use only if semantic search quality is poor and embeddings are missing/stale.
  * Do not run unless needed.

* `generate_wiki_tool`

  * Use only for documentation/architecture summary tasks.
  * Do not use for normal bugfix.

* `get_docs_section_tool`

  * Use when unsure how to use code-review-graph tools.
  * Prefer this over guessing tool syntax.
  * Request one exact section only; never load the full reference by default.
  * If it returns an inconsistent `not_found` for a listed section, fall back to the installed package's `code_review_graph/docs/LLM-OPTIMIZED-REFERENCE.md` or the live tool schema, and mention the MCP docs failure.

### 0.1.2.1 Latency and concurrency

The graph uses one repository database. Treat graph calls as an adaptive decision chain, not a parallel search fan-out.

* Do not send large `Promise.all`/parallel batches of `semantic_search_nodes_tool`, `query_graph_tool`, review, or impact calls to the same repository. In observed RRiter runs, batches of 8 calls took about 60-70 seconds and returned redundant context.
* Call one high-information tool, inspect its result, then issue the next exact query. At most 2 lightweight independent read queries may run together when they do not duplicate work.
* With parallel investigation agents, the controller should provide the known file/symbol/domain in each brief. Each agent should run only its narrow domain queries instead of repeating broad repository discovery.
* Never run `build_or_update_graph_tool`, `run_postprocess_tool`, `code-review-graph update`, or `build` concurrently. One controller owns graph mutation after source integration.
* If the graph becomes slow or noisy, stop expanding it and read the already identified exact source; the graph is an index, not a completion gate.

### 0.1.3 Graph target strategy

Preferred target order:

1. Exact symbol from user task.
2. Exact file path from user task.
3. Known hot file from AGENTS.md.
4. File guide path from PROJECT_GUIDE.md.
5. Fallback map entry from `PROJECT_AI_MAP.txt`.

When searching:

* First query exact name.
* If no result, query file basename.
* If no result, query owner/type name.
* If no result, use PROJECT_GUIDE.md routing.
* If still no result, use Grep/read-only search.

Stop after the shortest successful route. Do not issue all fallback variants preemptively. For an empty result, inspect the response `confidence`: only an explicit verified absence from a fresh graph is positive structural evidence. Missing confidence, stale/unindexed data, or a stated language/static-analysis gap is inconclusive and requires exact source or exact `rg`; verify high-risk negative claims in source even when confidence is high.

When graph returns standard/library callees (`get`, `map`, `unwrap_or`, `clone`, `round`, etc.), ignore them unless task is specifically about those calls.

### 0.1.3.1 Graph miss fallback for Rust include/impl methods

`callers_of` can return false `0` for Rust methods split through `include!`/module shell files, especially `impl Type { fn method(...) }` in chunk files.

If `callers_of('/abs/path/file.rs::Type.method')` returns `0` but target is an impl method or constructor:

1. Do not treat `0` as proof of no callers.
2. Use `semantic_search_nodes_tool` for exact variants:

   * `Type.method`
   * `Type::method`
   * `method`
   * implementation file basename

3. Run `file_summary` on both:

   * concrete implementation file, for example `src/renderer/renderer_init_methods.rs`
   * include shell/module file, for example `src/renderer.rs`

4. Retry `callers_of` only with canonical node ids/targets returned by graph search or `file_summary`; do not rely only on hand-written target strings.
5. If graph still returns `0`, use exact read-only `rg` as MCP gap fallback, not as first step:

   * constructors: `rg -n "Type::method\\(" src`
   * methods: `rg -n "\\.method\\(" src`
   * function name fallback: `rg -n "method\\(" src`

6. In summary, say: `Graph callers_of returned 0 for include/impl target; verified with exact rg fallback`.

### 0.1.4 Graph checks by task type (when using the graph)

Bug fix:

1. `get_minimal_context_tool` with the exact symptom.
2. `semantic_search_nodes_tool` only if the failing file/symbol is not already known.
3. `query_graph_tool file_summary` for the target file.
4. `callers_of`, `callees_of`, or `tests_for` only where the answer changes root-cause or test scope.
5. Read exact source, patch minimally, and run `make codex_test` if any RRiter file changed.

Render/input/editor hot-path change:

1. `get_minimal_context_tool`, then find the exact target symbol/file if still unknown.
2. `file_summary`, plus relevant `callers_of`/`callees_of`.
3. `get_affected_flows_tool` once for the explicit changed files; treat `0 flows` as inconclusive.
4. Read exact source, patch allocation-light, and run `make codex_test`.

Review current changes:

1. `get_minimal_context_tool` with the explicit changed files.
2. `detect_changes_tool`: minimal detail for low risk, standard only for medium/high risk.
3. `get_affected_flows_tool` for user-facing/high-risk paths; use `get_review_context_tool` only if important questions remain.
4. Inspect high-risk files and exact diff/source only.
5. Report findings with severity and `file:line`, risk, minimal fix, residual risks/test gaps, and verification. If editing, run `make codex_test`.

Refactor/rename:

1. `get_minimal_context_tool`, then `semantic_search_nodes_tool` for the exact symbol if needed.
2. `query_graph_tool callers_of` and `callees_of`.
3. `refactor_tool` where useful.
4. Read exact source/direct call sites, patch only required files, and run `make codex_test`.

Architecture explanation:

1. `get_minimal_context_tool`.
2. `semantic_search_nodes_tool` for entrypoints.
3. `query_graph_tool file_summary`.
4. `get_affected_flows_tool` or docs/wiki tool if needed.
5. Do not edit files.

### 0.1.5 Graph freshness

Graph lives in `.code-review-graph/`.

Run `code-review-graph status` once when graph output contradicts exact source, after a branch switch, or before relying on freshness-sensitive results. A commit mismatch or structural dirty change means the graph may be stale; exact source remains authoritative.

Normal update command:

```bash
code-review-graph update
```

Full rebuild command:

```bash
code-review-graph build
```

Use full build after large moves/splits or graph corruption.

Prefer one controller-owned `code-review-graph update --brief` after integrated source changes. Do not make every subagent refresh the same graph, and do not rebuild merely because a query returned `0`.

If full postprocess is too slow, acceptable fallback:

```bash
code-review-graph build --skip-postprocess
```

Do not edit `.code-review-graph/` manually.
Do not commit `.code-review-graph/`.

`.code-review-graphignore` must ignore:

```text
target/
.git/
.code-review-graph/
```
