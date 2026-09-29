use serde_json::Value;
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub struct Manifest {
    pub version: String,
    pub platform: Option<PlatformEntry>,
}

#[derive(Debug)]
pub struct PlatformEntry {
    pub archive: String,
    pub sha256: String,
    pub lib: String,
}

impl Manifest {
    pub fn parse_for_key(json: &str, key: &str) -> Result<Self, String> {
        let value: Value = serde_json::from_str(json).map_err(|error| error.to_string())?;
        let version = value
            .get("version")
            .and_then(Value::as_str)
            .ok_or_else(|| "manifest version missing".to_owned())?
            .to_owned();
        let platforms = value
            .get("platforms")
            .and_then(Value::as_object)
            .ok_or_else(|| "manifest platforms missing".to_owned())?;
        let Some(value) = platforms.get(key) else {
            return Ok(Self { version, platform: None });
        };
        let archive = value
            .get("archive")
            .and_then(Value::as_str)
            .ok_or_else(|| format!("archive missing for {key}"))?
            .to_owned();
        let sha256 = value
            .get("sha256")
            .and_then(Value::as_str)
            .ok_or_else(|| format!("sha256 missing for {key}"))?
            .to_owned();
        if sha256.len() != 64 || !sha256.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(format!("bad sha256 for {key}"));
        }
        let lib = value
            .get("lib")
            .and_then(Value::as_str)
            .ok_or_else(|| format!("lib missing for {key}"))?
            .to_owned();
        Ok(Self {
            version,
            platform: Some(PlatformEntry { archive, sha256, lib }),
        })
    }
}

pub fn manifest() -> Result<Manifest, String> {
    Manifest::parse_for_key(include_str!("../../pdfium.json"), current_platform_key())
}

fn current_platform_key() -> &'static str {
    if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        "linux-x64"
    } else if cfg!(all(target_os = "windows", target_arch = "x86_64")) {
        "win-x64"
    } else if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        "mac-arm64"
    } else if cfg!(all(target_os = "macos", target_arch = "x86_64")) {
        "mac-x64"
    } else {
        "unsupported"
    }
}

pub fn version_slug(version: &str) -> String {
    version.replace('/', "-")
}

pub fn lib_name(entry: &PlatformEntry) -> &str {
    entry.lib.rsplit('/').next().unwrap_or(entry.lib.as_str())
}

pub fn archive_url(version: &str, entry: &PlatformEntry) -> String {
    format!(
        "https://github.com/bblanchon/pdfium-binaries/releases/download/{version}/{}",
        entry.archive
    )
}

pub fn managed_dir() -> PathBuf {
    let root = crate::platform::data_dir().join("tools/managed/pdfium");
    match manifest() {
        Ok(parsed) => root.join(version_slug(&parsed.version)),
        Err(_) => root,
    }
}

pub fn managed_path() -> Option<PathBuf> {
    let parsed = manifest().ok()?;
    let entry = parsed.platform?;
    Some(managed_dir().join(lib_name(&entry)))
}

/// `Missing` message of an engine that is simply not installed (as opposed to an install failure).
pub const NOT_FOUND_MESSAGE: &str = "библиотека PDF-движка не найдена";

pub enum LocateResult {
    Found(PathBuf),
    Missing { message: String, installable: bool },
}

pub fn candidate_paths(exe_dir: &Path, managed_dir: &Path, name: &str) -> Vec<PathBuf> {
    let mut paths = vec![exe_dir.join(name)];
    #[cfg(target_os = "macos")]
    paths.push(exe_dir.join("../Frameworks").join(name));
    paths.push(managed_dir.join(name));
    paths
}

pub fn locate() -> LocateResult {
    if let Some(path) = std::env::var_os("RRITER_PDFIUM_PATH") {
        let path = PathBuf::from(path);
        return if path.is_file() {
            LocateResult::Found(path)
        } else {
            LocateResult::Missing {
                message: format!(
                    "RRITER_PDFIUM_PATH указывает на несуществующий файл: {}",
                    path.display()
                ),
                installable: false,
            }
        };
    }

    let Ok(parsed) = manifest() else {
        return LocateResult::Missing {
            message: "платформа не поддерживается".to_owned(),
            installable: false,
        };
    };
    let Some(entry) = parsed.platform else {
        return LocateResult::Missing {
            message: "платформа не поддерживается".to_owned(),
            installable: false,
        };
    };
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(Path::to_path_buf))
        .unwrap_or_default();
    let managed = crate::platform::data_dir()
        .join("tools/managed/pdfium")
        .join(version_slug(&parsed.version));
    for candidate in candidate_paths(&exe_dir, &managed, lib_name(&entry)) {
        if candidate.is_file() {
            return LocateResult::Found(candidate);
        }
    }
    LocateResult::Missing {
        message: NOT_FOUND_MESSAGE.to_owned(),
        installable: true,
    }
}

#[cfg(test)]
mod tests {
    use super::{LocateResult, Manifest, archive_url, candidate_paths, locate, manifest, version_slug};
    use std::fs;
    use std::path::PathBuf;

    #[test]
    fn parses_current_platform_manifest() {
        let parsed = manifest().expect("manifest should parse");
        assert_eq!(parsed.version, "chromium/8066");
        let entry = parsed.platform.expect("current platform should exist");
        assert_eq!(entry.sha256.len(), 64);
        assert!(entry.sha256.chars().all(|ch| ch.is_ascii_hexdigit()));
        assert!(!entry.lib.is_empty());
    }

    #[test]
    fn handles_invalid_manifest_inputs() {
        let unsupported = Manifest::parse_for_key(
            r#"{"version":"v","platforms":{}}"#,
            "haiku-ppc",
        )
        .expect("unsupported platform is valid manifest");
        assert!(unsupported.platform.is_none());
        assert!(Manifest::parse_for_key("{", "linux-x64").is_err());
        let bad_hash = r#"{"version":"v","platforms":{"linux-x64":{"archive":"a","sha256":"xyz","lib":"libpdfium.so"}}}"#;
        assert_eq!(
            Manifest::parse_for_key(bad_hash, "linux-x64").unwrap_err(),
            "bad sha256 for linux-x64"
        );
    }

    #[test]
    fn version_and_archive_url_are_stable() {
        let parsed = manifest().expect("manifest should parse");
        let entry = parsed.platform.expect("current platform should exist");
        assert_eq!(version_slug("chromium/8066"), "chromium-8066");
        #[cfg(target_os = "linux")]
        assert_eq!(
            archive_url("chromium/8066", &entry),
            "https://github.com/bblanchon/pdfium-binaries/releases/download/chromium/8066/pdfium-linux-x64.tgz"
        );
    }

    #[test]
    fn candidate_paths_follow_platform_search_order() {
        let exe_dir = PathBuf::from("exe");
        let managed_dir = PathBuf::from("managed");
        let paths = candidate_paths(&exe_dir, &managed_dir, "libpdfium.so");
        #[cfg(target_os = "macos")]
        assert_eq!(
            paths,
            vec![
                exe_dir.join("libpdfium.so"),
                exe_dir.join("../Frameworks/libpdfium.so"),
                managed_dir.join("libpdfium.so"),
            ]
        );
        #[cfg(not(target_os = "macos"))]
        assert_eq!(
            paths,
            vec![exe_dir.join("libpdfium.so"), managed_dir.join("libpdfium.so")]
        );
    }

    #[test]
    fn explicit_environment_path_is_exclusive() {
        let root = std::env::temp_dir().join(format!("rriter_pdf_locate_{}", std::process::id()));
        fs::create_dir_all(&root).expect("temp directory should be created");
        let missing = root.join("nope");
        unsafe { std::env::set_var("RRITER_PDFIUM_PATH", &missing) };
        match locate() {
            LocateResult::Missing { message, installable } => {
                assert!(!installable);
                assert!(message.contains(&missing.to_string_lossy().to_string()));
            }
            LocateResult::Found(path) => panic!("unexpected path: {}", path.display()),
        }

        let existing = root.join("pdfium");
        fs::write(&existing, b"stub").expect("temp file should be created");
        unsafe { std::env::set_var("RRITER_PDFIUM_PATH", &existing) };
        assert!(matches!(locate(), LocateResult::Found(path) if path == existing));
        unsafe { std::env::remove_var("RRITER_PDFIUM_PATH") };
        let _ = fs::remove_file(existing);
        let _ = fs::remove_dir(root);
    }
}
