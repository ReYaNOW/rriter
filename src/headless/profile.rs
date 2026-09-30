//! Headless command-line options and the isolated profile root that replaces the user's
//! config/data/cache/state directories for the whole headless process.

use crate::app::automation::{AutomationOptions, PgoScenario};
use crate::headless::protocol::{parse_f64, parse_scale, parse_size};
use crate::platform::{self, AppPaths};
use std::ffi::{OsStr, OsString};
use std::fs::{self, DirBuilder};
use std::io;
use std::os::unix::fs::DirBuilderExt;
use std::path::{Path, PathBuf};
use std::time::Duration;

const DEFAULT_SIZE: (u32, u32) = (1920, 1080);
/// Window of a `--pgo-train` run unless `--size`/`--scale` say otherwise: the training window
/// the GUI pipeline used, so the layout paths that get profiled are the same.
const PGO_SIZE: (u32, u32) = (2560, 1440);
const PGO_SCALE: f64 = 1.333;
const PGO_DEFAULT_TIMEOUT_SECONDS: u64 = 240;
const PGO_MIN_TIMEOUT_SECONDS: u64 = 30;
const PGO_DEFAULT_REPORT: &str = "rriter-pgo-automation-report.json";
const DEFAULT_RUNTIME_BASE: &str = "/tmp";
const TEMP_ROOT_PREFIX: &str = "rriter-headless-";
/// Upper bound on `rriter-headless-<pid>-<n>` names tried before giving up.
const TEMP_ROOT_ATTEMPTS: u32 = 1000;

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct HeadlessOptions {
    pub script: Option<PathBuf>,
    pub size: (u32, u32),
    pub scale: f64,
    pub profile: ProfileChoice,
    pub budget: BudgetChoice,
    pub keep_profile: bool,
    pub allow_writes: bool,
    pub path: Option<PathBuf>,
    /// `--pgo-train`: run this scenario headlessly instead of reading protocol commands.
    pub automation: Option<AutomationOptions>,
}

impl Default for HeadlessOptions {
    fn default() -> Self {
        Self {
            script: None,
            size: DEFAULT_SIZE,
            scale: 1.0,
            profile: ProfileChoice::Temp,
            budget: BudgetChoice::Auto,
            keep_profile: false,
            allow_writes: false,
            path: None,
            automation: None,
        }
    }
}

/// Raw `--pgo-*` flag values, turned into `AutomationOptions` once `--pgo-train` is known.
#[derive(Default)]
struct PgoFlags {
    train: bool,
    scenario: Option<PgoScenario>,
    workspace: Option<PathBuf>,
    report: Option<PathBuf>,
    timeout_seconds: Option<u64>,
    /// First `--pgo-*` flag other than `--pgo-train`, for the "requires --pgo-train" error.
    first_extra: Option<&'static str>,
}

impl PgoFlags {
    fn build(self) -> Result<AutomationOptions, String> {
        let workspace = match self.workspace {
            Some(workspace) => workspace,
            None => std::env::current_dir()
                .map_err(|e| format!("cannot determine the PGO workspace: {e}"))?,
        };
        let workspace = std::path::absolute(&workspace)
            .map_err(|e| format!("--pgo-workspace {}: {e}", workspace.display()))?;
        let report_path = self.report.unwrap_or_else(|| workspace.join(PGO_DEFAULT_REPORT));
        let timeout_seconds = self.timeout_seconds.unwrap_or(PGO_DEFAULT_TIMEOUT_SECONDS);
        if timeout_seconds < PGO_MIN_TIMEOUT_SECONDS {
            return Err(format!(
                "--pgo-timeout-seconds must be at least {PGO_MIN_TIMEOUT_SECONDS}"
            ));
        }
        Ok(AutomationOptions {
            workspace,
            report_path,
            timeout: Duration::from_secs(timeout_seconds),
            scenario: self.scenario.unwrap_or(PgoScenario::Full),
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum ProfileChoice {
    Temp,
    Dir(PathBuf),
    FromUser,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum BudgetChoice {
    Auto,
    Hz(f64),
    Ms(f64),
}

/// Parses headless arguments (without the program name). `--headless` itself is ignored.
pub(crate) fn parse_args(args: &[OsString]) -> Result<HeadlessOptions, String> {
    let mut opts = HeadlessOptions::default();
    let mut profile_dir: Option<PathBuf> = None;
    let mut from_user = false;
    let mut hz: Option<f64> = None;
    let mut budget_ms: Option<f64> = None;
    let mut size_given = false;
    let mut scale_given = false;
    let mut pgo = PgoFlags::default();
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        let flag = match arg.to_str() {
            Some(s) if s.starts_with('-') && s != "-" => s,
            _ => {
                if opts.path.is_some() {
                    return Err(format!(
                        "unexpected extra argument '{}'",
                        arg.to_string_lossy()
                    ));
                }
                if arg.is_empty() {
                    return Err("empty FILE_OR_DIR argument".to_string());
                }
                opts.path = Some(PathBuf::from(arg));
                continue;
            }
        };
        match flag {
            "--headless" => {}
            "--script" => opts.script = Some(path_value(flag, iter.next())?),
            "--size" => {
                opts.size = parse_size(str_value(flag, iter.next())?)?;
                size_given = true;
            }
            "--scale" => {
                opts.scale = parse_scale(str_value(flag, iter.next())?)?;
                scale_given = true;
            }
            "--profile" => profile_dir = Some(path_value(flag, iter.next())?),
            "--profile-from-user" => from_user = true,
            "--hz" => hz = Some(positive(flag, str_value(flag, iter.next())?)?),
            "--budget-ms" => budget_ms = Some(positive(flag, str_value(flag, iter.next())?)?),
            "--keep-profile" => opts.keep_profile = true,
            "--allow-writes" => opts.allow_writes = true,
            "--pgo-train" => pgo.train = true,
            "--pgo-scenario" => {
                let text = str_value(flag, iter.next())?;
                pgo.scenario = Some(PgoScenario::parse(text).map_err(|e| format!("{flag}: {e}"))?);
                pgo.first_extra.get_or_insert("--pgo-scenario");
            }
            "--pgo-workspace" => {
                pgo.workspace = Some(path_value(flag, iter.next())?);
                pgo.first_extra.get_or_insert("--pgo-workspace");
            }
            "--pgo-report" => {
                pgo.report = Some(path_value(flag, iter.next())?);
                pgo.first_extra.get_or_insert("--pgo-report");
            }
            "--pgo-timeout-seconds" => {
                let text = str_value(flag, iter.next())?;
                pgo.timeout_seconds = Some(
                    text.parse::<u64>().map_err(|_| format!("{flag}: invalid value {text:?}"))?,
                );
                pgo.first_extra.get_or_insert("--pgo-timeout-seconds");
            }
            _ => return Err(format!("unknown option '{flag}'")),
        }
    }
    opts.budget = match (hz, budget_ms) {
        (Some(_), Some(_)) => return Err("--hz and --budget-ms are mutually exclusive".to_string()),
        (Some(hz), None) => BudgetChoice::Hz(hz),
        (None, Some(ms)) => BudgetChoice::Ms(ms),
        (None, None) => BudgetChoice::Auto,
    };
    opts.profile = match (profile_dir, from_user) {
        (Some(_), true) => {
            return Err("--profile and --profile-from-user are mutually exclusive".to_string());
        }
        (Some(dir), false) => ProfileChoice::Dir(dir),
        (None, true) => ProfileChoice::FromUser,
        (None, false) => ProfileChoice::Temp,
    };
    if !pgo.train {
        return match pgo.first_extra {
            Some(flag) => Err(format!("{flag} requires --pgo-train")),
            None => Ok(opts),
        };
    }
    if opts.script.is_some() || opts.path.is_some() {
        return Err("--pgo-train takes neither --script nor FILE_OR_DIR".to_string());
    }
    // The scenario writes into its workspace (fixtures, report, saved session).
    opts.allow_writes = true;
    if !size_given {
        opts.size = PGO_SIZE;
    }
    if !scale_given {
        opts.scale = PGO_SCALE;
    }
    opts.automation = Some(pgo.build()?);
    Ok(opts)
}

/// Returns the flag value; a missing value or another `--flag` in its place is an error.
fn raw_value<'a>(flag: &str, value: Option<&'a OsString>) -> Result<&'a OsStr, String> {
    match value {
        Some(v) if !v.is_empty() && !v.to_str().is_some_and(|s| s.starts_with("--")) => Ok(v),
        _ => Err(format!("missing value for {flag}")),
    }
}

fn path_value(flag: &str, value: Option<&OsString>) -> Result<PathBuf, String> {
    raw_value(flag, value).map(PathBuf::from)
}

fn str_value<'a>(flag: &str, value: Option<&'a OsString>) -> Result<&'a str, String> {
    raw_value(flag, value)?
        .to_str()
        .ok_or_else(|| format!("{flag}: value is not valid UTF-8"))
}

fn positive(flag: &str, t: &str) -> Result<f64, String> {
    let value = parse_f64(t).map_err(|e| format!("{flag}: {e}"))?;
    if value <= 0.0 {
        return Err(format!("{flag} must be greater than 0"));
    }
    Ok(value)
}

/// Profile root prepared before the app starts; `finish` removes it when this run created it.
#[derive(Debug)]
pub(crate) struct Profile {
    root: PathBuf,
    /// True only for roots created by `prepare_in` for `Temp`/`FromUser`.
    remove_on_finish: bool,
}

impl Profile {
    /// Must run before `set_app_root_override`, so `app_paths()` still yields the user's directories.
    pub(crate) fn prepare(opts: &HeadlessOptions) -> Result<Profile, String> {
        Self::prepare_in(opts, &runtime_base(), &platform::app_paths())
    }

    pub(crate) fn prepare_in(
        opts: &HeadlessOptions,
        runtime_base: &Path,
        user_paths: &AppPaths,
    ) -> Result<Profile, String> {
        match &opts.profile {
            ProfileChoice::Dir(dir) => {
                fs::create_dir_all(dir).map_err(|e| {
                    format!("cannot create profile directory {}: {e}", dir.display())
                })?;
                Ok(Profile { root: dir.clone(), remove_on_finish: false })
            }
            ProfileChoice::Temp => {
                let root = create_temp_root(runtime_base)?;
                Ok(Profile { root, remove_on_finish: !opts.keep_profile })
            }
            ProfileChoice::FromUser => {
                let root = create_temp_root(runtime_base)?;
                if let Err(err) = copy_user_state(user_paths, &root) {
                    // The root is ours and still unused: do not leave a partial copy behind.
                    let _ = fs::remove_dir_all(&root);
                    return Err(format!("cannot copy user profile into {}: {err}", root.display()));
                }
                Ok(Profile { root, remove_on_finish: !opts.keep_profile })
            }
        }
    }

    pub(crate) fn root(&self) -> &Path {
        &self.root
    }

    /// Removes the root only if this run created it and `--keep-profile` was not given.
    pub(crate) fn finish(self) {
        if !self.remove_on_finish {
            return;
        }
        if let Err(err) = fs::remove_dir_all(&self.root) {
            eprintln!("headless: cannot remove profile {}: {err}", self.root.display());
        }
    }
}

pub(crate) fn runtime_base() -> PathBuf {
    std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| PathBuf::from(DEFAULT_RUNTIME_BASE))
}

/// Creates a fresh `rriter-headless-<pid>[-<n>]` directory with mode 0700. The final component
/// is created non-recursively, so an existing (possibly foreign) directory is never reused.
fn create_temp_root(runtime_base: &Path) -> Result<PathBuf, String> {
    fs::create_dir_all(runtime_base)
        .map_err(|e| format!("cannot create {}: {e}", runtime_base.display()))?;
    let pid = std::process::id();
    let mut builder = DirBuilder::new();
    builder.mode(0o700);
    for n in 0..TEMP_ROOT_ATTEMPTS {
        let name = if n == 0 {
            format!("{TEMP_ROOT_PREFIX}{pid}")
        } else {
            format!("{TEMP_ROOT_PREFIX}{pid}-{n}")
        };
        let path = runtime_base.join(name);
        match builder.create(&path) {
            Ok(()) => return Ok(path),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(format!("cannot create profile root {}: {e}", path.display())),
        }
    }
    Err(format!(
        "cannot create profile root in {}: {TEMP_ROOT_ATTEMPTS} names already taken",
        runtime_base.display()
    ))
}

/// Copies the user's config/data/state (never cache) into the layout of `app_paths_for_root`.
fn copy_user_state(user_paths: &AppPaths, root: &Path) -> io::Result<()> {
    let target = platform::app_paths_for_root(root);
    for (src, dst) in [
        (&user_paths.config, &target.config),
        (&user_paths.data, &target.data),
        (&user_paths.state, &target.state),
    ] {
        match fs::symlink_metadata(src) {
            Ok(meta) if meta.is_dir() => copy_tree(src, dst)?,
            // Missing source or a symlink/non-directory: nothing to copy.
            Ok(_) => {}
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(e),
        }
    }
    Ok(())
}

/// Recursively copies regular files and directories; symlinks and special files are skipped.
fn copy_tree(src: &Path, dst: &Path) -> io::Result<()> {
    fs::create_dir(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        // `DirEntry::file_type` does not follow symlinks (same as `symlink_metadata`).
        let file_type = entry.file_type()?;
        let target = dst.join(entry.file_name());
        if file_type.is_dir() {
            copy_tree(&entry.path(), &target)?;
        } else if file_type.is_file() {
            fs::copy(entry.path(), &target)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::ffi::OsStringExt;
    use std::os::unix::fs::{PermissionsExt, symlink};
    use std::sync::atomic::{AtomicU32, Ordering};

    static TEST_DIR_COUNTER: AtomicU32 = AtomicU32::new(0);

    struct TestDir(PathBuf);

    impl TestDir {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "rriter-headless-profile-test-{}-{}",
                std::process::id(),
                TEST_DIR_COUNTER.fetch_add(1, Ordering::Relaxed)
            ));
            let _ = fs::remove_dir_all(&path);
            fs::create_dir_all(&path).unwrap();
            TestDir(path)
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn os(args: &[&str]) -> Vec<OsString> {
        args.iter().map(OsString::from).collect()
    }

    fn parse(args: &[&str]) -> Result<HeadlessOptions, String> {
        parse_args(&os(args))
    }

    fn opts_with(profile: ProfileChoice, keep_profile: bool) -> HeadlessOptions {
        HeadlessOptions { profile, keep_profile, ..HeadlessOptions::default() }
    }

    fn pid_root(base: &Path) -> PathBuf {
        base.join(format!("rriter-headless-{}", std::process::id()))
    }

    #[test]
    fn headless_profile_parse_defaults() {
        let opts = parse(&["--headless"]).unwrap();
        assert_eq!(opts.size, (1920, 1080));
        assert_eq!(opts.scale, 1.0);
        assert_eq!(opts.profile, ProfileChoice::Temp);
        assert_eq!(opts.budget, BudgetChoice::Auto);
        assert_eq!(opts.script, None);
        assert_eq!(opts.path, None);
        assert!(!opts.keep_profile);
        assert!(!opts.allow_writes);
        assert_eq!(parse(&[]).unwrap(), opts);
    }

    #[test]
    fn headless_profile_parse_every_flag() {
        let opts = parse(&[
            "--headless",
            "--script",
            "/tmp/s.txt",
            "--size",
            "800x600",
            "--scale",
            "1.5",
            "--profile",
            "/tmp/prof",
            "--hz",
            "144",
            "--keep-profile",
            "--allow-writes",
            "/work/project",
        ])
        .unwrap();
        assert_eq!(opts.script, Some(PathBuf::from("/tmp/s.txt")));
        assert_eq!(opts.size, (800, 600));
        assert_eq!(opts.scale, 1.5);
        assert_eq!(opts.profile, ProfileChoice::Dir(PathBuf::from("/tmp/prof")));
        assert_eq!(opts.budget, BudgetChoice::Hz(144.0));
        assert!(opts.keep_profile);
        assert!(opts.allow_writes);
        assert_eq!(opts.path, Some(PathBuf::from("/work/project")));

        let opts = parse(&["main.rs", "--budget-ms", "5.5", "--profile-from-user"]).unwrap();
        assert_eq!(opts.budget, BudgetChoice::Ms(5.5));
        assert_eq!(opts.profile, ProfileChoice::FromUser);
        assert_eq!(opts.path, Some(PathBuf::from("main.rs")));
    }

    #[test]
    fn headless_profile_parse_script_stdin_and_non_utf8_paths() {
        assert_eq!(parse(&["--script", "-"]).unwrap().script, Some(PathBuf::from("-")));

        let raw = OsString::from_vec(b"/tmp/f\xff.txt".to_vec());
        let opts = parse_args(&[OsString::from("--script"), raw.clone(), raw.clone()]).unwrap();
        assert_eq!(opts.script, Some(PathBuf::from(raw.clone())));
        assert_eq!(opts.path, Some(PathBuf::from(raw)));
    }

    #[test]
    fn headless_profile_parse_rejects_invalid_arguments() {
        let cases: &[(&[&str], &str)] = &[
            (&["--size", "10x10"], "out of range"),
            (&["--size", "1920x"], "invalid size"),
            (&["--size", "x"], "invalid size"),
            (&["--size", "99999x99999"], "out of range"),
            (&["--size"], "missing value for --size"),
            (&["--size", "--scale", "2"], "missing value for --size"),
            (&["--scale", "abc"], "invalid number"),
            (&["--scale", "nan"], "non-finite"),
            (&["--scale", "inf"], "non-finite"),
            (&["--scale", "9"], "scale must be between"),
            (&["--hz", "0"], "--hz must be greater than 0"),
            (&["--hz", "-60"], "--hz must be greater than 0"),
            (&["--budget-ms", "0"], "--budget-ms must be greater than 0"),
            (&["--hz", "144", "--budget-ms", "5"], "mutually exclusive"),
            (&["--profile", "/x", "--profile-from-user"], "mutually exclusive"),
            (&["--profile", ""], "missing value for --profile"),
            (&["--script"], "missing value for --script"),
            (&["--foo"], "unknown option '--foo'"),
            (&["-x"], "unknown option '-x'"),
            (&["a.rs", "b.rs"], "unexpected extra argument 'b.rs'"),
            (&[""], "empty FILE_OR_DIR"),
        ];
        for (args, needle) in cases {
            let err = parse(args).unwrap_err();
            assert!(err.contains(needle), "{args:?}: {err:?} does not contain {needle:?}");
        }
    }

    #[test]
    fn headless_profile_parse_rejects_non_utf8_numbers() {
        for flag in ["--scale", "--size", "--hz", "--budget-ms"] {
            let args = [OsString::from(flag), OsString::from_vec(vec![b'1', 0xff])];
            let err = parse_args(&args).unwrap_err();
            assert!(err.contains("not valid UTF-8"), "{flag}: {err}");
        }
    }

    #[test]
    fn headless_profile_temp_root_is_private_unique_and_removed() {
        let base = TestDir::new();
        let user = platform::app_paths_for_root(&base.0.join("user"));
        let opts = opts_with(ProfileChoice::Temp, false);

        let first = Profile::prepare_in(&opts, &base.0, &user).unwrap();
        assert_eq!(first.root(), pid_root(&base.0));
        let mode = fs::metadata(first.root()).unwrap().permissions().mode();
        assert_eq!(mode & 0o077, 0, "profile root mode {mode:o}");

        let second = Profile::prepare_in(&opts, &base.0, &user).unwrap();
        let mut expected = pid_root(&base.0).into_os_string();
        expected.push("-1");
        assert_eq!(second.root(), Path::new(&expected));

        first.finish();
        assert!(!pid_root(&base.0).exists());
        let second_root = second.root().to_path_buf();
        second.finish();
        assert!(!second_root.exists());
    }

    #[test]
    fn headless_profile_temp_root_skips_foreign_directory() {
        let base = TestDir::new();
        let user = platform::app_paths_for_root(&base.0.join("user"));
        let foreign = pid_root(&base.0);
        fs::create_dir_all(&foreign).unwrap();
        fs::write(foreign.join("keep.txt"), "foreign").unwrap();

        let profile = Profile::prepare_in(&opts_with(ProfileChoice::Temp, false), &base.0, &user)
            .unwrap();
        assert_ne!(profile.root(), foreign);
        profile.finish();
        assert_eq!(fs::read_to_string(foreign.join("keep.txt")).unwrap(), "foreign");
    }

    #[test]
    fn headless_profile_keep_profile_and_dir_survive_finish() {
        let base = TestDir::new();
        let user = platform::app_paths_for_root(&base.0.join("user"));

        let kept = Profile::prepare_in(&opts_with(ProfileChoice::Temp, true), &base.0, &user)
            .unwrap();
        let kept_root = kept.root().to_path_buf();
        kept.finish();
        assert!(kept_root.is_dir());

        let dir = base.0.join("nested/explicit");
        let explicit =
            Profile::prepare_in(&opts_with(ProfileChoice::Dir(dir.clone()), false), &base.0, &user)
                .unwrap();
        assert_eq!(explicit.root(), dir);
        assert!(dir.is_dir());
        explicit.finish();
        assert!(dir.is_dir());
    }

    #[test]
    fn headless_profile_from_user_copies_config_data_state_only() {
        let base = TestDir::new();
        let user = platform::app_paths_for_root(&base.0.join("user"));
        fs::create_dir_all(user.config.join("sub")).unwrap();
        fs::write(user.config.join("settings.json"), "{}").unwrap();
        fs::write(user.config.join("sub/deep.txt"), "deep").unwrap();
        fs::create_dir_all(&user.data).unwrap();
        fs::write(user.data.join("data.bin"), "data").unwrap();
        fs::create_dir_all(&user.state).unwrap();
        fs::write(user.state.join("session.json"), "state").unwrap();
        fs::create_dir_all(&user.cache).unwrap();
        fs::write(user.cache.join("cache.bin"), "cache").unwrap();
        let outside = base.0.join("outside.txt");
        fs::write(&outside, "secret").unwrap();
        symlink(&outside, user.config.join("link.txt")).unwrap();
        symlink(base.0.join("user"), user.config.join("loop")).unwrap();

        let runtime = base.0.join("runtime");
        let profile =
            Profile::prepare_in(&opts_with(ProfileChoice::FromUser, false), &runtime, &user)
                .unwrap();
        let root = profile.root().to_path_buf();
        assert_eq!(root, pid_root(&runtime));
        let copied = platform::app_paths_for_root(&root);
        assert_eq!(fs::read_to_string(copied.config.join("settings.json")).unwrap(), "{}");
        assert_eq!(fs::read_to_string(copied.config.join("sub/deep.txt")).unwrap(), "deep");
        assert_eq!(fs::read_to_string(copied.data.join("data.bin")).unwrap(), "data");
        assert_eq!(fs::read_to_string(copied.state.join("session.json")).unwrap(), "state");
        assert!(!copied.cache.exists());
        assert!(fs::symlink_metadata(copied.config.join("link.txt")).is_err());
        assert!(fs::symlink_metadata(copied.config.join("loop")).is_err());

        profile.finish();
        assert!(!root.exists());
        assert_eq!(fs::read_to_string(&outside).unwrap(), "secret");
        assert!(user.config.join("settings.json").is_file());
    }

    #[test]
    fn headless_profile_from_user_skips_missing_sources() {
        let base = TestDir::new();
        let user = platform::app_paths_for_root(&base.0.join("absent"));
        let profile =
            Profile::prepare_in(&opts_with(ProfileChoice::FromUser, false), &base.0, &user)
                .unwrap();
        let copied = platform::app_paths_for_root(profile.root());
        assert!(profile.root().is_dir());
        assert!(!copied.config.exists() && !copied.data.exists() && !copied.state.exists());
        profile.finish();
    }
}
