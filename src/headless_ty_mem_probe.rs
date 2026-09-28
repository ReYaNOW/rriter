use crate::editor::Editor;
use std::path::PathBuf;
use std::time::{Duration, Instant};

#[cfg_attr(coverage_nightly, coverage(off))]
fn default_headless_ty_mem_files() -> Vec<String> {
    vec![
        "tests/perf_diagnostics_stress_12000.py".to_string(),
        "tests/perf_identical_words_20000.py".to_string(),
        "tests/perf_large_realistic_15000.py".to_string(),
    ]
}

#[cfg_attr(coverage_nightly, coverage(off))]
fn headless_smaps_field_kb(pid: u32, field: &str) -> Option<u64> {
    let content = std::fs::read_to_string(format!("/proc/{pid}/smaps_rollup")).ok()?;
    for line in content.lines() {
        if let Some(rest) = line.strip_prefix(field) {
            return rest
                .split_whitespace()
                .next()
                .and_then(|value| value.parse::<u64>().ok());
        }
    }
    None
}

#[cfg_attr(coverage_nightly, coverage(off))]
fn headless_pid_ppid(pid: u32) -> Option<u32> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let end = stat.rfind(')')?;
    let rest = stat.get(end + 2..)?;
    let mut fields = rest.split_whitespace();
    let _state = fields.next()?;
    fields.next()?.parse::<u32>().ok()
}

#[cfg_attr(coverage_nightly, coverage(off))]
fn headless_process_name(pid: u32) -> Option<String> {
    std::fs::read_to_string(format!("/proc/{pid}/comm"))
        .ok()
        .map(|value| value.trim().to_string())
}

#[cfg_attr(coverage_nightly, coverage(off))]
fn headless_lsp_child_rss_kb(parent_pid: u32) -> (u64, u64, u64) {
    let mut total = 0;
    let mut ty = 0;
    let mut ruff = 0;
    let Ok(entries) = std::fs::read_dir("/proc") else {
        return (0, 0, 0);
    };

    for entry in entries.flatten() {
        let Some(pid) = entry.file_name().to_string_lossy().parse::<u32>().ok() else {
            continue;
        };
        if headless_pid_ppid(pid) != Some(parent_pid) {
            continue;
        }
        let rss = headless_smaps_field_kb(pid, "Rss:").unwrap_or(0);
        total += rss;
        if let Some(name) = headless_process_name(pid) {
            if name.starts_with("ty") {
                ty += rss;
            } else if name.starts_with("ruff") {
                ruff += rss;
            }
        }
    }

    (total, ty, ruff)
}

#[cfg_attr(coverage_nightly, coverage(off))]
fn print_headless_ty_mem_sample(
    label: &str,
    started_at: Instant,
    manager: Option<&crate::lsp::LspManager>,
    opened_files: usize,
    opened_bytes: usize,
) {
    let pid = std::process::id();
    let parent_rss = headless_smaps_field_kb(pid, "Rss:").unwrap_or(0);
    let parent_private_dirty = headless_smaps_field_kb(pid, "Private_Dirty:").unwrap_or(0);
    let (child_rss, ty_rss, ruff_rss) = headless_lsp_child_rss_kb(pid);
    let (
        diag_paths,
        diag_count,
        ty_diag_paths,
        ty_diag_count,
        ruff_diag_paths,
        ruff_diag_count,
        logs,
        log_bytes,
    ) = if let Some(manager) = manager {
        let paths = manager.diagnostic_paths();
        let diag_count = paths
            .iter()
            .map(|path| manager.diagnostic_count(path.as_path()))
            .sum();
        let ty_diag_count = manager
            .ty_instant_diagnostics
            .values()
            .map(|(_, items)| items.len())
            .sum();
        let (ruff_diag_paths, ruff_diag_count) = manager.ruff_diagnostic_storage_counts();
        let mut logs = 0usize;
        let mut log_bytes = 0usize;
        for entries in manager.server_logs.values() {
            logs += entries.len();
            log_bytes += entries.iter().map(|entry| entry.text.len()).sum::<usize>();
        }
        (
            paths.len(),
            diag_count,
            manager.ty_instant_diagnostics.len(),
            ty_diag_count,
            ruff_diag_paths,
            ruff_diag_count,
            logs,
            log_bytes,
        )
    } else {
        (0, 0, 0, 0, 0, 0, 0, 0)
    };

    println!(
        "HEADLESS_TY_MEM stage={label} elapsed_ms={} parent_rss_kb={parent_rss} parent_private_dirty_kb={parent_private_dirty} child_rss_kb={child_rss} ty_rss_kb={ty_rss} ruff_rss_kb={ruff_rss} opened_files={opened_files} opened_bytes={opened_bytes} diag_paths={diag_paths} diag_count={diag_count} ty_diag_paths={ty_diag_paths} ty_diag_count={ty_diag_count} ruff_diag_paths={ruff_diag_paths} ruff_diag_count={ruff_diag_count} logs={logs} log_bytes={log_bytes}",
        started_at.elapsed().as_millis(),
    );
}

#[cfg_attr(coverage_nightly, coverage(off))]
pub(crate) fn run_headless_ty_mem_probe(args: &[String], flag_idx: usize) {
    let raw_files: Vec<String> = args
        .iter()
        .skip(flag_idx + 1)
        .filter(|arg| !arg.starts_with("--"))
        .cloned()
        .collect();
    let raw_files = if raw_files.is_empty() {
        default_headless_ty_mem_files()
    } else {
        raw_files
    };
    let workspace = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let workspace = std::fs::canonicalize(&workspace).unwrap_or(workspace);
    let started_at = Instant::now();
    // The probe drives `poll` itself; no event loop listens for wakes.
    let mut manager = crate::lsp::LspManager::with_ui_waker(
        vec![workspace.clone()],
        crate::ui_waker::UiWaker::counting(),
    );
    let mut editors = Vec::with_capacity(raw_files.len());
    let mut opened_bytes = 0usize;

    print_headless_ty_mem_sample("start", started_at, Some(&manager), 0, 0);

    for (idx, raw) in raw_files.iter().enumerate() {
        let input_path = PathBuf::from(raw);
        let path = if input_path.is_absolute() {
            input_path
        } else {
            workspace.join(input_path)
        };
        let path = std::fs::canonicalize(&path).unwrap_or(path);
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(err) => {
                eprintln!(
                    "HEADLESS_TY_MEM read_error path={} err={err}",
                    path.display()
                );
                continue;
            }
        };
        opened_bytes += text.len();

        let mut editor = Editor::new(text.len().saturating_add(8192));
        editor.set_clean_text(&text);
        editors.push(editor);

        let ext = path.extension().and_then(|ext| ext.to_str()).unwrap_or("");
        manager.notify_open(&path, ext, &text, idx as i32 + 1);
        let label = format!("after_open_{}", idx + 1);
        print_headless_ty_mem_sample(
            &label,
            started_at,
            Some(&manager),
            editors.len(),
            opened_bytes,
        );
    }

    let mut workspace_done = false;
    let wait_started = Instant::now();
    while wait_started.elapsed() < Duration::from_secs(45) {
        let events = manager.poll();
        if events
            .iter()
            .any(|event| matches!(event, crate::lsp::LspEvent::WorkspaceDiagnosticsDone { .. }))
        {
            workspace_done = true;
            print_headless_ty_mem_sample(
                "workspace_diagnostics_done",
                started_at,
                Some(&manager),
                editors.len(),
                opened_bytes,
            );
            break;
        }
        if !events.is_empty() {
            print_headless_ty_mem_sample(
                "poll_events",
                started_at,
                Some(&manager),
                editors.len(),
                opened_bytes,
            );
        }
        std::thread::sleep(Duration::from_millis(25));
    }

    for _ in 0..10 {
        let _ = manager.poll();
        std::thread::sleep(Duration::from_millis(25));
    }
    print_headless_ty_mem_sample(
        if workspace_done {
            "final_done"
        } else {
            "final_timeout"
        },
        started_at,
        Some(&manager),
        editors.len(),
        opened_bytes,
    );

    manager.shutdown();
    std::thread::sleep(Duration::from_millis(250));
    print_headless_ty_mem_sample(
        "after_shutdown",
        started_at,
        None,
        editors.len(),
        opened_bytes,
    );
    std::hint::black_box(editors);
}
