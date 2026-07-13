use std::path::{Path, PathBuf};

fn colorization_argb_to_rgba(color: u32) -> [f32; 4] {
    [
        ((color >> 16) & 0xff) as f32 / 255.0,
        ((color >> 8) & 0xff) as f32 / 255.0,
        (color & 0xff) as f32 / 255.0,
        1.0,
    ]
}

pub(super) fn system_accent_color() -> Option<[f32; 4]> {
    use windows_sys::Win32::Graphics::Dwm::DwmGetColorizationColor;

    let mut color = 0u32;
    let mut opaque_blend = 0;
    let result = unsafe { DwmGetColorizationColor(&mut color, &mut opaque_blend) };
    (result >= 0).then(|| colorization_argb_to_rgba(color))
}

#[inline(always)]
pub(super) fn finish_present() {
    use windows_sys::Win32::Graphics::Dwm::DwmFlush;

    let _ = unsafe { DwmFlush() };
}

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


pub(super) fn without_extended_prefix(path: &Path) -> PathBuf {
    use std::os::windows::ffi::{OsStrExt, OsStringExt};

    const BACKSLASH: u16 = b'\\' as u16;
    let units = path.as_os_str().encode_wide().collect::<Vec<_>>();
    let extended_unc = [
        BACKSLASH, BACKSLASH, b'?' as u16, BACKSLASH,
        b'U' as u16, b'N' as u16, b'C' as u16, BACKSLASH,
    ];
    let extended = [BACKSLASH, BACKSLASH, b'?' as u16, BACKSLASH];
    let native = [BACKSLASH, b'?' as u16, b'?' as u16, BACKSLASH];

    let normal = if starts_with_ascii_case_insensitive(&units, &extended_unc) {
        let mut normal = vec![BACKSLASH, BACKSLASH];
        normal.extend_from_slice(&units[extended_unc.len()..]);
        normal
    } else if starts_with_ascii_case_insensitive(&units, &extended) {
        units[extended.len()..].to_vec()
    } else if starts_with_ascii_case_insensitive(&units, &native) {
        units[native.len()..].to_vec()
    } else {
        return path.to_path_buf();
    };

    PathBuf::from(std::ffi::OsString::from_wide(&normal))
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



pub(super) fn run_elevated_helper(executable: &Path, request: &Path) -> std::io::Result<i32> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Foundation::{CloseHandle, WAIT_FAILED, WAIT_TIMEOUT};
    use windows_sys::Win32::System::Threading::{
        GetExitCodeProcess, TerminateProcess, WaitForSingleObject,
    };
    use windows_sys::Win32::UI::Shell::{
        SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW, ShellExecuteExW,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_HIDE;

    fn wide(value: &std::ffi::OsStr) -> Vec<u16> {
        value.encode_wide().chain(std::iter::once(0)).collect()
    }

    let verb = wide(std::ffi::OsStr::new("runas"));
    let executable = wide(executable.as_os_str());
    let mut parameter_units = "--rriter-elevated-save \""
        .encode_utf16()
        .collect::<Vec<_>>();
    parameter_units.extend(request.as_os_str().encode_wide());
    parameter_units.extend(['"' as u16, 0]);

    let mut info = unsafe { std::mem::zeroed::<SHELLEXECUTEINFOW>() };
    info.cbSize = std::mem::size_of::<SHELLEXECUTEINFOW>() as u32;
    info.fMask = SEE_MASK_NOCLOSEPROCESS;
    info.lpVerb = verb.as_ptr();
    info.lpFile = executable.as_ptr();
    info.lpParameters = parameter_units.as_ptr();
    info.nShow = SW_HIDE;

    if unsafe { ShellExecuteExW(&mut info) } == 0 {
        let error = std::io::Error::last_os_error();
        if error.raw_os_error() == Some(1223) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "Windows elevation was cancelled",
            ));
        }
        return Err(error);
    }
    if info.hProcess.is_null() {
        return Err(std::io::Error::other(
            "ShellExecuteExW did not return an elevated process handle",
        ));
    }

    let wait = unsafe { WaitForSingleObject(info.hProcess, 120_000) };
    if wait == WAIT_TIMEOUT {
        unsafe {
            let _ = TerminateProcess(info.hProcess, 1);
            let _ = WaitForSingleObject(info.hProcess, 5_000);
            let _ = CloseHandle(info.hProcess);
        }
        return Err(std::io::Error::new(
            std::io::ErrorKind::TimedOut,
            "Windows elevated save helper timed out",
        ));
    }
    if wait == WAIT_FAILED {
        let error = std::io::Error::last_os_error();
        unsafe {
            let _ = CloseHandle(info.hProcess);
        }
        return Err(error);
    }

    let mut exit_code = 1_u32;
    let status = unsafe { GetExitCodeProcess(info.hProcess, &mut exit_code) };
    unsafe {
        let _ = CloseHandle(info.hProcess);
    }
    if status == 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(exit_code as i32)
    }
}

pub(super) fn initialize_gui_application() {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::UI::Shell::SetCurrentProcessExplicitAppUserModelID;

    let app_id = std::ffi::OsStr::new("RRiter.Editor")
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let result = unsafe { SetCurrentProcessExplicitAppUserModelID(app_id.as_ptr()) };
    if result < 0 {
        eprintln!(
            "RRiter could not set Windows AppUserModelID (HRESULT 0x{:08x})",
            result as u32
        );
    }
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

    #[test]
    fn visible_paths_drop_extended_prefix_without_changing_case() {
        assert_eq!(
            without_extended_prefix(Path::new(r"\\?\C:\Users\ReYaN\Project")),
            PathBuf::from(r"C:\Users\ReYaN\Project")
        );
        assert_eq!(
            without_extended_prefix(Path::new(r"\\?\UNC\Server\Share\File.txt")),
            PathBuf::from(r"\\Server\Share\File.txt")
        );
    }

    #[test]
    fn colorization_argb_is_decoded_as_aarrggbb() {
        assert_eq!(
            colorization_argb_to_rgba(0x7f7259af),
            [114.0 / 255.0, 89.0 / 255.0, 175.0 / 255.0, 1.0]
        );
    }
}

pub(super) fn protect_user_secret(bytes: &[u8], purpose: &str) -> std::io::Result<Vec<u8>> {
    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Cryptography::{
        CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN, CryptProtectData,
    };

    let input = CRYPT_INTEGER_BLOB {
        cbData: u32::try_from(bytes.len()).map_err(|_| {
            std::io::Error::new(std::io::ErrorKind::InvalidInput, "secret is too large")
        })?,
        pbData: bytes.as_ptr().cast_mut(),
    };
    let entropy_bytes = purpose.as_bytes();
    let entropy = CRYPT_INTEGER_BLOB {
        cbData: u32::try_from(entropy_bytes.len()).map_err(|_| {
            std::io::Error::new(std::io::ErrorKind::InvalidInput, "secret purpose is too large")
        })?,
        pbData: entropy_bytes.as_ptr().cast_mut(),
    };
    let mut output = CRYPT_INTEGER_BLOB::default();
    let ok = unsafe {
        CryptProtectData(
            &raw const input,
            std::ptr::null(),
            &raw const entropy,
            std::ptr::null(),
            std::ptr::null(),
            CRYPTPROTECT_UI_FORBIDDEN,
            &raw mut output,
        )
    };
    if ok == 0 {
        return Err(std::io::Error::last_os_error());
    }
    let protected = unsafe {
        std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec()
    };
    unsafe {
        LocalFree(output.pbData.cast());
    }
    Ok(protected)
}

pub(super) fn unprotect_user_secret(bytes: &[u8], purpose: &str) -> std::io::Result<Vec<u8>> {
    use windows_sys::Win32::Foundation::{LocalFree, HLOCAL};
    use windows_sys::Win32::Security::Cryptography::{
        CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN, CryptUnprotectData,
    };

    let input = CRYPT_INTEGER_BLOB {
        cbData: u32::try_from(bytes.len()).map_err(|_| {
            std::io::Error::new(std::io::ErrorKind::InvalidInput, "secret is too large")
        })?,
        pbData: bytes.as_ptr().cast_mut(),
    };
    let entropy_bytes = purpose.as_bytes();
    let entropy = CRYPT_INTEGER_BLOB {
        cbData: u32::try_from(entropy_bytes.len()).map_err(|_| {
            std::io::Error::new(std::io::ErrorKind::InvalidInput, "secret purpose is too large")
        })?,
        pbData: entropy_bytes.as_ptr().cast_mut(),
    };
    let mut output = CRYPT_INTEGER_BLOB::default();
    let mut description = std::ptr::null_mut();
    let ok = unsafe {
        CryptUnprotectData(
            &raw const input,
            &raw mut description,
            &raw const entropy,
            std::ptr::null(),
            std::ptr::null(),
            CRYPTPROTECT_UI_FORBIDDEN,
            &raw mut output,
        )
    };
    if ok == 0 {
        return Err(std::io::Error::last_os_error());
    }
    let plain = unsafe {
        std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec()
    };
    unsafe {
        LocalFree(output.pbData.cast());
        if !description.is_null() {
            LocalFree(description.cast::<core::ffi::c_void>() as HLOCAL);
        }
    }
    Ok(plain)
}

pub(super) fn native_root_certificates_der() -> std::io::Result<Vec<Vec<u8>>> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Security::Cryptography::{
        CERT_CONTEXT, CertCloseStore, CertEnumCertificatesInStore, CertOpenSystemStoreW,
    };

    let store_name = std::ffi::OsStr::new("ROOT")
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let store = unsafe { CertOpenSystemStoreW(0, store_name.as_ptr()) };
    if store.is_null() {
        return Err(std::io::Error::last_os_error());
    }

    let mut certificates = Vec::new();
    let mut previous: *const CERT_CONTEXT = std::ptr::null();
    loop {
        let current = unsafe { CertEnumCertificatesInStore(store, previous) };
        if current.is_null() {
            break;
        }
        let context = unsafe { &*current };
        if !context.pbCertEncoded.is_null() && context.cbCertEncoded > 0 {
            certificates.push(unsafe {
                std::slice::from_raw_parts(
                    context.pbCertEncoded,
                    context.cbCertEncoded as usize,
                )
                .to_vec()
            });
        }
        previous = current;
    }
    unsafe {
        CertCloseStore(store, 0);
    }
    Ok(certificates)
}

pub(super) fn raw_system_proxy_config() -> Option<(String, Option<String>)> {
    use windows_sys::Win32::Foundation::GlobalFree;
    use windows_sys::Win32::Networking::WinHttp::{
        WINHTTP_CURRENT_USER_IE_PROXY_CONFIG, WinHttpGetIEProxyConfigForCurrentUser,
    };

    let mut config = WINHTTP_CURRENT_USER_IE_PROXY_CONFIG::default();
    if unsafe { WinHttpGetIEProxyConfigForCurrentUser(&raw mut config) } == 0 {
        return None;
    }
    let proxy = wide_ptr_to_string(config.lpszProxy);
    let bypass = wide_ptr_to_string(config.lpszProxyBypass);
    unsafe {
        if !config.lpszAutoConfigUrl.is_null() {
            GlobalFree(config.lpszAutoConfigUrl.cast());
        }
        if !config.lpszProxy.is_null() {
            GlobalFree(config.lpszProxy.cast());
        }
        if !config.lpszProxyBypass.is_null() {
            GlobalFree(config.lpszProxyBypass.cast());
        }
    }
    proxy.map(|proxy| (proxy, bypass))
}

fn wide_ptr_to_string(value: windows_sys::core::PCWSTR) -> Option<String> {
    if value.is_null() {
        return None;
    }
    let mut len = 0usize;
    unsafe {
        while *value.add(len) != 0 {
            len = len.saturating_add(1);
        }
        Some(String::from_utf16_lossy(std::slice::from_raw_parts(value, len)))
    }
}

pub(super) fn current_process_memory_kb() -> Option<usize> {
    use windows_sys::Win32::System::ProcessStatus::{
        GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS,
    };
    use windows_sys::Win32::System::Threading::GetCurrentProcess;

    let mut counters = PROCESS_MEMORY_COUNTERS::default();
    counters.cb = std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32;
    let ok = unsafe {
        GetProcessMemoryInfo(
            GetCurrentProcess(),
            &raw mut counters,
            counters.cb,
        )
    };
    (ok != 0).then_some(counters.WorkingSetSize / 1024)
}
