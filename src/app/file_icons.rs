//! Определение иконки для файла/папки.
//! Точный match — O(1), regex — once_cell, инит один раз.
//! Вызывается только при построении дерева, не в draw-цикле.

use once_cell::sync::Lazy;
use regex::RegexSet;
use std::borrow::Cow;

pub mod file_icons_map {
    include!(concat!(env!("OUT_DIR"), "/file_icons_map.rs"));
}
pub mod file_icons_bytes {
    include!(concat!(env!("OUT_DIR"), "/file_icons_bytes.rs"));
}

use file_icons_bytes::{file_svg, folder_svg};
use file_icons_map::{
    FILE_ICON_FALLBACKS, FOLDER_ICON_FALLBACKS, file_icon_key_exact, folder_icon_key_exact,
    match_file_pattern, match_folder_pattern,
};

struct FallbackMatcher {
    set: RegexSet,
    keys: Vec<&'static str>,
}

static FILE_REGEXES: Lazy<FallbackMatcher> = Lazy::new(|| {
    let patterns: Vec<&str> = FILE_ICON_FALLBACKS.iter().map(|(p, _)| *p).collect();
    let keys: Vec<&'static str> = FILE_ICON_FALLBACKS.iter().map(|(_, k)| *k).collect();
    let set = RegexSet::new(patterns).unwrap_or_else(|_| RegexSet::empty());
    FallbackMatcher { set, keys }
});

static FOLDER_REGEXES: Lazy<FallbackMatcher> = Lazy::new(|| {
    let patterns: Vec<&str> = FOLDER_ICON_FALLBACKS.iter().map(|(p, _)| *p).collect();
    let keys: Vec<&'static str> = FOLDER_ICON_FALLBACKS.iter().map(|(_, k)| *k).collect();
    let set = RegexSet::new(patterns).unwrap_or_else(|_| RegexSet::empty());
    FallbackMatcher { set, keys }
});

fn lowercase_name_for_icon_lookup(name: &str) -> Cow<'_, str> {
    if name.chars().any(char::is_uppercase) {
        Cow::Owned(name.to_lowercase())
    } else {
        Cow::Borrowed(name)
    }
}

pub fn file_icon_key_for_name(name: &str) -> &'static str {
    let lower = lowercase_name_for_icon_lookup(name);
    file_icon_key(lower.as_ref())
}

pub fn folder_icon_key_for_name(name: &str) -> &'static str {
    let lower = lowercase_name_for_icon_lookup(name);
    folder_icon_key(lower.as_ref())
}

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
    // 3. Быстрые статические паттерны из build.rs
    if let Some(k) = match_file_pattern(name) {
        return k;
    }

    // 4. Оставшиеся сложные regex проверяются за ОДИН проход!
    if let Some(idx) = FILE_REGEXES.set.matches(name).into_iter().next() {
        return FILE_REGEXES.keys[idx];
    }
    "default_file"
}

/// `name` — имя папки в нижнем регистре. Возвращает ключ иконки (stem SVG-файла).
/// SVG-файлы папок лежат в `icons/folders/` и названы без префикса (например, `src.svg`).
pub fn folder_icon_key(name: &str) -> &'static str {
    if let Some(k) = folder_icon_key_exact(name) {
        return k;
    }
    if let Some(k) = match_folder_pattern(name) {
        return k;
    }
    if let Some(idx) = FOLDER_REGEXES.set.matches(name).into_iter().next() {
        return FOLDER_REGEXES.keys[idx];
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_icon_resolution_covers_exact_extension_pattern_and_fallback() {
        assert_ne!(file_icon_key("cargo.toml"), "default_file");
        assert_ne!(file_icon_key("main.py"), "default_file");
        assert_ne!(file_icon_key("component.test.ts"), "default_file");
        assert_eq!(file_icon_key("unknown.rriter-no-icon"), "default_file");
        assert_eq!(file_icon_key_for_name("MAIN.PY"), file_icon_key("main.py"));

        assert!(!svg_for_key(file_icon_key("main.py"), false).is_empty());
        assert!(!svg_for_key("default_file", false).is_empty());
    }

    #[test]
    fn folder_icon_resolution_covers_exact_pattern_and_fallback() {
        assert_ne!(folder_icon_key("src"), "default");
        assert_ne!(folder_icon_key(".github"), "default");
        assert_eq!(folder_icon_key("rriter-no-icon-folder"), "default");
        assert_eq!(folder_icon_key_for_name("SRC"), folder_icon_key("src"));

        assert!(!svg_for_key(folder_icon_key("src"), true).is_empty());
        assert!(!svg_for_key("default", true).is_empty());
    }
}
