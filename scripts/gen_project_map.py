#!/usr/bin/env python3
"""gen_project_map.py — генератор идеального PROJECT_MAP.xml (структуры + логика)."""
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
    "push", "pop", "insert", "clear", "push_back", "pop_back", "push_front", "pop_front"
}

def short_path(p): return str(p).replace("\\", "/")
def module_name(sp): return sp.removeprefix("src/").removesuffix(".rs").replace("/", "::")

def strip_strings_and_comments(src):
    result = list(src)
    i = 0; n = len(src)
    while i < n:
        if src[i] == "'" and i+2 < n and src[i+2] == "'":
            result[i+1] = ' '; i += 3; continue
        if src[i] == '/' and i+1 < n and src[i+1] == '/':
            j = i
            while j < n and src[j] != '\n': result[j] = ' '; j += 1
            i = j; continue
        if src[i] == '/' and i+1 < n and src[i+1] == '*':
            result[i] = result[i+1] = ' '; j = i+2
            while j < n-1:
                if src[j] == '*' and src[j+1] == '/':
                    result[j] = result[j+1] = ' '; j += 2; break
                if src[j] != '\n': result[j] = ' '
                j += 1
            i = j; continue
        if src[i] == '"':
            result[i] = ' '; j = i+1
            while j < n:
                if src[j] == '\\' and j+1 < n:
                    result[j] = result[j+1] = ' '; j += 2; continue
                if src[j] == '"': result[j] = ' '; j += 1; break
                if src[j] != '\n': result[j] = ' '
                j += 1
            i = j; continue
        i += 1
    return "".join(result)

def extract_body(src, brace_pos):
    depth = 1; i = brace_pos+1; n = len(src)
    while i < n and depth > 0:
        if src[i] == '{': depth += 1
        elif src[i] == '}': depth -= 1
        i += 1
    return src[brace_pos+1:i-1], i

FN_RE = re.compile(
    r'^([ \t]*)(pub(?:\s*\([^)]*\))?\s+)?(?:async\s+)?(?:unsafe\s+)?fn\s+(\w+)([^{]*)\{',
    re.MULTILINE
)

def extract_functions(src_clean, src_orig):
    fns =[]
    for m in FN_RE.finditer(src_clean):
        brace_pos = m.end()-1
        body, _ = extract_body(src_clean, brace_pos)
        name = m.group(3)
        is_pub = bool(m.group(2) and 'pub' in m.group(2))
        sig_start = m.start()
        sig_raw = src_orig[sig_start:m.end()].split('{')[0].strip()
        sig_clean = re.sub(r'\s+', ' ', sig_raw)
        ret_m = re.search(r'->\s*(.+?)(?:\s+where\b.*)?$', sig_clean)
        ret = ret_m.group(1).strip() if ret_m else ""
        line_no = src_orig[:sig_start].count('\n') + 1
        fns.append({"name": name, "ret": ret, "is_pub": is_pub, "body": body, "line": line_no})
    return fns

def extract_structs_enums(src_clean):
    types = {}
    
    # 1. Обычные структуры и энумы: struct App { ... }
    pat_block = re.compile(r'(?:pub\s+)?(?:struct|enum)\s+([a-zA-Z0-9_]+)[^{]*\{([^}]*)\}')
    for m in pat_block.finditer(src_clean):
        name = m.group(1)
        body = m.group(2)
        # Ужимаем пробелы и убираем pub
        body = re.sub(r'\s+', ' ', body).replace('pub ', '').strip()
        if body.endswith(','): body = body[:-1]
        types[name] = f"{{ {body} }}"

    # 2. Кортежные структуры: struct Color(f32, f32);
    pat_tuple = re.compile(r'(?:pub\s+)?struct\s+([a-zA-Z0-9_]+)[^(]*\(([^)]+)\)\s*;')
    for m in pat_tuple.finditer(src_clean):
        name = m.group(1)
        body = m.group(2)
        body = re.sub(r'\s+', ' ', body).replace('pub ', '').strip()
        types[name] = f"({body})"

    return types

CALL_RE = re.compile(r'\b(\w+)::(\w+)\s*\(|\b(\w+)\s*\(')
SELF_ACCESS_RE = re.compile(r'\bself\.([a-zA-Z0-9_]+)\s*(\()?')
SELF_MUTATE_RE = re.compile(r'\bself\.([a-zA-Z0-9_]+)\.(push|pop|clear|insert|extend|retain|remove|truncate|sort|drain|push_back|pop_back)\b')
SELF_ASSIGN_RE = re.compile(r'\bself\.([a-zA-Z0-9_]+)\s*=[^=]')

def analyze_function_body(body, known_fns):
    analysis = {"calls": set(), "self_calls": set(), "reads": set(), "writes": set()}

    for m in CALL_RE.finditer(body):
        module, fn1, fn2 = m.groups()
        fn_name = fn1 or fn2
        if fn_name and fn_name in known_fns and fn_name not in STD_CALLS:
            call_str = f"{module}::{fn_name}" if module else fn_name
            analysis["calls"].add(call_str)

    for m in SELF_ACCESS_RE.finditer(body):
        name = m.group(1)
        is_call = m.group(2) == '('
        if is_call:
            if name in known_fns: 
                 analysis["self_calls"].add(name)
        else:
            analysis["reads"].add(name)

    for m in SELF_ASSIGN_RE.finditer(body):
        analysis["writes"].add(f"{m.group(1)}=")

    for m in SELF_MUTATE_RE.finditer(body):
        analysis["writes"].add(f"{m.group(1)}.{m.group(2)}()")

    analysis["reads"] -= {w.split('.')[0].replace('=', '') for w in analysis["writes"]}
    analysis["calls"] -= analysis["self_calls"]

    for key in analysis: analysis[key] = sorted(list(analysis[key]))
    return analysis

def group_calls(calls, fn_to_file, fn_to_module):
    result = defaultdict(list)
    for call_str in calls:
        if '::' in call_str:
            module, fn_name = call_str.split('::', 1)
            for path, mod_name in fn_to_module.items():
                if mod_name.endswith(module):
                     if fn_name not in result[path]: result[path].append(fn_name)
        else:
            fn_name = call_str
            for f in fn_to_file.get(fn_name,[]):
                 if fn_name not in result[f]: result[f].append(fn_name)
    return dict(result)

def build():
    files = sorted(p for p in SRC_DIR.rglob("*.rs") if "test" not in p.name)
    file_data =[]
    fn_to_file = defaultdict(list)
    all_fn_names = set()
    fn_to_module = {}

    for path in files:
        src_orig = path.read_text(encoding="utf-8", errors="replace")
        src_clean = strip_strings_and_comments(src_orig)
        sp = short_path(path)
        fn_to_module[sp] = module_name(sp)

        structs = extract_structs_enums(src_clean)
        fns = extract_functions(src_clean, src_orig)
        
        for fn in fns:
            fn_to_file[fn["name"]].append(sp)
            all_fn_names.add(fn["name"])
            
        file_data.append({"sp": sp, "structs": structs, "fns": fns})

    for fd in file_data:
        for fn in fd["fns"]:
            analysis = analyze_function_body(fn["body"], all_fn_names)
            analysis["calls"] = group_calls([c for c in analysis["calls"] if c != fn["name"]], fn_to_file, fn_to_module)
            fn.update(analysis)
    return file_data

def escape_xml(s):
    return str(s).replace("&", "&amp;").replace("<", "&lt;").replace(">", "&gt;").replace('"', "&quot;")

def create_xml(file_data):
    lines =['<?xml version="1.0" encoding="utf-8"?>', '<project_map>']
    
    for fd in file_data:
        if not fd["fns"] and not fd["structs"]: continue
        mod_name = module_name(fd["sp"])
        lines.append(f'  <mod path="{fd["sp"]}" name="{mod_name}">')
        
        if fd["structs"]:
            for t_name, t_body in sorted(fd["structs"].items()):
                # Записываем сжатое тело структуры (с типами!)
                lines.append(f'    <type name="{t_name}">{escape_xml(t_body)}</type>')
        
        for fn in sorted(fd["fns"], key=lambda x: x['line']):
            attrs = [f'name="{fn["name"]}"', f'line="{fn["line"]}"']
            if fn["is_pub"]: attrs.append('pub="true"')
            if fn["ret"]: attrs.append(f'ret="{escape_xml(fn["ret"])}"')
            
            if fn["reads"]: attrs.append(f'reads="{escape_xml(", ".join(fn["reads"]))}"')
            if fn["writes"]: attrs.append(f'writes="{escape_xml(", ".join(fn["writes"]))}"')
            
            calls_list =[]
            if fn["self_calls"]:
                calls_list.append(f"self: {', '.join(fn['self_calls'])}")
            for mod_path, called_fns in sorted(fn["calls"].items()):
                target_mod = "self" if mod_path == fd["sp"] else module_name(mod_path)
                calls_list.append(f"{target_mod}: {', '.join(sorted(called_fns))}")
                
            if calls_list:
                attrs.append(f'calls="{escape_xml("; ".join(calls_list))}"')
                
            lines.append(f'    <fn {" ".join(attrs)} />')
            
        lines.append('  </mod>')
    
    lines.append('</project_map>')
    return "\n".join(lines)

def main():
    if not SRC_DIR.exists():
        print(f"ERROR: {SRC_DIR} не найдена. Запускай из корня проекта.", file=sys.stderr)
        sys.exit(1)
        
    print("Генерация плотного дерева проекта...")
    file_data = build()
    xml_text = create_xml(file_data)
    
    out_file = "PROJECT_MAP.xml"
    with open(out_file, "w", encoding="utf-8") as f:
        f.write(xml_text)
        
    total_fns = sum(len(fd["fns"]) for fd in file_data)
    print(f"✓ {out_file} успешно сгенерирован (Ультра-компактный формат) — {len(file_data)} файлов, {total_fns} функций")

if __name__ == "__main__":
    main()