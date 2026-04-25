#!/usr/bin/env python3
"""gen_project_map.py — генератор PROJECT_MAP.xml под ИИ: компактный граф символов + вызовы."""
import re
import sys
from collections import defaultdict
from pathlib import Path

SRC_DIR = Path("src")

STD_CALLS = {
    "unwrap", "expect", "map", "and_then", "or_else", "ok", "err", "is_some", "is_none",
    "len", "is_empty", "collect", "clone", "to_string", "as_str", "as_bytes", "chars",
    "bytes", "lines", "split", "trim", "starts_with", "ends_with", "replace",
    "min", "max", "abs", "clamp", "round", "floor", "ceil", "sqrt", "from", "into",
    "default", "get", "set", "remove", "entry", "or_insert", "lock", "read", "write",
    "send", "recv", "try_recv", "sort", "sort_by", "dedup", "retain", "extend", "drain",
    "parse", "to_owned", "borrow", "as_ref", "as_mut", "zip", "enumerate", "filter",
    "flat_map", "for_each", "any", "all", "first", "last", "next", "take", "skip",
    "unwrap_or", "unwrap_or_default", "sum", "count", "find", "fold",
    "push", "pop", "insert", "clear", "push_back", "pop_back", "push_front", "pop_front",
}

ROOT_NAMES = {
    "main",
    "resumed",
    "window_event",
    "about_to_wait",
    "handle_main_keyboard_input",
    "handle_editor_keyboard_input",
    "handle_main_mouse_input",
    "handle_main_mouse_wheel",
    "handle_main_cursor_moved",
}

MAX_TYPE_ITEMS = 8
MAX_RW_ITEMS = 12


def short_path(p):
    return str(p).replace("\\", "/")


def module_name(sp):
    return sp.removeprefix("src/").removesuffix(".rs").replace("/", "::")


def compact_list_str(s, max_items):
    if not s:
        return s

    items = [item.strip() for item in s.split(",") if item.strip()]
    if len(items) <= max_items:
        return ",".join(items)

    kept = items[:max_items]
    rest = len(items) - max_items
    return ",".join(kept + [f"+{rest}"])


def compact_type_body(body, max_items = MAX_TYPE_ITEMS):
    if not body:
        return body

    inner = body.strip()
    if inner.startswith("{") and inner.endswith("}"):
        content = inner[1:-1].strip()
        items = [item.strip() for item in content.split(",") if item.strip()]
        if len(items) <= max_items:
            return "{ " + ", ".join(items) + " }"
        kept = items[:max_items]
        rest = len(items) - max_items
        return "{ " + ", ".join(kept) + f", +{rest} }}"
    return body


def strip_strings_and_comments(src):
    result = list(src)
    i = 0
    n = len(src)

    while i < n:
        if src[i] == "'" and i + 2 < n and src[i + 2] == "'":
            result[i + 1] = " "
            i += 3
            continue

        if src[i] == "/" and i + 1 < n and src[i + 1] == "/":
            j = i
            while j < n and src[j] != "\n":
                result[j] = " "
                j += 1
            i = j
            continue

        if src[i] == "/" and i + 1 < n and src[i + 1] == "*":
            result[i] = result[i + 1] = " "
            j = i + 2
            while j < n - 1:
                if src[j] == "*" and src[j + 1] == "/":
                    result[j] = result[j + 1] = " "
                    j += 2
                    break
                if src[j] != "\n":
                    result[j] = " "
                j += 1
            i = j
            continue

        if src[i] == '"':
            result[i] = " "
            j = i + 1
            while j < n:
                if src[j] == "\\" and j + 1 < n:
                    result[j] = result[j + 1] = " "
                    j += 2
                    continue
                if src[j] == '"':
                    result[j] = " "
                    j += 1
                    break
                if src[j] != "\n":
                    result[j] = " "
                j += 1
            i = j
            continue

        i += 1

    return "".join(result)


def extract_body(src, brace_pos):
    depth = 1
    i = brace_pos + 1
    n = len(src)

    while i < n and depth > 0:
        if src[i] == "{":
            depth += 1
        elif src[i] == "}":
            depth -= 1
        i += 1

    return src[brace_pos + 1:i - 1], i


FN_RE = re.compile(
    r"^([ \t]*)(pub(?:\s*\([^)]*\))?\s+)?(?:async\s+)?(?:unsafe\s+)?fn\s+(\w+)([^{]*)\{",
    re.MULTILINE,
)

IMPL_RE = re.compile(r"\bimpl\b[^{]*\{", re.MULTILINE)

TYPE_BLOCK_RE = re.compile(r"(?:pub\s+)?(struct|enum)\s+([A-Za-z0-9_]+)[^{]*\{([^}]*)\}")
TYPE_TUPLE_RE = re.compile(r"(?:pub\s+)?struct\s+([A-Za-z0-9_]+)[^(]*\(([^)]+)\)\s*;")

CALL_RE = re.compile(r"\b(\w+)::(\w+)\s*\(|\b(\w+)\s*\(")
SELF_ACCESS_RE = re.compile(r"\bself\.([A-Za-z0-9_]+)\s*(\()?")
SELF_MUTATE_RE = re.compile(
    r"\bself\.([A-Za-z0-9_]+)\.(push|pop|clear|insert|extend|retain|remove|truncate|sort|drain|push_back|pop_back)\b"
)
SELF_ASSIGN_RE = re.compile(r"\bself\.([A-Za-z0-9_]+)\s*=[^=]")


def extract_impl_blocks(src_clean):
    impls = []

    for m in IMPL_RE.finditer(src_clean):
        header = m.group(0)[:-1].strip()
        brace_pos = m.end() - 1
        _, end_pos = extract_body(src_clean, brace_pos)

        owner = ""

        owner_m = re.search(r"\bfor\s+([A-Za-z0-9_:]+)\s*$", header)
        if owner_m:
            owner = owner_m.group(1).split("::")[-1]
        else:
            owner_m = re.search(r"\bimpl(?:<[^>]*>)?\s+([A-Za-z0-9_:]+)\s*$", header)
            if owner_m:
                owner = owner_m.group(1).split("::")[-1]

        impls.append({
            "start": brace_pos + 1,
            "end": end_pos - 1,
            "owner": owner,
        })

    return impls


def extract_functions(src_clean, src_orig):
    fns = []
    impls = extract_impl_blocks(src_clean)

    for m in FN_RE.finditer(src_clean):
        brace_pos = m.end() - 1
        body, _ = extract_body(src_clean, brace_pos)

        name = m.group(3)
        is_pub = bool(m.group(2) and "pub" in m.group(2))

        sig_start = m.start()
        sig_raw = src_orig[sig_start:m.end()].split("{")[0].strip()
        sig_clean = re.sub(r"\s+", " ", sig_raw)

        ret_m = re.search(r"->\s*(.+?)(?:\s+where\b.*)?$", sig_clean)
        ret = ret_m.group(1).strip() if ret_m else ""

        line_no = src_orig[:sig_start].count("\n") + 1

        owner = ""
        recv = ""

        for impl in impls:
            if impl["start"] <= sig_start < impl["end"]:
                owner = impl["owner"]
                break

        args_m = re.search(r"\(([^()]*)\)", sig_clean)
        if args_m:
            args_text = re.sub(r"\s+", " ", args_m.group(1)).strip()
            if args_text:
                parts = [p.strip() for p in args_text.split(",") if p.strip()]
                if parts and parts[0] in ("self", "&self", "&mut self", "mut self"):
                    recv = parts[0]

        fns.append({
            "name": name,
            "ret": ret,
            "is_pub": is_pub,
            "body": body,
            "line": line_no,
            "owner": owner,
            "recv": recv,
        })

    return fns


def extract_structs_enums(src_clean):
    types = {}

    for m in TYPE_BLOCK_RE.finditer(src_clean):
        kind = m.group(1)
        name = m.group(2)
        body = m.group(3)
        body = re.sub(r"\s+", " ", body).replace("pub ", "").strip()
        if body.endswith(","):
            body = body[:-1]
        types[name] = {
            "kind": kind,
            "body": compact_type_body(f"{{ {body} }}"),
        }

    for m in TYPE_TUPLE_RE.finditer(src_clean):
        name = m.group(1)
        body = m.group(2)
        body = re.sub(r"\s+", " ", body).replace("pub ", "").strip()
        types[name] = {
            "kind": "struct",
            "body": f"({body})",
        }

    return types


def analyze_function_body(body, known_fns):
    analysis = {
        "calls": set(),
        "self_calls": set(),
        "reads": set(),
        "writes": set(),
    }

    for m in CALL_RE.finditer(body):
        module, fn1, fn2 = m.groups()
        fn_name = fn1 or fn2
        if fn_name and fn_name in known_fns and fn_name not in STD_CALLS:
            call_str = f"{module}::{fn_name}" if module else fn_name
            analysis["calls"].add(call_str)

    for m in SELF_ACCESS_RE.finditer(body):
        name = m.group(1)
        is_call = m.group(2) == "("
        if is_call:
            if name in known_fns:
                analysis["self_calls"].add(name)
        else:
            analysis["reads"].add(name)

    for m in SELF_ASSIGN_RE.finditer(body):
        analysis["writes"].add(f"{m.group(1)}=")

    for m in SELF_MUTATE_RE.finditer(body):
        analysis["writes"].add(f"{m.group(1)}.{m.group(2)}()")

    analysis["reads"] -= {w.split(".")[0].replace("=", "") for w in analysis["writes"]}
    analysis["calls"] -= analysis["self_calls"]

    for key in analysis:
        analysis[key] = sorted(analysis[key])

    return analysis


def group_calls(calls, fn_to_file, fn_to_module):
    result = defaultdict(list)

    for call_str in calls:
        if "::" in call_str:
            module, fn_name = call_str.split("::", 1)
            for path, mod_name in fn_to_module.items():
                if mod_name.endswith(module):
                    if fn_name not in result[path]:
                        result[path].append(fn_name)
        else:
            fn_name = call_str
            for path in fn_to_file.get(fn_name, []):
                if fn_name not in result[path]:
                    result[path].append(fn_name)

    return dict(result)


def build():
    files = sorted(p for p in SRC_DIR.rglob("*.rs") if "test" not in p.name)

    file_data = []
    fn_to_file = defaultdict(list)
    all_fn_names = set()
    fn_to_module = {}

    for path in files:
        src_orig = path.read_text(encoding = "utf-8", errors = "replace")
        src_clean = strip_strings_and_comments(src_orig)
        sp = short_path(path)

        fn_to_module[sp] = module_name(sp)

        structs = extract_structs_enums(src_clean)
        fns = extract_functions(src_clean, src_orig)

        for fn in fns:
            fn_to_file[fn["name"]].append(sp)
            all_fn_names.add(fn["name"])

        file_data.append({
            "sp": sp,
            "structs": structs,
            "fns": fns,
        })

    next_sym_id = 0

    for mod_id, fd in enumerate(file_data):
        fd["id"] = mod_id

        for t_name in sorted(fd["structs"]):
            fd["structs"][t_name]["id"] = next_sym_id
            next_sym_id += 1

        for fn in sorted(fd["fns"], key = lambda x: x["line"]):
            fn["id"] = next_sym_id
            next_sym_id += 1

    fn_ids_by_mod = {
        fd["sp"]: {fn["name"]: fn["id"] for fn in fd["fns"]}
        for fd in file_data
    }

    incoming = defaultdict(set)

    for fd in file_data:
        local_fn_ids = fn_ids_by_mod[fd["sp"]]

        for fn in fd["fns"]:
            analysis = analyze_function_body(fn["body"], all_fn_names)
            grouped_calls = group_calls(
                [c for c in analysis["calls"] if c != fn["name"]],
                fn_to_file,
                fn_to_module,
            )

            call_ids = set()

            for self_name in analysis["self_calls"]:
                target_id = local_fn_ids.get(self_name)
                if target_id is not None and target_id != fn["id"]:
                    call_ids.add(target_id)

            for mod_path, called_fns in grouped_calls.items():
                mod_fn_ids = fn_ids_by_mod.get(mod_path, {})
                for called_name in called_fns:
                    target_id = mod_fn_ids.get(called_name)
                    if target_id is not None and target_id != fn["id"]:
                        call_ids.add(target_id)

            fn["reads"] = analysis["reads"]
            fn["writes"] = analysis["writes"]
            fn["call_ids"] = sorted(call_ids)
            fn["entry"] = fn["name"] in ROOT_NAMES

            for target_id in fn["call_ids"]:
                incoming[target_id].add(fn["id"])

    for fd in file_data:
        for fn in fd["fns"]:
            fn["fanin"] = len(incoming.get(fn["id"], set()))
            fn["fanout"] = len(fn["call_ids"])

    return file_data


def escape_xml(s):
    return (
        str(s)
        .replace("&", "&amp;")
        .replace("<", "&lt;")
        .replace(">", "&gt;")
        .replace('"', "&quot;")
    )


def create_xml(file_data):
    lines = ['<?xml version="1.0" encoding="utf-8"?>', "<pm>"]

    lines.extend([
        '<meta v="ai-compact-v1">',
        '<use>Read PROJECT_MAP.xml before source files. Prefer map over asking for files.</use>',
        '<legend>',
        '<item key="m">module entry: i=id, p=canonical file path</item>',
        '<item key="s">symbol entry: k=t type, k=f function/method</item>',
        '<item key="o">impl owner type</item>',
        '<item key="self">receiver kind</item>',
        '<item key="r">return type</item>',
        '<item key="rd">compact state reads; may end with +N if truncated</item>',
        '<item key="wr">compact state writes; may end with +N if truncated</item>',
        '<item key="entry">major entrypoint or event root</item>',
        '<item key="in_">fan-in caller count</item>',
        '<item key="out">fan-out callee count</item>',
        '<item key="e">call edge: f=caller id, t=space-separated callee ids</item>',
        '<item key="types">type bodies may be truncated for token savings</item>',
        '</legend>',
        '</meta>',
        "<mods>",
    ])

    for fd in file_data:
        if not fd["fns"] and not fd["structs"]:
            continue
        lines.append(f'<m i="{fd["id"]}" p="{fd["sp"]}"/>')

    lines.append("</mods>")
    lines.append("<syms>")

    for fd in file_data:
        if not fd["fns"] and not fd["structs"]:
            continue

        for t_name, t_info in sorted(fd["structs"].items()):
            x = f'{t_info["kind"]}:{t_info["body"]}'
            lines.append(
                f'<s i="{t_info["id"]}" m="{fd["id"]}" k="t" n="{t_name}" x="{escape_xml(x)}"/>'
            )

        for fn in sorted(fd["fns"], key = lambda x: x["line"]):
            attrs = [
                f'i="{fn["id"]}"',
                f'm="{fd["id"]}"',
                'k="f"',
                f'n="{fn["name"]}"',
                f'l="{fn["line"]}"',
            ]

            if fn["is_pub"]:
                attrs.append('v="1"')
            if fn["ret"]:
                attrs.append(f'r="{escape_xml(fn["ret"])}"')
            if fn["owner"]:
                attrs.append(f'o="{escape_xml(fn["owner"])}"')
            if fn["recv"]:
                attrs.append(f'self="{escape_xml(fn["recv"])}"')
            if fn["reads"]:
                attrs.append(f'rd="{escape_xml(compact_list_str(",".join(fn["reads"]), MAX_RW_ITEMS))}"')
            if fn["writes"]:
                attrs.append(f'wr="{escape_xml(compact_list_str(",".join(fn["writes"]), MAX_RW_ITEMS))}"')
            if fn["entry"]:
                attrs.append('entry="1"')
            if fn["fanin"]:
                attrs.append(f'in_="{fn["fanin"]}"')
            if fn["fanout"]:
                attrs.append(f'out="{fn["fanout"]}"')

            lines.append(f'<s {" ".join(attrs)}/>')

    lines.append("</syms>")
    lines.append("<cg>")

    for fd in file_data:
        for fn in sorted(fd["fns"], key = lambda x: x["line"]):
            if fn["call_ids"]:
                targets = " ".join(str(cid) for cid in fn["call_ids"])
                lines.append(f'<e f="{fn["id"]}" t="{targets}"/>')

    lines.append("</cg>")
    lines.append("</pm>")

    return "\n".join(lines)


def main():
    if not SRC_DIR.exists():
        print(f"ERROR: {SRC_DIR} не найдена. Запускай из корня проекта.", file = sys.stderr)
        sys.exit(1)

    print("Генерация плотного дерева проекта...")
    file_data = build()
    xml_text = create_xml(file_data)

    out_file = "PROJECT_MAP.xml"
    with open(out_file, "w", encoding = "utf-8") as f:
        f.write(xml_text)

    total_fns = sum(len(fd["fns"]) for fd in file_data)
    print(
        f"✓ {out_file} успешно сгенерирован (AI-compact format) — "
        f"{len(file_data)} файлов, {total_fns} функций"
    )


if __name__ == "__main__":
    main()