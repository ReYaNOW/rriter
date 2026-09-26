use crate::renderer::Theme;
use crate::{load_config, load_panel_state};
use std::path::PathBuf;
use std::time::Instant;
#[cfg(target_os = "linux")]
use std::env;

pub(super) fn parse_kde_color(content: &str, target_group: &str, target_key: &str) -> Option<[f32; 4]> {
    let mut current_group = String::new();

    for line in content.lines() {
        let line = line.trim();
        if line.starts_with('[') && line.ends_with(']') {
            current_group = line[1..line.len() - 1].to_string();
        } else if current_group == target_group && line.starts_with(&format!("{}=", target_key)) {
            let parts: Vec<&str> = line[target_key.len() + 1..].split(',').collect();
            if parts.len() == 3 {
                let [r, g, b] = [parts[0], parts[1], parts[2]].map(|part| {
                    part.trim()
                        .parse::<f32>()
                        .ok()
                        .filter(|value| value.is_finite() && (0.0..=255.0).contains(value))
                });
                let (Some(r), Some(g), Some(b)) = (r, g, b) else {
                    return None;
                };
                return Some([r / 255.0, g / 255.0, b / 255.0, 1.0]);
            }
        }
    }
    None
}

#[cfg_attr(coverage_nightly, coverage(off))]
pub(super) fn run_project_search_probe(args: &[String], idx: usize) {
    let Some(query) = args.get(idx + 1).cloned() else {
        eprintln!("usage: rriter --probe-project-search <query> [iterations]");
        return;
    };
    let iterations = args
        .get(idx + 2)
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(3)
        .max(1);
    let config = load_config();
    let panel = load_panel_state();
    let include = panel.project_search.include_editor.get_full_text();
    let exclude = panel.project_search.exclude_editor.get_full_text();
    for run_idx in 0..iterations {
        let started = Instant::now();
        let rx = crate::app::project_search::start_project_search_worker(
            crate::app::project_search::ProjectSearchRequest {
                generation: run_idx as u64 + 1,
                query: query.clone(),
                include: include.clone(),
                exclude: exclude.clone(),
                case_sensitive: false,
                workspaces: config.ide_workspaces.clone(),
                ignore_patterns: config.ide_ignore_patterns.clone(),
            },
        );
        let mut result_files = 0usize;
        let mut matches = 0usize;
        let mut worker_ms = 0u128;
        let mut capped = false;
        let mut error = None;
        while let Ok(message) = rx.recv() {
            match message {
                crate::app::project_search::ProjectSearchWorkerMessage::File { file, .. } => {
                    result_files += 1;
                    matches = matches.saturating_add(file.matches.len());
                }
                crate::app::project_search::ProjectSearchWorkerMessage::Done {
                    elapsed_ms,
                    capped: done_capped,
                    error: done_error,
                    ..
                } => {
                    worker_ms = elapsed_ms;
                    capped = done_capped;
                    error = done_error;
                    break;
                }
            }
        }
        println!(
            "[PROJECT SEARCH PROBE] run={} wall={}ms worker={}ms result_files={} matches={} capped={} error={}",
            run_idx + 1,
            started.elapsed().as_millis(),
            worker_ms,
            result_files,
            matches,
            capped,
            error.unwrap_or_default()
        );
    }
}

#[cfg(target_os = "linux")]
fn get_kde_color(target_group: &str, target_key: &str) -> Option<[f32; 4]> {
    let path = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| PathBuf::from(env::var_os("HOME").unwrap_or_default()).join(".config"))
        .join("kdeglobals");
    let content = std::fs::read_to_string(path).ok()?;
    parse_kde_color(&content, target_group, target_key)
}

#[cfg(not(target_os = "linux"))]
fn get_kde_color(_target_group: &str, _target_key: &str) -> Option<[f32; 4]> {
    None
}

pub(super) fn rayon_thread_cap(available: usize) -> usize {
    available.clamp(1, 4)
}

pub(super) fn init_rayon_global_pool() {
    let threads = std::thread::available_parallelism()
        .map(|threads| rayon_thread_cap(threads.get()))
        .unwrap_or(1);
    let _ = rayon::ThreadPoolBuilder::new()
        .num_threads(threads)
        .build_global();
}

pub(super) fn selection_color(desktop_color: Option<[f32; 4]>, system_color: Option<[f32; 4]>) -> [f32; 4] {
    desktop_color
        .or(system_color)
        .unwrap_or(crate::platform::DEFAULT_ACCENT_COLOR)
}

pub(super) fn load_dracula() -> Theme {
    let sel_color = selection_color(
        get_kde_color("Colors:Selection", "BackgroundNormal"),
        crate::platform::system_accent_color(),
    );

    Theme {
        bg: [0.156, 0.164, 0.211, 1.0],
        fg: [0.972, 0.972, 0.949, 1.0],
        sel: sel_color,
        minimap_bg: [0.129, 0.133, 0.172, 1.0],
        line_num: [0.384, 0.447, 0.643, 1.0],
        minimap_cursor: sel_color,
        modified_unsaved: [1.0, 0.474, 0.776, 1.0],
        modified_saved: [0.313, 0.980, 0.482, 1.0],
        diag_warn: [0.945, 0.980, 0.549, 1.0],
        diag_error: [1.0, 0.333, 0.333, 1.0],
        unused: [0.48, 0.48, 0.48, 0.6],
    }
}

#[cfg(target_os = "linux")]
const EGL_VENDOR_ENV: &str = "__EGL_VENDOR_LIBRARY_FILENAMES";
#[cfg(target_os = "linux")]
const RRITER_EGL_VENDOR_ENV: &str = "RRITER_EGL_VENDOR";
#[cfg(target_os = "linux")]
const NVIDIA_EGL_VENDOR: &str = "/usr/share/glvnd/egl_vendor.d/10_nvidia.json";
#[cfg(target_os = "linux")]
const MESA_EGL_VENDOR: &str = "/usr/share/glvnd/egl_vendor.d/50_mesa.json";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum EglVendorPreference {
    Auto,
    System,
    Nvidia,
    Mesa,
}

pub(super) fn parse_egl_vendor_preference(raw: Option<&str>) -> EglVendorPreference {
    let Some(value) = raw.map(str::trim) else {
        return EglVendorPreference::Auto;
    };
    if value.eq_ignore_ascii_case("auto") {
        EglVendorPreference::Auto
    } else if value.eq_ignore_ascii_case("system") {
        EglVendorPreference::System
    } else if value.eq_ignore_ascii_case("nvidia") {
        EglVendorPreference::Nvidia
    } else if value.eq_ignore_ascii_case("mesa") {
        EglVendorPreference::Mesa
    } else {
        EglVendorPreference::System
    }
}

#[cfg(target_os = "linux")]
pub(super) fn prefer_egl_vendor() {
    if env::var_os(EGL_VENDOR_ENV).is_some() {
        return;
    }

    let preference = env::var_os(RRITER_EGL_VENDOR_ENV)
        .and_then(|value| value.into_string().ok())
        .map(|value| parse_egl_vendor_preference(Some(&value)))
        .unwrap_or(EglVendorPreference::Auto);
    let vendor_path = match preference {
        EglVendorPreference::System => return,
        EglVendorPreference::Nvidia => NVIDIA_EGL_VENDOR,
        EglVendorPreference::Mesa => MESA_EGL_VENDOR,
        EglVendorPreference::Auto => {
            if nvidia_gpu_present() {
                NVIDIA_EGL_VENDOR
            } else {
                return;
            }
        }
    };

    if !std::path::Path::new(vendor_path).exists() {
        return;
    }

    // Must run before EGL/GLVND loads; main is still single-threaded here.
    unsafe {
        env::set_var(EGL_VENDOR_ENV, vendor_path);
    }
}

#[cfg(not(target_os = "linux"))]
pub(super) fn prefer_egl_vendor() {}

#[cfg(target_os = "linux")]
fn nvidia_gpu_present() -> bool {
    if std::path::Path::new("/dev/nvidiactl").exists()
        || std::path::Path::new("/proc/driver/nvidia/version").exists()
    {
        return true;
    }

    let Ok(entries) = std::fs::read_dir("/sys/class/drm") else {
        return false;
    };
    for entry in entries.flatten() {
        let vendor_path = entry.path().join("device/vendor");
        let Ok(vendor) = std::fs::read_to_string(vendor_path) else {
            continue;
        };
        if vendor.trim().eq_ignore_ascii_case("0x10de") {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selection_color_prefers_desktop_then_system_and_uses_requested_purple_fallback() {
        let desktop = [0.1, 0.2, 0.3, 1.0];
        let system = [0.4, 0.5, 0.6, 1.0];
        assert_eq!(selection_color(Some(desktop), Some(system)), desktop);
        assert_eq!(selection_color(None, Some(system)), system);
        assert_eq!(
            selection_color(None, None),
            [114.0 / 255.0, 89.0 / 255.0, 175.0 / 255.0, 1.0]
        );
    }

    #[test]
    fn kde_color_parser_respects_group_boundaries_and_rgb_shape() {
        assert_eq!(
            parse_kde_color(
                "[Colors:Selection]\nOther=9,9,9\n[Colors:Window]\nBackgroundNormal=5,10,15\n",
                "Colors:Window",
                "BackgroundNormal",
            ),
            Some([5.0 / 255.0, 10.0 / 255.0, 15.0 / 255.0, 1.0])
        );
        assert_eq!(
            parse_kde_color(
                "[Colors:Selection]\nBackgroundNormal=1,2,3,4\n",
                "Colors:Selection",
                "BackgroundNormal",
            ),
            None
        );
        assert_eq!(
            parse_kde_color(
                "[Colors:Selection]\nBackgroundNormal=a,b,c\n",
                "Colors:Selection",
                "BackgroundNormal",
            ),
            None
        );
        assert_eq!(
            parse_kde_color(
                "[Colors:Selection]\nBackgroundNormal=1,2,3\n[Other]\nBackgroundNormal=4,5,6\n",
                "Other",
                "Missing",
            ),
            None
        );
    }

    #[test]
    fn rayon_thread_cap_stays_small_and_nonzero() {
        assert_eq!(rayon_thread_cap(0), 1);
        assert_eq!(rayon_thread_cap(1), 1);
        assert_eq!(rayon_thread_cap(4), 4);
        assert_eq!(rayon_thread_cap(128), 4);
    }

    #[test]
    fn egl_vendor_preference_parser_is_portable_and_conservative() {
        assert_eq!(parse_egl_vendor_preference(None), EglVendorPreference::Auto);
        assert_eq!(
            parse_egl_vendor_preference(Some("nvidia")),
            EglVendorPreference::Nvidia
        );
        assert_eq!(
            parse_egl_vendor_preference(Some(" mesa ")),
            EglVendorPreference::Mesa
        );
        assert_eq!(
            parse_egl_vendor_preference(Some("SYSTEM")),
            EglVendorPreference::System
        );
        assert_eq!(
            parse_egl_vendor_preference(Some("bad")),
            EglVendorPreference::System
        );
    }
}
