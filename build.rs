use std::collections::BTreeMap;
use std::fmt::Write as FmtWrite;
use std::fs;
use std::path::Path;

fn extract_str<'a>(obj: &'a str, field: &str) -> Option<&'a str> {
    let key = format!("\"{}\":", field);
    let start = obj.find(key.as_str())?;
    let after = &obj[start + key.len()..];
    let trimmed = after.trim_start();
    if !trimmed.starts_with('"') {
        return None;
    }
    let inner = &trimmed[1..];
    let mut end = 0;
    let bytes = inner.as_bytes();
    while end < bytes.len() {
        if bytes[end] == b'\\' {
            end += 2;
            continue;
        }
        if bytes[end] == b'"' {
            break;
        }
        end += 1;
    }
    Some(&inner[..end])
}

fn icon_stem(icon_path: &str) -> &str {
    let last = icon_path
        .rfind('/')
        .map(|i| &icon_path[i + 1..])
        .unwrap_or(icon_path);
    last.strip_suffix(".svg").unwrap_or(last)
}

fn parse_associations(json: &str, names_field: &str) -> Vec<(String, Vec<String>, String)> {
    let mut result = Vec::new();
    let mut search_from = 0;
    while let Some(pos) = json[search_from..].find("\"value\": {") {
        let abs = search_from + pos;
        let obj_start = abs + "\"value\": ".len();
        let mut depth = 0i32;
        let mut obj_end = obj_start;
        let bytes = json.as_bytes();
        for i in obj_start..json.len() {
            match bytes[i] {
                b'{' => depth += 1,
                b'}' => {
                    depth -= 1;
                    if depth == 0 {
                        obj_end = i + 1;
                        break;
                    }
                }
                _ => {}
            }
        }
        let obj = &json[obj_start..obj_end];
        if let Some(icon) = extract_str(obj, "icon") {
            let stem = icon_stem(icon).to_string();
            let names: Vec<String> = extract_str(obj, names_field)
                .unwrap_or("")
                .split(',')
                .map(|s| s.trim().to_ascii_lowercase())
                .filter(|s| !s.is_empty())
                .collect();
            let pattern = extract_str(obj, "pattern").unwrap_or("").to_string();
            if !stem.is_empty() && (!names.is_empty() || !pattern.is_empty()) {
                result.push((stem, names, pattern));
            }
        }
        search_from = if obj_end > search_from {
            obj_end
        } else {
            search_from + 1
        };
    }
    result
}

fn escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

fn pattern_to_rust(pat: &str, key: &str) -> Option<String> {
    let mut p = pat;
    let start = p.starts_with('^');
    let end = p.ends_with('$');
    if start {
        p = &p[1..];
    }
    if end {
        p = &p[..p.len() - 1];
    }

    let clean = |s: &str| {
        s.replace("\\.", ".")
            .replace("\\\\", "\\")
            .replace("\\-", "-")
    };

    if p.contains('[') || p.contains('(') || p.contains('|') || p.contains('+') || p.contains('?') {
        return None;
    }

    if p.starts_with(".*") && !p[2..].contains(".*") {
        let suffix = clean(&p[2..]);
        if end {
            return Some(format!(
                "    if name.ends_with(\"{}\") {{ return Some(\"{}\"); }}",
                escape(&suffix),
                escape(key)
            ));
        } else {
            return Some(format!(
                "    if name.contains(\"{}\") {{ return Some(\"{}\"); }}",
                escape(&suffix),
                escape(key)
            ));
        }
    }
    if p.ends_with(".*") && !p[..p.len() - 2].contains(".*") {
        let prefix = clean(&p[..p.len() - 2]);
        if start {
            return Some(format!(
                "    if name.starts_with(\"{}\") {{ return Some(\"{}\"); }}",
                escape(&prefix),
                escape(key)
            ));
        } else {
            return Some(format!(
                "    if name.contains(\"{}\") {{ return Some(\"{}\"); }}",
                escape(&prefix),
                escape(key)
            ));
        }
    }
    if start && end {
        if let Some(idx) = p.find(".*") {
            let prefix = clean(&p[..idx]);
            let suffix = clean(&p[idx + 2..]);
            if !prefix.contains(".*") && !suffix.contains(".*") {
                return Some(format!(
                    "    if name.starts_with(\"{}\") && name.ends_with(\"{}\") && name.len() >= {} {{ return Some(\"{}\"); }}",
                    escape(&prefix), escape(&suffix), prefix.len() + suffix.len(), escape(key)
                ));
            }
        }
    }

    if !p.contains(".*") {
        let text = clean(p);
        if start && end {
            return Some(format!(
                "    if name == \"{}\" {{ return Some(\"{}\"); }}",
                escape(&text),
                escape(key)
            ));
        } else if start {
            return Some(format!(
                "    if name.starts_with(\"{}\") {{ return Some(\"{}\"); }}",
                escape(&text),
                escape(key)
            ));
        } else if end {
            return Some(format!(
                "    if name.ends_with(\"{}\") {{ return Some(\"{}\"); }}",
                escape(&text),
                escape(key)
            ));
        } else {
            return Some(format!(
                "    if name.contains(\"{}\") {{ return Some(\"{}\"); }}",
                escape(&text),
                escape(key)
            ));
        }
    }

    None
}

fn main() {
    let manifest = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let base = Path::new(&manifest);

    println!("cargo:rerun-if-changed=src/icons/atom/icon_associations.json");
    println!("cargo:rerun-if-changed=src/icons/atom/folder_associations.json");
    println!("cargo:rerun-if-changed=src/icons/atom/icons/files");
    println!("cargo:rerun-if-changed=src/icons/atom/icons/folders");

    let out_dir = std::env::var("OUT_DIR").unwrap();
    let gen_dir = Path::new(&out_dir);

    let file_json_path = base.join("src/icons/atom/icon_associations.json");
    let folder_json_path = base.join("src/icons/atom/folder_associations.json");
    let files_svg_dir = base.join("src/icons/atom/icons/files");
    let folders_svg_dir = base.join("src/icons/atom/icons/folders");

    // Fallback если JSON ещё не положили
    if !file_json_path.exists() || !folder_json_path.exists() {
        let stub = "pub fn file_icon_key_exact(_: &str) -> Option<&'static str> { None }\n\
                    pub fn folder_icon_key_exact(_: &str) -> Option<&'static str> { None }\n\
                    pub fn match_file_pattern(_: &str) -> Option<&'static str> { None }\n\
                    pub fn match_folder_pattern(_: &str) -> Option<&'static str> { None }\n\
                    pub static FILE_ICON_FALLBACKS: &[(&str, &str)] = &[];\n\
                    pub static FOLDER_ICON_FALLBACKS: &[(&str, &str)] = &[];\n";
        fs::write(gen_dir.join("file_icons_map.rs"), stub).unwrap();
        let stub2 = "pub fn file_svg(key: &str) -> &'static [u8] { b\"\" }\n\
                     pub fn folder_svg(key: &str) -> &'static [u8] { b\"\" }\n";
        fs::write(gen_dir.join("file_icons_bytes.rs"), stub2).unwrap();
        return;
    }

    let file_json = fs::read_to_string(&file_json_path).unwrap();
    let folder_json = fs::read_to_string(&folder_json_path).unwrap();

    let file_entries = parse_associations(&file_json, "fileNames");
    let folder_entries = parse_associations(&folder_json, "folderNames");

    // -----------------------------------------------------------------------
    // Собираем реально существующие SVG файлы на диске
    // -----------------------------------------------------------------------
    let existing_files: std::collections::HashSet<String> = fs::read_dir(&files_svg_dir)
        .map(|rd| {
            rd.flatten()
                .filter_map(|e| {
                    let p = e.path();
                    if p.extension().map(|x| x == "svg").unwrap_or(false) {
                        p.file_stem().map(|s| s.to_string_lossy().into_owned())
                    } else {
                        None
                    }
                })
                .collect()
        })
        .unwrap_or_default();

    let existing_folders: std::collections::HashSet<String> = fs::read_dir(&folders_svg_dir)
        .map(|rd| {
            rd.flatten()
                .filter_map(|e| {
                    let p = e.path();
                    if p.extension().map(|x| x == "svg").unwrap_or(false) {
                        p.file_stem().map(|s| s.to_string_lossy().into_owned())
                    } else {
                        None
                    }
                })
                .collect()
        })
        .unwrap_or_default();

    // -----------------------------------------------------------------------
    // Генерируем file_icons_map.rs — match + regex массивы
    // -----------------------------------------------------------------------
    let mut map_out = String::with_capacity(512 * 1024);
    writeln!(
        map_out,
        "// @generated by build.rs — не редактировать вручную"
    )
    .unwrap();
    writeln!(map_out).unwrap();

    // exact match для файлов
    {
        let mut exact: BTreeMap<String, String> = BTreeMap::new();
        for (stem, names, _) in &file_entries {
            for name in names {
                exact.entry(name.clone()).or_insert_with(|| stem.clone());
            }
        }
        writeln!(map_out, "#[inline]").unwrap();
        writeln!(
            map_out,
            "pub fn file_icon_key_exact(name: &str) -> Option<&'static str> {{"
        )
        .unwrap();
        writeln!(map_out, "    match name {{").unwrap();
        for (name, stem) in &exact {
            writeln!(
                map_out,
                "        \"{}\" => Some(\"{}\"),",
                escape(name),
                escape(stem)
            )
            .unwrap();
        }
        writeln!(map_out, "        _ => None,").unwrap();
        writeln!(map_out, "    }}").unwrap();
        writeln!(map_out, "}}").unwrap();
        writeln!(map_out).unwrap();
    }

    // exact match для папок
    {
        let mut exact: BTreeMap<String, String> = BTreeMap::new();
        for (stem, names, _) in &folder_entries {
            for name in names {
                exact.entry(name.clone()).or_insert_with(|| stem.clone());
            }
        }
        writeln!(map_out, "#[inline]").unwrap();
        writeln!(
            map_out,
            "pub fn folder_icon_key_exact(name: &str) -> Option<&'static str> {{"
        )
        .unwrap();
        writeln!(map_out, "    match name {{").unwrap();
        for (name, stem) in &exact {
            writeln!(
                map_out,
                "        \"{}\" => Some(\"{}\"),",
                escape(name),
                escape(stem)
            )
            .unwrap();
        }
        writeln!(map_out, "        _ => None,").unwrap();
        writeln!(map_out, "    }}").unwrap();
        writeln!(map_out, "}}").unwrap();
        writeln!(map_out).unwrap();
    }

    // regex функции + fallbacks
    {
        writeln!(map_out, "#[inline]").unwrap();
        writeln!(
            map_out,
            "pub fn match_file_pattern(name: &str) -> Option<&'static str> {{"
        )
        .unwrap();
        let mut fallback_file = Vec::new();
        for (stem, _, pattern) in &file_entries {
            if !pattern.is_empty() {
                if let Some(code) = pattern_to_rust(pattern, stem) {
                    writeln!(map_out, "{}", code).unwrap();
                } else {
                    fallback_file.push((pattern.clone(), stem.clone()));
                }
            }
        }
        writeln!(map_out, "    None\n}}\n").unwrap();

        writeln!(
            map_out,
            "pub static FILE_ICON_FALLBACKS: &[(&str, &str)] = &["
        )
        .unwrap();
        for (pattern, stem) in fallback_file {
            writeln!(
                map_out,
                "    (\"{}\", \"{}\"),",
                escape(&pattern),
                escape(&stem)
            )
            .unwrap();
        }
        writeln!(map_out, "];\n").unwrap();

        writeln!(map_out, "#[inline]").unwrap();
        writeln!(
            map_out,
            "pub fn match_folder_pattern(name: &str) -> Option<&'static str> {{"
        )
        .unwrap();
        let mut fallback_folder = Vec::new();
        for (stem, _, pattern) in &folder_entries {
            if !pattern.is_empty() {
                if let Some(code) = pattern_to_rust(pattern, stem) {
                    writeln!(map_out, "{}", code).unwrap();
                } else {
                    fallback_folder.push((pattern.clone(), stem.clone()));
                }
            }
        }
        writeln!(map_out, "    None\n}}\n").unwrap();

        writeln!(
            map_out,
            "pub static FOLDER_ICON_FALLBACKS: &[(&str, &str)] = &["
        )
        .unwrap();
        for (pattern, stem) in fallback_folder {
            writeln!(
                map_out,
                "    (\"{}\", \"{}\"),",
                escape(&pattern),
                escape(&stem)
            )
            .unwrap();
        }
        writeln!(map_out, "];\n").unwrap();
    }

    fs::write(gen_dir.join("file_icons_map.rs"), map_out.as_bytes()).unwrap();

    // -----------------------------------------------------------------------
    // Генерируем file_icons_bytes.rs — include_bytes! только для существующих
    // -----------------------------------------------------------------------
    let mut bytes_out = String::with_capacity(256 * 1024);
    writeln!(
        bytes_out,
        "// @generated by build.rs — не редактировать вручную"
    )
    .unwrap();
    writeln!(bytes_out).unwrap();

    // Определяем fallback (если нет — пустой слайс)
    let file_fallback = if existing_files.contains("default") {
        format!(
            "include_bytes!(\"{}/src/icons/atom/icons/files/default.svg\")",
            escape(&manifest)
        )
    } else {
        "b\"\"".to_string()
    };
    let folder_fallback = if existing_folders.contains("default") {
        format!(
            "include_bytes!(\"{}/src/icons/atom/icons/folders/default.svg\")",
            escape(&manifest)
        )
    } else {
        "b\"\"".to_string()
    };

    writeln!(bytes_out, "pub fn file_svg(key: &str) -> &'static [u8] {{").unwrap();
    writeln!(bytes_out, "    match key {{").unwrap();
    // Только те стемы, SVG которых реально лежат на диске.
    // _dark-файлы не являются самостоятельными ключами — они используются как замена
    // для базового стема, если существуют (предпочтительны для тёмной темы).
    let mut file_stems: Vec<&String> = existing_files
        .iter()
        .filter(|s| !s.ends_with("_dark"))
        .collect();
    file_stems.sort();
    for stem in &file_stems {
        // Если есть _dark-вариант, встраиваем его байты вместо оригинала
        let dark_stem = format!("{}_dark", stem);
        let svg_file = if existing_files.contains(&dark_stem) {
            dark_stem
        } else {
            stem.to_string()
        };
        writeln!(
            bytes_out,
            "        \"{}\" => include_bytes!(\"{}/src/icons/atom/icons/files/{}.svg\"),",
            escape(stem),
            escape(&manifest),
            escape(&svg_file)
        )
        .unwrap();
    }
    writeln!(bytes_out, "        _ => {},", file_fallback).unwrap();
    writeln!(bytes_out, "    }}").unwrap();
    writeln!(bytes_out, "}}").unwrap();
    writeln!(bytes_out).unwrap();

    writeln!(bytes_out, "pub fn folder_svg(key: &str) -> &'static[u8] {{").unwrap();
    writeln!(bytes_out, "    match key {{").unwrap();
    let mut folder_stems: Vec<&String> = existing_folders.iter().collect();
    folder_stems.sort();
    for stem in &folder_stems {
        writeln!(
            bytes_out,
            "        \"{}\" => include_bytes!(\"{}/src/icons/atom/icons/folders/{}.svg\"),",
            escape(stem),
            escape(&manifest),
            escape(stem)
        )
        .unwrap();
    }
    writeln!(bytes_out, "        _ => {},", folder_fallback).unwrap();
    writeln!(bytes_out, "    }}").unwrap();
    writeln!(bytes_out, "}}").unwrap();

    fs::write(gen_dir.join("file_icons_bytes.rs"), bytes_out.as_bytes()).unwrap();

    println!(
        "cargo:info=file_icons: {} file svgs, {} folder svgs, {} file patterns, {} folder patterns",
        existing_files.len(),
        existing_folders.len(),
        file_entries
            .iter()
            .filter(|(_, _, p)| !p.is_empty())
            .count(),
        folder_entries
            .iter()
            .filter(|(_, _, p)| !p.is_empty())
            .count(),
    );
}
