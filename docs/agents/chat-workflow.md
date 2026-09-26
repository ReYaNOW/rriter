# Chat / no-file-access workflow

For agents working without direct file access (chat UIs, exact-substring patch parsers).
Agents with file access follow `AGENTS.md` instead.

## 2. Chat Workflow

When source files unavailable:

1. Use `code-review-graph` MCP to inspect relevant files/symbols/call edges.
2. If MCP unavailable, use `PROJECT_AI_MAP.txt` fallback.
3. Pick minimal files.
4. Ask complete files.
5. No exact patches before full source.
6. No exact code from graph/map/index.

Fallback map format for `PROJECT_AI_MAP.txt`:

* `M path` -> source file. All following `C`, `I`, `F` rows belong to this file until next `M`.
* `C kind name@line` -> type symbol (`struct` / `enum`) and source line.
* `I owner` -> impl/type owner. Following `F` rows are methods for this owner until next `I` or `M`.
* `F name@line>called_fn_ids` -> function/method declaration, source line, and direct calls.
* Function id = zero-based order of all `F` rows in the whole map.
* Call ids after `>` are base36 function ids.
* Missing `>` means no known direct project calls.

Required file request format:

```text
Need files:
1. path/to/file.rs
   Reason: exact code needed for `Owner.method` / behavior.
   Graph ref: `file_summary` / `callers_of` / `callees_of` result for target symbol.

2. path/to/other.rs
   Reason: called by / calls previous symbol.
   Graph ref: direct caller/callee from `code-review-graph`.
```

If MCP unavailable and fallback map is used:

```text
Need files:
1. path/to/file.rs
   Reason: exact code needed for `Owner.method` / behavior.
   Map ref: `M path/to/file.rs` -> `I Owner` -> `F method@line>...`

2. path/to/other.rs
   Reason: called by / calls previous symbol.
   Map ref: call id `<base36_id>` from `F caller>...`
```

Request minimum files:

* Prefer 1-3 files.
* Ask 4+ only when behavior crosses subsystems.
* Unsure -> ask narrowest entrypoint file first.
* Stop after file request. No speculative patch.

### Chat-Only

If no write access:

* Request full source first.
* Then provide patch format user asks for.
* Convenient diff -> unified diff.
* Exact-substring parser -> strict format below.

### Strict Exact-Substring Parser

Use only when user explicitly needs exact-substring parser patches and direct edits/unified diff unavailable.

````text
1) path/to/file.rs
```rust
// Before:
exact_old_code
// After:
exact_new_code
```
````

Rules:

1. `Before:` copied from actual source provided in task.
2. Never create `Before:` from memory/map/summary/guess.
3. No `...`, no omitted code, no `unchanged`.
4. Distant edits -> split numbered blocks.
5. Use fewest lines needed for unique exact match.
6. Missing exact source -> ask file, do not patch.
7. Block line only path, or path plus `(New file)`.
8. No comments on path line.
9. Preserve original spacing in `Before:` exactly.

New file:

````text
2) path/to/new_file.rs (New file)
```rust
file contents
```
````
