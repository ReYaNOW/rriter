//! Определение иконки для файла/папки.
//! Точный match — O(1), regex — once_cell, инит один раз.
//! Вызывается только при построении дерева, не в draw-цикле.

use once_cell::sync::Lazy;
use regex::Regex;

pub mod file_icons_map {
    include!(concat!(env!("OUT_DIR"), "/file_icons_map.rs"));
}
pub mod file_icons_bytes {
    include!(concat!(env!("OUT_DIR"), "/file_icons_bytes.rs"));
}

use file_icons_bytes::{file_svg, folder_svg};
use file_icons_map::{
    file_icon_key_exact, folder_icon_key_exact, FILE_ICON_PATTERNS, FOLDER_ICON_PATTERNS,
};

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

/// `name` — имя папки в нижнем регистре. Возвращает ключ иконки (stem SVG-файла).
/// SVG-файлы папок лежат в `icons/folders/` и названы без префикса (например, `src.svg`).
pub fn folder_icon_key(name: &str) -> &'static str {
    if let Some(k) = folder_icon_key_exact(name) {
        return k;
    }
    for entry in FOLDER_REGEXES.iter() {
        if entry.re.is_match(name) {
            return entry.key;
        }
    }
    "default"
}

/// Байты SVG для ключа файла.
pub fn svg_for_key(key: &str, is_folder: bool) -> &'static [u8] {
    if is_folder {
        folder_svg(key)
    } else {
        file_svg(key)
    }
}
