//! Project Search panel interaction regressions.

use crate::headless::tests_support::{
    click_ui, dump, has_ui, run_script, scratch_dir, ui_center, wait_until,
    workspace_with_explorer,
};
use crate::headless::HeadlessSession;
use serde_json::Value;
use std::path::Path;

const TEST_WIDTH: u32 = 1280;
const TEST_HEIGHT: u32 = 720;
const TEST_SCALE: f32 = 4.0 / 3.0;

fn project_search_session(dir: &Path) -> HeadlessSession {
    let mut session = workspace_with_explorer(TEST_WIDTH, TEST_HEIGHT, TEST_SCALE, dir);
    click_ui(&mut session, "SidebarSlot(Search)");
    let settled = run_script(&mut session, b"settle 2000\n");
    assert!(settled.iter().any(|line| line.contains("settled=true")), "{settled:?}");
    assert!(has_ui(&dump(&mut session), "ProjectSearchQueryInput"));
    session
}

fn replace_input(session: &mut HeadlessSession, id: &str, text: &str) {
    click_ui(session, id);
    let lines = run_script(session, format!("key ctrl+a\ntype {text}\n").as_bytes());
    assert!(lines.iter().all(|line| line == "ok"), "{lines:?}");
}

fn run_project_search(session: &mut HeadlessSession) {
    let previous_generation = session.app.ide_panel.project_search.generation;
    click_ui(session, "ProjectSearchRun");
    wait_until(session, 5000, "project search results", |session| {
        let state = &session.app.ide_panel.project_search;
        state.generation != previous_generation && state.running_generation.is_none()
    });
}

fn result_paths(session: &HeadlessSession) -> Vec<String> {
    session
        .app
        .ide_panel
        .project_search
        .results
        .iter()
        .map(|file| file.relative_path.clone())
        .collect()
}

fn active_tab_matches(session: &mut HeadlessSession, path: &Path, line: u32) -> bool {
    let state = dump(session);
    state["tabs"].as_array().is_some_and(|tabs| {
        tabs.iter().any(|tab| {
            tab["active"] == true
                && tab["path"].as_str() == path.to_str()
                && tab["cursor"]["line"].as_u64() == Some((line + 1) as u64)
        })
    })
}

fn visible_match_indices(state: &Value, file_idx: usize) -> Vec<usize> {
    let prefix = format!("ProjectSearchMatchJump({file_idx}, ");
    state["ui"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|element| element["id"].as_str())
        .filter_map(|id| {
            id.strip_prefix(&prefix)
                .and_then(|suffix| suffix.strip_suffix(')'))
                .and_then(|index| index.parse().ok())
        })
        .collect()
}

#[test]
fn headless_project_search_include_and_exclude_globs_narrow_files() {
    let dir = scratch_dir("ui-project-search-globs");
    let included = dir.join("src/keep/one.rs");
    let excluded = dir.join("src/skip/two.rs");
    let wrong_extension = dir.join("docs/three.txt");
    std::fs::create_dir_all(included.parent().unwrap()).unwrap();
    std::fs::create_dir_all(excluded.parent().unwrap()).unwrap();
    std::fs::create_dir_all(wrong_extension.parent().unwrap()).unwrap();
    std::fs::write(&included, "needle included\n").unwrap();
    std::fs::write(&excluded, "needle excluded\n").unwrap();
    std::fs::write(&wrong_extension, "needle outside include glob\n").unwrap();

    let mut session = project_search_session(&dir);
    replace_input(&mut session, "ProjectSearchQueryInput", "needle");
    replace_input(&mut session, "ProjectSearchIncludeInput", "src/**/*.rs");
    replace_input(&mut session, "ProjectSearchExcludeInput", "src/skip/**");
    run_project_search(&mut session);

    let state = dump(&mut session);
    let workspace_name = dir.file_name().unwrap().to_string_lossy();
    assert_eq!(result_paths(&session), vec![format!("{workspace_name}/src/keep/one.rs")]);
    assert_eq!(session.app.ide_panel.project_search.total_matches, 1);
    assert!(has_ui(&state, "ProjectSearchFileToggle(0)"), "{state}");
    assert!(has_ui(&state, "ProjectSearchMatchJump(0, 0)"), "{state}");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_project_search_case_help_and_live_path_filter_controls() {
    let dir = scratch_dir("ui-project-search-controls");
    std::fs::write(dir.join("alpha.txt"), "Needle upper\nneedle lower\n").unwrap();
    std::fs::write(dir.join("beta.txt"), "NEEDLE uppercase\n").unwrap();
    std::fs::write(dir.join("other.txt"), "unrelated text\n").unwrap();

    let mut session = project_search_session(&dir);
    for id in [
        "ProjectSearchQueryInput",
        "ProjectSearchIncludeInput",
        "ProjectSearchExcludeInput",
        "ProjectSearchCaseToggle",
        "ProjectSearchHelp",
    ] {
        assert!(has_ui(&dump(&mut session), id), "missing {id}");
    }
    click_ui(&mut session, "ProjectSearchHelp");
    assert!(has_ui(&dump(&mut session), "ProjectSearchHelpPopup"));
    click_ui(&mut session, "ProjectSearchHelp");
    assert!(!has_ui(&dump(&mut session), "ProjectSearchHelpPopup"));

    replace_input(&mut session, "ProjectSearchQueryInput", "needle");
    run_project_search(&mut session);
    assert_eq!(session.app.ide_panel.project_search.total_matches, 3);
    let insensitive_paths = result_paths(&session);
    let workspace_name = dir.file_name().unwrap().to_string_lossy();
    let alpha_path = format!("{workspace_name}/alpha.txt");
    let beta_path = format!("{workspace_name}/beta.txt");
    assert!(insensitive_paths.contains(&alpha_path));
    assert!(insensitive_paths.contains(&beta_path));
    assert!(has_ui(&dump(&mut session), "ProjectSearchFilterInput"));

    let beta_idx = insensitive_paths.iter().position(|path| path == &beta_path).unwrap();
    let alpha_idx = insensitive_paths.iter().position(|path| path == &alpha_path).unwrap();
    replace_input(&mut session, "ProjectSearchFilterInput", "beta.txt");
    wait_until(&mut session, 5000, "live path filter", |session| {
        session.app.ide_panel.project_search.flat_rows.len() == 2
    });
    let filtered = dump(&mut session);
    assert!(has_ui(&filtered, &format!("ProjectSearchFileToggle({beta_idx})")), "{filtered}");
    assert!(!has_ui(&filtered, &format!("ProjectSearchFileToggle({alpha_idx})")), "{filtered}");

    replace_input(&mut session, "ProjectSearchFilterInput", "alpha.txt");
    wait_until(&mut session, 5000, "updated live path filter", |session| {
        session.app.ide_panel.project_search.flat_rows.len() == 3
    });
    click_ui(&mut session, "ProjectSearchCaseToggle");
    run_project_search(&mut session);

    assert!(session.app.ide_panel.project_search.case_sensitive);
    assert_eq!(result_paths(&session), vec![alpha_path]);
    assert_eq!(session.app.ide_panel.project_search.total_matches, 1);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_project_search_second_and_third_file_results_open_at_match_line() {
    let dir = scratch_dir("ui-project-search-multiple-files");
    std::fs::write(dir.join("01-alpha.txt"), "skip\nneedle alpha\n").unwrap();
    std::fs::create_dir_all(dir.join("nested")).unwrap();
    std::fs::write(dir.join("nested/02-beta.txt"), "skip\nskip\nneedle beta\n").unwrap();
    std::fs::write(dir.join("03-gamma.txt"), "skip\nskip\nskip\nneedle gamma\n").unwrap();

    let mut session = project_search_session(&dir);
    replace_input(&mut session, "ProjectSearchQueryInput", "needle");
    run_project_search(&mut session);

    let state = dump(&mut session);
    let results = session.app.ide_panel.project_search.results.clone();
    assert_eq!(results.len(), 3, "{}", state);
    let workspace_name = dir.file_name().unwrap().to_string_lossy();
    for (expected_path, expected_line) in [
        ("01-alpha.txt", 1),
        ("nested/02-beta.txt", 2),
        ("03-gamma.txt", 3),
    ] {
        let expected_path = format!("{workspace_name}/{expected_path}");
        let file = results
            .iter()
            .find(|file| file.relative_path == expected_path)
            .unwrap_or_else(|| panic!("missing {expected_path}: {state}"));
        assert_eq!(file.matches[0].start_line, expected_line, "{state}");
    }
    for (file_idx, file) in results.iter().enumerate() {
        assert_eq!(file.matches.len(), 1, "{}", state);
        assert!(has_ui(&state, &format!("ProjectSearchFileToggle({file_idx})")), "{state}");
        assert!(has_ui(&state, &format!("ProjectSearchMatchJump({file_idx}, 0)")), "{state}");
    }

    for file_idx in [1, 2] {
        let result = results[file_idx].clone();
        let expected_line = result.matches[0].start_line;
        click_ui(&mut session, &format!("ProjectSearchMatchJump({file_idx}, 0)"));
        wait_until(&mut session, 5000, "project search result navigation", |session| {
            active_tab_matches(session, &result.path, expected_line)
        });
        assert!(active_tab_matches(&mut session, &result.path, expected_line));
    }
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_project_search_result_list_scrolls_through_many_matches() {
    let dir = scratch_dir("ui-project-search-scroll");
    let mut source = String::new();
    for line in 0..50 {
        source.push_str(&format!("needle row {line:02}\n"));
    }
    std::fs::write(dir.join("many.txt"), source).unwrap();

    let mut session = project_search_session(&dir);
    replace_input(&mut session, "ProjectSearchQueryInput", "needle");
    run_project_search(&mut session);

    let initial = dump(&mut session);
    assert_eq!(session.app.ide_panel.project_search.total_matches, 50);
    assert!(has_ui(&initial, "ProjectSearchScrollbar"), "{initial}");
    assert!(has_ui(&initial, "ProjectSearchMatchJump(0, 0)"), "{initial}");
    let (x, y) = ui_center(&initial, "ProjectSearchMatchJump(0, 0)");
    let lines = run_script(&mut session, format!("mouse_move {x} {y}\nwheel 0 -5\n").as_bytes());
    assert!(lines.iter().all(|line| line == "ok"), "{lines:?}");
    wait_until(&mut session, 5000, "project search list scroll animation", |session| {
        let scroll = &session.app.ide_panel.project_search.scroll;
        scroll.current > 0.0 && scroll.is_settled()
    });

    let scrolled = dump(&mut session);
    assert!(has_ui(&scrolled, "ProjectSearchScrollbar"), "{scrolled}");
    assert!(!has_ui(&scrolled, "ProjectSearchMatchJump(0, 0)"), "{scrolled}");
    assert!(visible_match_indices(&scrolled, 0).iter().any(|index| *index > 0), "{scrolled}");
    let _ = std::fs::remove_dir_all(dir);
}
