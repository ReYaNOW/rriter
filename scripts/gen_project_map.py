#!/usr/bin/env python3
"""gen_project_map.py — генератор PROJECT_MAP.md для RRiter."""
import re, sys
from collections import defaultdict
from pathlib import Path

SRC_DIR = Path("src")

# Шум — stdlib примитивы не интересны как вызовы
NOISE = {
    "unwrap","expect","map","and_then","or_else","ok","err","is_some","is_none",
    "len","is_empty","collect","clone","to_string","as_str","as_bytes","chars",
    "bytes","lines","split","trim","starts_with","ends_with","replace",
    "println","eprintln","print","eprint","min","max","abs","clamp","round",
    "floor","ceil","sqrt","from","into","default","get","set","remove","entry",
    "or_insert","lock","read","write","send","recv","try_recv","sort","sort_by",
    "sort_unstable_by_key","dedup","retain","extend","drain","parse","to_owned",
    "borrow","as_ref","as_mut","zip","enumerate","filter","flat_map","for_each",
    "any","all","first","last","next","peekable","take","skip","map_err",
    "unwrap_or","unwrap_or_else","unwrap_or_default","ok_or","ok_or_else",
    "transpose","flatten","chain","rev","sum","product","count","position",
    "find","fold","reduce","with_capacity","resize","truncate","swap",
    "contains_key","get_mut","values","keys","iter_mut","values_mut","to_vec",
    "join","splitn","split_once","strip_prefix","strip_suffix",
    "to_ascii_lowercase","to_uppercase","is_ascii","is_alphabetic","is_numeric",
    "is_alphanumeric","is_whitespace","saturating_add","saturating_sub",
    "checked_add","checked_sub","wrapping_add","wrapping_sub","pow",
    "min_by","max_by","min_by_key","max_by_key","copied","cloned","unzip",
    "partition","scan","take_while","skip_while","step_by","windows","chunks",
    "split_at","new","drop","hash","is_char_boundary","iter","into_iter",
    "contains","format","insert",
}

_SKIP_VARIANTS = {
    'None','Some','Ok','Err','True','False','WindowEvent','KeyEvent',
    'PhysicalKey','MouseScrollDelta','ElementState','ModifiersState',
}

def short_path(p): return str(p).replace("\\", "/")
def module_name(sp): return sp.removeprefix("src/").removesuffix(".rs").replace("/", "::")

# ── Очистка строк/комментариев ────────────────────────────────────────────────

def strip_strings_and_comments(src):
    result = list(src)
    i = 0; n = len(src)
    while i < n:
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
        if src[i] == 'r' and i+1 < n and src[i+1] in '#"':
            hashes = 0; j = i+1
            while j < n and src[j] == '#': hashes += 1; j += 1
            if j < n and src[j] == '"':
                close = '"' + '#'*hashes
                end = src.find(close, j+1)
                end = (end + len(close)) if end != -1 else n
                for k in range(i, end):
                    if result[k] != '\n': result[k] = ' '
                i = end; continue
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

# ── Функции ───────────────────────────────────────────────────────────────────

FN_RE = re.compile(
    r'^([ \t]*)(pub(?:\s*\([^)]*\))?\s+)?(?:async\s+)?(?:unsafe\s+)?fn\s+(\w+)([^{]*)\{',
    re.MULTILINE
)

def extract_functions(src_clean, src_orig):
    fns = []
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
        fns.append({"name": name, "ret": ret, "is_pub": is_pub,
                    "body": body, "line": line_no})
    return fns

# ── Типы ─────────────────────────────────────────────────────────────────────

def extract_structs_enums(src):
    pat = re.compile(r'(?:^|\n)\s*(?:pub(?:\([^)]*\))?\s+)?(?:struct|enum)\s+(\w+)')
    return list(dict.fromkeys(m.group(1) for m in pat.finditer(src)))

ENUM_VARIANT_RE = re.compile(r'^\s*([A-Z][A-Za-z0-9_]*)', re.MULTILINE)

def extract_enum_variants(src_clean, src_orig):
    result = {}
    enum_re = re.compile(r'(?:pub\s+)?enum\s+(\w+)[^{]*\{')
    for m in enum_re.finditer(src_clean):
        enum_name = m.group(1)
        body, _ = extract_body(src_clean, m.end()-1)
        variants = [v.group(1) for v in ENUM_VARIANT_RE.finditer(body)
                    if v.group(1) not in _SKIP_VARIANTS]
        if variants:
            result[enum_name] = variants
    return result

# ── Вызовы fn проекта ─────────────────────────────────────────────────────────

# Ловим: word( или word.word( или word.word.word(
# Для фильтрации stdlib-вызовов на полях структуры нужно знать
# полный префикс. Берём до 2 уровней: a.b.fn( — это stdlib на поле.
CALL_RE = re.compile(r'(?:(\w+)\s*\.\s*)?(\w+)\s*\.\s*(\w+)\s*\(|(?:(\w+)\s*\.\s*)?(\w+)\s*\(')

def extract_calls(body, known):
    seen = {}
    for m in CALL_RE.finditer(body):
        if m.group(3) is not None:
            # Форма a.b.fn( — fn вызывается на поле b объекта a → это stdlib, пропускаем
            continue
        recv = m.group(4) or ""
        fn_name = m.group(5)
        if fn_name in NOISE or fn_name not in known:
            continue
        seen[(recv, fn_name)] = True
    return list(seen.keys())

def group_by_file(calls, fn_to_file):
    result = defaultdict(list)
    for _, fn_name in calls:
        for f in fn_to_file.get(fn_name, []):
            if fn_name not in result[f]:
                result[f].append(fn_name)
    return dict(result)

# ── self.field мутации ────────────────────────────────────────────────────────
# Ловим:
#   self.field = ...          (присваивание)
#   self.field.push(...)      (мутирующий метод на поле)
#   self.field.clear()
#   self.field.pop()
#   self.field.insert(...)
#   self.field.extend(...)
#   self.field.retain(...)
#   self.field.remove(...)
#   self.field.truncate(...)
#   self.field.sort...()
#   self.field.dedup()
#   self.field.drain(...)

SELF_ASSIGN_RE = re.compile(r'\bself\.(\w+)\s*=\s*')  # self.field =
SELF_MUTATE_RE = re.compile(
    r'\bself\.(\w+)\.'
    r'(push|pop|clear|insert|extend|retain|remove|truncate|sort(?:_by|_unstable_by_key)?|dedup|drain|push_back|pop_back|push_front|pop_front)\s*\('
)

def extract_self_mutations(body):
    fields = {}
    for m in SELF_ASSIGN_RE.finditer(body):
        fields[m.group(1)] = True
    for m in SELF_MUTATE_RE.finditer(body):
        field = m.group(1)
        method = m.group(2)
        fields[f"{field}.{method}"] = True
    return sorted(fields.keys())

# ── Match паттерны ────────────────────────────────────────────────────────────

MATCH_PAT_RE = re.compile(
    r'\b([A-Z][A-Za-z0-9_]+)::([A-Z][A-Za-z0-9_]+)\s*(?:=>|\{|\()'
)

def extract_match_arms(body):
    found = {}
    match_re = re.compile(r'\bmatch\b[^{]*\{')
    for m in match_re.finditer(body):
        match_body, _ = extract_body(body, m.end()-1)
        for arm in MATCH_PAT_RE.finditer(match_body):
            e, v = arm.group(1), arm.group(2)
            if e not in _SKIP_VARIANTS and v not in _SKIP_VARIANTS:
                found[f"{e}::{v}"] = True
    return sorted(found.keys())

# ── Сборка ────────────────────────────────────────────────────────────────────

def collect_files():
    return sorted(p for p in SRC_DIR.rglob("*.rs") if "test" not in p.name)

def build():
    files = collect_files()
    file_data = []
    fn_to_file = defaultdict(list)
    all_fn_names = set()

    for path in files:
        src_orig = path.read_text(encoding="utf-8", errors="replace")
        src_clean = strip_strings_and_comments(src_orig)
        sp = short_path(path)
        structs = extract_structs_enums(src_orig)
        enum_variants = extract_enum_variants(src_clean, src_orig)
        fns = extract_functions(src_clean, src_orig)
        for fn in fns:
            fn_to_file[fn["name"]].append(sp)
            all_fn_names.add(fn["name"])
        file_data.append({"sp": sp, "structs": structs, "fns": fns,
                          "enum_variants": enum_variants,
                          "src_orig": src_orig, "src_clean": src_clean})

    for fd in file_data:
        for fn in fd["fns"]:
            calls = extract_calls(fn["body"], all_fn_names)
            calls = [(r, n) for r, n in calls if n != fn["name"]]
            fn["calls"] = group_by_file(calls, fn_to_file)
            fn["match_arms"] = extract_match_arms(fn["body"])
            fn["mutations"] = extract_self_mutations(fn["body"])

    return file_data

# ── Рендер ────────────────────────────────────────────────────────────────────

SEP = "─" * 60

def render(file_data):
    out = []
    out.append("# RRiter PROJECT_MAP")
    out.append("# AUTO-GENERATED. Команда: make api-tree (python3 gen_project_map.py)")
    out.append("#")
    out.append("# pub fn name  [LINE] -> RetType")
    out.append("#   SELF:  fn1, fn2            <- вызовы fn из этого же файла")
    out.append("#   CALL module: fn1, fn2      <- вызовы fn из другого файла")
    out.append("#   WRITE: field, other.push   <- self.field мутации / присваивания")
    out.append("#   MATCH: Enum::Variant        <- паттерны match-веток")
    out.append("# enum Name: Var1, Var2         <- варианты enum")
    out.append("")

    for fd in file_data:
        sp = fd["sp"]
        fns = fd["fns"]
        structs = fd["structs"]
        enum_variants = fd.get("enum_variants", {})
        if not fns and not structs:
            continue

        out.append(f"FILE {sp}")
        out.append(f"  module: {module_name(sp)}")
        if structs:
            out.append(f"  types:  {', '.join(structs)}")
        for ename, variants in sorted(enum_variants.items()):
            out.append(f"  enum {ename}: {', '.join(variants)}")
        out.append("")

        for fn in fns:
            pub = "pub " if fn["is_pub"] else "    "
            ret = f" -> {fn['ret']}" if fn["ret"] else ""
            out.append(f"  {pub}fn {fn['name']}  [{fn['line']}]{ret}")

            calls = fn.get("calls", {})
            same = sorted(calls.get(sp, []))
            other = {k: sorted(v) for k, v in calls.items() if k != sp}
            arms  = fn.get("match_arms", [])
            muts  = fn.get("mutations", [])

            if same:  out.append(f"    SELF:  {', '.join(same)}")
            for of, names in sorted(other.items()):
                out.append(f"    CALL {module_name(of)}: {', '.join(names)}")
            if muts:  out.append(f"    WRITE: {', '.join(muts)}")
            if arms:  out.append(f"    MATCH: {', '.join(arms)}")
            out.append("")

        out.append(SEP)
        out.append("")

    return "\n".join(out)

# ── Main ─────────────────────────────────────────────────────────────────────

def main():
    if not SRC_DIR.exists():
        print(f"ERROR: {SRC_DIR} не найдена. Запускай из корня проекта.", file=sys.stderr)
        sys.exit(1)
    file_data = build()
    text = render(file_data)
    with open("PROJECT_MAP.md", "w", encoding="utf-8") as f:
        f.write(text)
    total_fns = sum(len(fd["fns"]) for fd in file_data)
    print(f"✓ PROJECT_MAP.md обновлён — {len(file_data)} файлов, {total_fns} функций")

if __name__ == "__main__":
    main()
# (патч применяется отдельно)