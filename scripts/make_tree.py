import os
import re

def scan():
    # Список утилит, которые только засоряют карту
    NOISE = {'len', 'new', 'clear', 'default', 'push', 'contains', 'is_empty', 'as_ref', 'unwrap', 'insert', 'get', 'clone', 'resize', 'flush'}

    type_re = re.compile(r'^\s*(pub(?:\([^)]+\))?\s+)?(struct|enum|trait|impl(?:\s+[A-Za-z0-9_<> ]+\s+for)?)\s+([a-zA-Z0-9_<> ]+)', re.MULTILINE)
    # Исправленная регулярка для полных сигнатур (не обрезает типы)
    fn_re = re.compile(r'^\s*(pub(?:\([^)]+\))?\s+)?(?:async\s+)?fn\s+([a-zA-Z0-9_]+)\s*(?:<[^>]+>)?\s*\(([\s\S]*?)\)\s*(?:\s*->\s*([^{;]+))?', re.MULTILINE)
    call_re = re.compile(r'([a-zA-Z0-9_]+)\(')

    all_defs = {}
    file_data = []

    for root, _, files in os.walk("src"):
        for file in files:
            if not file.endswith(".rs"): continue
            with open(os.path.join(root, file), "r") as f:
                for m in re.finditer(r'(?:fn|struct|enum|trait)\s+([a-zA-Z0-9_]+)', f.read()):
                    all_defs[m.group(1)] = file

    for root, _, files in os.walk("src"):
        for file in files:
            if not file.endswith(".rs"): continue
            rel_path = os.path.join(root, file)
            with open(rel_path, "r") as f:
                content = f.read()
                lines = content.split('\n')

            sections = [f"\n## 📄 File: {rel_path}"]
            
            # Добавляем описание модуля из первой строки, если есть //!
            if lines and lines[0].startswith('//!'):
                sections.append(f"> {lines[0][3:].strip()}")

            for m in type_re.finditer(content):
                kind, name = m.group(2), m.group(3).strip()
                sections.append(f"#### `{kind} {name}`")
            
            for m in fn_re.finditer(content):
                name = m.group(2)
                # Собираем чистую сигнатуру без переносов строк
                args = " ".join(m.group(3).split())
                ret = f" -> {m.group(4).strip()}" if m.group(4) else ""
                full_sig = f"fn {name}({args}){ret}".replace('  ', ' ')
                
                start = m.end()
                end_idx = content.find('fn ', start)
                body = content[start:end_idx] if end_idx != -1 else content[start:]
                
                calls = set(call_re.findall(body))
                # Фильтруем шум и группируем зависимости по файлам
                deps_dict = {}
                for c in calls:
                    if c in all_defs and c != name and c.lower() not in NOISE:
                        origin = all_defs[c]
                        if origin not in deps_dict: deps_dict[origin] = []
                        if c not in deps_dict[origin]: deps_dict[origin].append(c)
                
                res_info = f"### ⚡ {full_sig}"
                if deps_dict:
                    formatted_deps = [f"{f}: {', '.join(funcs)}" for f, funcs in deps_dict.items()]
                    res_info += f"\n  - **Logic Deps:** `{'; '.join(formatted_deps[:5])}`"
                sections.append(res_info)

            file_data.append("\n".join(sections))

    with open("PROJECT_MAP.md", "w") as f:
        f.write("# 🌳 RRiter Signal Map v4.5\n")
        f.write("> AI Instructions: Use 'Logic Deps' to track cross-module flow. Utilities are hidden.\n\n")
        f.writelines(file_data)

if __name__ == "__main__": scan()