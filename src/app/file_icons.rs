//! Определение иконки для файла/папки.
//! Точный match — O(1), fallback-паттерны — generated checks без compiled regex cache.
//! Вызывается только при построении дерева, не в draw-цикле.

use std::borrow::Cow;

pub mod file_icons_map {
    include!(concat!(env!("OUT_DIR"), "/file_icons_map.rs"));
}
pub mod file_icons_bytes {
    include!(concat!(env!("OUT_DIR"), "/file_icons_bytes.rs"));
}

use file_icons_bytes::{file_svg, folder_svg};
use file_icons_map::{
    file_icon_key_exact, folder_icon_key_exact, match_file_pattern, match_folder_pattern,
};

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

pub(super) fn simple_regex_match(name: &str, pattern: &str) -> bool {
    let text = name.as_bytes();
    let pat = pattern.as_bytes();
    if pat.first() == Some(&b'^') {
        return match_pattern_range(pat, 1, pat.len(), text, 0);
    }
    for start in 0..=text.len() {
        if match_pattern_range(pat, 0, pat.len(), text, start) {
            return true;
        }
    }
    false
}

fn match_pattern_range(pat: &[u8], start: usize, end: usize, text: &[u8], pos: usize) -> bool {
    let mut accept = |_| true;
    pattern_positions_match(pat, start, end, text, pos, &mut accept)
}

fn pattern_positions_match(
    pat: &[u8],
    start: usize,
    end: usize,
    text: &[u8],
    pos: usize,
    accept: &mut dyn FnMut(usize) -> bool,
) -> bool {
    if start >= end {
        return accept(pos);
    }
    if pat[start] == b'$' && start + 1 == end {
        return pos == text.len() && accept(pos);
    }

    let atom_end = regex_atom_end(pat, start, end);
    let quant = if atom_end < end {
        match pat[atom_end] {
            b'?' | b'*' | b'+' => Some(pat[atom_end]),
            _ => None,
        }
    } else {
        None
    };
    let next = atom_end + usize::from(quant.is_some());

    match quant {
        Some(b'?') => {
            pattern_positions_match(pat, next, end, text, pos, accept)
                || atom_positions_match(pat, start, atom_end, text, pos, &mut |next_pos| {
                    pattern_positions_match(pat, next, end, text, next_pos, accept)
                })
        }
        Some(b'*') => {
            pattern_positions_match(pat, next, end, text, pos, accept)
                || repeat_atom_positions_match(pat, start, atom_end, text, pos, next, end, accept)
        }
        Some(b'+') => atom_positions_match(pat, start, atom_end, text, pos, &mut |next_pos| {
            if next_pos == pos {
                return false;
            }
            pattern_positions_match(pat, next, end, text, next_pos, accept)
                || repeat_atom_positions_match(
                    pat, start, atom_end, text, next_pos, next, end, accept,
                )
        }),
        _ => atom_positions_match(pat, start, atom_end, text, pos, &mut |next_pos| {
            pattern_positions_match(pat, next, end, text, next_pos, accept)
        }),
    }
}

fn repeat_atom_positions_match(
    pat: &[u8],
    atom_start: usize,
    atom_end: usize,
    text: &[u8],
    pos: usize,
    rest_start: usize,
    end: usize,
    accept: &mut dyn FnMut(usize) -> bool,
) -> bool {
    atom_positions_match(pat, atom_start, atom_end, text, pos, &mut |next_pos| {
        if next_pos == pos {
            return false;
        }
        pattern_positions_match(pat, rest_start, end, text, next_pos, accept)
            || repeat_atom_positions_match(
                pat, atom_start, atom_end, text, next_pos, rest_start, end, accept,
            )
    })
}

fn atom_positions_match(
    pat: &[u8],
    start: usize,
    end: usize,
    text: &[u8],
    pos: usize,
    accept: &mut dyn FnMut(usize) -> bool,
) -> bool {
    if start >= end {
        return accept(pos);
    }
    match pat[start] {
        b'(' if end > start + 1 => {
            let inner_start = start + 1;
            let inner_end = end.saturating_sub(1);
            let mut alt_start = inner_start;
            let mut idx = inner_start;
            while idx <= inner_end {
                if idx == inner_end
                    || (pat[idx] == b'|' && regex_group_depth(pat, inner_start, idx) == 0)
                {
                    if pattern_positions_match(pat, alt_start, idx, text, pos, accept) {
                        return true;
                    }
                    alt_start = idx.saturating_add(1);
                }
                idx += 1;
            }
            false
        }
        b'[' => {
            if pos < text.len() && class_matches(&pat[start + 1..end.saturating_sub(1)], text[pos])
            {
                accept(pos + 1)
            } else {
                false
            }
        }
        b'.' => {
            if pos < text.len() {
                accept(pos + 1)
            } else {
                false
            }
        }
        b'\\' => {
            if pos < text.len() && escaped_atom_matches(pat.get(start + 1).copied(), text[pos]) {
                accept(pos + 1)
            } else {
                false
            }
        }
        literal => {
            if pos < text.len() && text[pos] == literal {
                accept(pos + 1)
            } else {
                false
            }
        }
    }
}

fn regex_atom_end(pat: &[u8], start: usize, end: usize) -> usize {
    match pat[start] {
        b'\\' => (start + 2).min(end),
        b'[' => find_regex_close(pat, start, end, b'[', b']').unwrap_or((start + 1).min(end)),
        b'(' => find_regex_close(pat, start, end, b'(', b')').unwrap_or((start + 1).min(end)),
        _ => (start + 1).min(end),
    }
}

fn find_regex_close(pat: &[u8], start: usize, end: usize, open: u8, close: u8) -> Option<usize> {
    let mut depth = 0usize;
    let mut idx = start;
    while idx < end {
        match pat[idx] {
            b'\\' => idx += 1,
            byte if byte == open => depth += 1,
            byte if byte == close => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return Some(idx + 1);
                }
            }
            _ => {}
        }
        idx += 1;
    }
    None
}

fn regex_group_depth(pat: &[u8], start: usize, end: usize) -> usize {
    let mut depth = 0usize;
    let mut idx = start;
    while idx < end {
        match pat[idx] {
            b'\\' => idx += 1,
            b'(' => depth += 1,
            b')' => depth = depth.saturating_sub(1),
            _ => {}
        }
        idx += 1;
    }
    depth
}

fn escaped_atom_matches(escaped: Option<u8>, byte: u8) -> bool {
    match escaped {
        Some(b'd') => byte.is_ascii_digit(),
        Some(b'w') => byte.is_ascii_alphanumeric() || byte == b'_',
        Some(other) => byte == other,
        None => false,
    }
}

fn class_matches(class: &[u8], byte: u8) -> bool {
    let mut idx = 0usize;
    let negated = class.first() == Some(&b'^');
    if negated {
        idx = 1;
    }
    let mut matched = false;
    while idx < class.len() {
        let (start, next_idx) = class_atom(class, idx);
        idx = next_idx;
        if idx + 1 < class.len() && class[idx] == b'-' {
            let (end, after_range) = class_atom(class, idx + 1);
            idx = after_range;
            matched |= byte >= start && byte <= end;
        } else {
            matched |= byte == start;
        }
    }
    matched != negated
}

fn class_atom(class: &[u8], idx: usize) -> (u8, usize) {
    if class.get(idx) == Some(&b'\\') && idx + 1 < class.len() {
        (class[idx + 1], idx + 2)
    } else {
        (class[idx], idx + 1)
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

    #[test]
    fn simple_icon_regex_matcher_covers_generated_fallback_shapes() {
        assert!(simple_regex_match(
            "workflow.yaml",
            "(main|workflow|ci|release|build|config)\\.ya?ml$"
        ));
        assert!(simple_regex_match(
            "readme.md",
            "^(README|readme)(\\.(md|txt))?$"
        ));
        assert!(simple_regex_match(".github", "^[\\._]?github$"));
        assert!(simple_regex_match("adapter", "^[\\._]?adapters?$"));
        assert!(simple_regex_match(
            "vite.config.ts",
            "^vite\\.config\\.[jt]s$"
        ));
        assert!(!simple_regex_match(
            "vite.config.rs",
            "^vite\\.config\\.[jt]s$"
        ));
    }
}
