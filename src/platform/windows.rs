use std::path::{Path, PathBuf};

pub(super) fn extended_length_path(path: &Path) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;

    const BACKSLASH: u16 = b'\\' as u16;
    let units = path.as_os_str().encode_wide().collect::<Vec<_>>();
    let extended = [BACKSLASH, BACKSLASH, b'?' as u16, BACKSLASH];
    if starts_with_ascii_case_insensitive(&units, &extended) {
        return nul_terminated(units);
    }

    let mut out = extended.to_vec();
    if units.starts_with(&[BACKSLASH, BACKSLASH]) {
        out.extend([b'U' as u16, b'N' as u16, b'C' as u16, BACKSLASH]);
        out.extend_from_slice(&units[2..]);
    } else {
        out.extend_from_slice(&units);
    }
    nul_terminated(out)
}

fn nul_terminated(mut units: Vec<u16>) -> Vec<u16> {
    units.push(0);
    units
}

pub(super) fn normalized_path_bytes(path: &Path) -> Vec<u8> {
    normalized_units(path)
        .into_iter()
        .flat_map(u16::to_le_bytes)
        .collect()
}

fn normalized_units(path: &Path) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    normalize_windows_units(path.as_os_str().encode_wide().collect())
}

fn normalize_windows_units(mut units: Vec<u16>) -> Vec<u16> {
    const BACKSLASH: u16 = b'\\' as u16;
    const SLASH: u16 = b'/' as u16;
    const DOT: u16 = b'.' as u16;
    const COLON: u16 = b':' as u16;

    for unit in &mut units {
        if *unit == SLASH {
            *unit = BACKSLASH;
        }
    }

    let extended_unc = [
        BACKSLASH,
        BACKSLASH,
        b'?' as u16,
        BACKSLASH,
        b'U' as u16,
        b'N' as u16,
        b'C' as u16,
        BACKSLASH,
    ];
    let extended = [BACKSLASH, BACKSLASH, b'?' as u16, BACKSLASH];
    let native = [BACKSLASH, b'?' as u16, b'?' as u16, BACKSLASH];
    if starts_with_ascii_case_insensitive(&units, &extended_unc) {
        let mut normal = vec![BACKSLASH, BACKSLASH];
        normal.extend_from_slice(&units[extended_unc.len()..]);
        units = normal;
    } else if starts_with_ascii_case_insensitive(&units, &extended) {
        units.drain(..extended.len());
    } else if starts_with_ascii_case_insensitive(&units, &native) {
        units.drain(..native.len());
    }
    units = windows_invariant_lowercase(&units);

    let (mut out, rest_start, rooted, minimum_parts) = if units.starts_with(&[BACKSLASH, BACKSLASH])
    {
        (vec![BACKSLASH, BACKSLASH], 2usize, true, 2usize)
    } else if units.len() >= 2 && units[1] == COLON {
        let rooted = units.get(2) == Some(&BACKSLASH);
        let mut start = 2usize;
        while units.get(start) == Some(&BACKSLASH) {
            start += 1;
        }
        (units[..2].to_vec(), start, rooted, 0usize)
    } else if units.first() == Some(&BACKSLASH) {
        (vec![BACKSLASH], 1usize, true, 0usize)
    } else {
        (Vec::new(), 0usize, false, 0usize)
    };

    let mut parts: Vec<Vec<u16>> = Vec::new();
    for part in units[rest_start..].split(|unit| *unit == BACKSLASH) {
        if part.is_empty() || part == [DOT] {
            continue;
        }
        if part == [DOT, DOT] {
            if parts.len() > minimum_parts
                && parts
                    .last()
                    .is_some_and(|last| last.as_slice() != [DOT, DOT])
            {
                parts.pop();
            } else if !rooted {
                parts.push(part.to_vec());
            }
            continue;
        }
        parts.push(part.to_vec());
    }

    if rooted && !out.ends_with(&[BACKSLASH]) {
        out.push(BACKSLASH);
    }
    for part in parts {
        if !out.is_empty() && !out.ends_with(&[BACKSLASH]) && !out.ends_with(&[COLON]) {
            out.push(BACKSLASH);
        } else if out.ends_with(&[COLON]) && rooted {
            out.push(BACKSLASH);
        }
        out.extend_from_slice(&part);
    }
    if out.is_empty() {
        out.push(DOT);
    }
    out
}

fn starts_with_ascii_case_insensitive(value: &[u16], prefix: &[u16]) -> bool {
    value.len() >= prefix.len()
        && value.iter().zip(prefix).all(|(left, right)| {
            ascii_lower_u16(*left) == ascii_lower_u16(*right)
        })
}

fn ascii_lower_u16(value: u16) -> u16 {
    if (b'A' as u16..=b'Z' as u16).contains(&value) {
        value + (b'a' - b'A') as u16
    } else {
        value
    }
}

fn windows_invariant_lowercase(units: &[u16]) -> Vec<u16> {
    use windows_sys::Win32::Globalization::{
        LCMAP_LOWERCASE, LCMapStringEx, LOCALE_NAME_INVARIANT,
    };

    if units.is_empty() || units.len() > i32::MAX as usize {
        return units.iter().copied().map(ascii_lower_u16).collect();
    }
    let required = unsafe {
        LCMapStringEx(
            LOCALE_NAME_INVARIANT,
            LCMAP_LOWERCASE,
            units.as_ptr(),
            units.len() as i32,
            std::ptr::null_mut(),
            0,
            std::ptr::null(),
            std::ptr::null(),
            0,
        )
    };
    if required <= 0 {
        return units.iter().copied().map(ascii_lower_u16).collect();
    }
    let mut lowered = vec![0u16; required as usize];
    let written = unsafe {
        LCMapStringEx(
            LOCALE_NAME_INVARIANT,
            LCMAP_LOWERCASE,
            units.as_ptr(),
            units.len() as i32,
            lowered.as_mut_ptr(),
            lowered.len() as i32,
            std::ptr::null(),
            std::ptr::null(),
            0,
        )
    };
    if written <= 0 {
        units.iter().copied().map(ascii_lower_u16).collect()
    } else {
        lowered.truncate(written as usize);
        lowered
    }
}

pub(super) fn path_is_within(path: &Path, root: &Path) -> bool {
    const BACKSLASH: u16 = b'\\' as u16;
    let path = normalized_units(path);
    let root = normalized_units(root);
    path == root
        || (path.starts_with(&root)
            && (root.last() == Some(&BACKSLASH)
                || path.get(root.len()) == Some(&BACKSLASH)))
}

fn windows_component_key(component: std::path::Component<'_>) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    normalize_windows_units(component.as_os_str().encode_wide().collect())
}

pub(super) fn relative_to(path: &Path, root: &Path) -> Option<PathBuf> {
    if !path_is_within(path, root) {
        return None;
    }
    let path_components = path.components().collect::<Vec<_>>();
    let root_components = root.components().collect::<Vec<_>>();
    if root_components.len() > path_components.len()
        || !root_components
            .iter()
            .zip(&path_components)
            .all(|(root, path)| windows_component_key(*root) == windows_component_key(*path))
    {
        return None;
    }
    let mut relative = PathBuf::new();
    for component in &path_components[root_components.len()..] {
        relative.push(component.as_os_str());
    }
    Some(relative)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;
    use std::os::windows::ffi::{OsStrExt, OsStringExt};

    #[test]
    fn path_key_preserves_distinct_unpaired_surrogates() {
        let first = PathBuf::from(OsString::from_wide(&[
            b'C' as u16,
            b':' as u16,
            b'\\' as u16,
            0xd800,
        ]));
        let second = PathBuf::from(OsString::from_wide(&[
            b'C' as u16,
            b':' as u16,
            b'\\' as u16,
            0xd801,
        ]));
        assert_ne!(normalized_path_bytes(&first), normalized_path_bytes(&second));
    }

    #[test]
    fn extended_paths_keep_wtf16_and_convert_unc_prefix() {
        let path = PathBuf::from(OsString::from_wide(&[
            b'\\' as u16,
            b'\\' as u16,
            b'S' as u16,
            b'r' as u16,
            b'v' as u16,
            b'\\' as u16,
            b'S' as u16,
            b'h' as u16,
            b'a' as u16,
            b'r' as u16,
            b'e' as u16,
            b'\\' as u16,
            0xd800,
        ]));
        let encoded = extended_length_path(&path);
        assert_eq!(
            &encoded[..8],
            &[
                b'\\' as u16,
                b'\\' as u16,
                b'?' as u16,
                b'\\' as u16,
                b'U' as u16,
                b'N' as u16,
                b'C' as u16,
                b'\\' as u16,
            ]
        );
        assert_eq!(encoded[encoded.len() - 2], 0xd800);
        assert_eq!(encoded.last(), Some(&0));
        assert_eq!(path.as_os_str().encode_wide().last(), Some(0xd800));
    }
}
