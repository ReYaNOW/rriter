//! Определение иконки для файла/папки.
//! Точный match — O(1), regex — once_cell, инит один раз.
//! Вызывается только при построении дерева, не в draw-цикле.

use once_cell::sync::Lazy;
use regex::Regex;

use crate::generated::file_icons_map::{
    file_icon_key_exact, folder_icon_key_exact,
    FILE_ICON_PATTERNS, FOLDER_ICON_PATTERNS,
};
use crate::generated::file_icons_bytes::{file_svg, folder_svg};

struct PatternEntry {
    re: Regex,
    key: &'static str,
}

static FILE_REGEXES: Lazy<Vec<PatternEntry>> = Lazy::new(|| {
    FILE_ICON_PATTERNS
        .iter()
        .filter_map(|(pat, key)| Regex::new(pat).ok().map(|re| PatternEntry { re, key }))
        .collect()
});

static FOLDER_REGEXES: Lazy<Vec<PatternEntry>> = Lazy::new(|| {
    FOLDER_ICON_PATTERNS
        .iter()
        .filter_map(|(pat, key)| Regex::new(pat).ok().map(|re| PatternEntry { re, key }))
        .collect()
});

/// `name` — имя файла в нижнем регистре. Возвращает ключ иконки.
pub fn file_icon_key(name: &str) -> &'static str {
    // 1. Точное совпадение полного имени (например "dockerfile", ".eslintrc")
    if let Some(k) = file_icon_key_exact(name) {
        return k;
    }
    // 2. Совпадение по расширению через "*.ext" (например "*.py", "*.js")
    if let Some(dot) = name.rfind('.') {
        let ext_pattern = {
            // Собираем "*.<ext>" в стековом буфере без heap-аллокации через format!
            let ext = &name[dot..]; // включая точку: ".py"
            let mut buf = [0u8; 64];
            if ext.len() + 1 < buf.len() {
                buf[0] = b'*';
                buf[1..1 + ext.len()].copy_from_slice(ext.as_bytes());
                if let Ok(s) = std::str::from_utf8(&buf[..1 + ext.len()]) {
                    file_icon_key_exact(s)
                } else {
                    None
                }
            } else {
                None
            }
        };
        if let Some(k) = ext_pattern {
            return k;
        }
    }
    // 3. Regex-паттерны (для составных имён типа "*.test.ts", "dockerfile.*")
    for entry in FILE_REGEXES.iter() {
        if entry.re.is_match(name) {
            return entry.key;
        }
    }
    "default_file"
}

/// `name` — имя папки в нижнем регистре. Возвращает ключ иконки.
/// Возвращаемый ключ всегда с префиксом `folder_`, чтобы совпадать со stems SVG-файлов.
pub fn folder_icon_key(name: &str) -> &'static str {
    // Exact match: ключи в карте вида ".src" -> возвращаемое значение без префикса
    if let Some(k) = folder_icon_key_exact(name) {
        return prepend_folder_prefix(k);
    }
    // Regex-паттерны: возвращают ключ без префикса
    for entry in FOLDER_REGEXES.iter() {
        if entry.re.is_match(name) {
            return prepend_folder_prefix(entry.key);
        }
    }
    "folder_default"
}

/// Добавляет префикс `folder_` к ключу иконки папки если его ещё нет.
/// Результат — `&'static str` из заранее построенной таблицы.
/// Используется только при построении дерева файлов, не в draw-цикле.
fn prepend_folder_prefix(key: &'static str) -> &'static str {
    // Ключи из FOLDER_ICON_PATTERNS уже без префикса ("src", "config", "docs").
    // SVG файлы на диске называются "folder_src.svg" -> stem "folder_src".
    // folder_svg() ожидает stem, поэтому добавляем префикс через статическую таблицу.
    // Используем once_cell чтобы не выделять память в hot-path.
    use once_cell::sync::Lazy;
    use std::collections::HashMap;
    // Собираем все уникальные ключи из паттернов один раз
    static PREFIX_MAP: Lazy<HashMap<&'static str, String>> = Lazy::new(|| {
        let mut m = HashMap::new();
        for &(_, key) in FOLDER_ICON_PATTERNS {
            m.entry(key).or_insert_with(|| format!("folder_{}", key));
        }
        m
    });
    // &'static str невозможен для динамических строк, поэтому возвращаем &str из leaked box
    static INTERNED: Lazy<HashMap<&'static str, &'static str>> = Lazy::new(|| {
        PREFIX_MAP.iter().map(|(&k, v)| {
            let leaked: &'static str = Box::leak(v.clone().into_boxed_str());
            (k, leaked)
        }).collect()
    });
    if key.starts_with("folder_") || key.starts_with("acf") {
        return key;
    }
    INTERNED.get(key).copied().unwrap_or("folder_default")
}

/// Байты SVG для ключа: папочные ключи (с префиксом folder_) идут в folder_svg,
/// файловые — в file_svg.
pub fn svg_for_key(key: &str) -> &'static [u8] {
    if key.starts_with("folder_") || key.starts_with("acf") {
        folder_svg(key)
    } else {
        file_svg(key)
    }
}