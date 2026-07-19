use super::*;

fn git_diff_test_root(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "rriter-{name}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ))
}

#[test]
fn git_diff_build_added_deleted_modified_order() {
    let state = build_diff_view(
        "same\nold\nremove\n".to_string(),
        "same\nnew\nadd\n".to_string(),
    );
    assert_eq!(state.displayed_text, "same\nold\nremove\nnew\nadd\n");
    assert_eq!(
        state.line_kinds,
        vec![
            DiffLineKind::Context,
            DiffLineKind::ModifiedOld,
            DiffLineKind::ModifiedOld,
            DiffLineKind::ModifiedNew,
            DiffLineKind::ModifiedNew,
        ]
    );
}

#[test]
fn git_diff_rollback_added_deletes_new_lines() {
    let state = build_diff_view("a\n".to_string(), "a\nb\n".to_string());
    let hunk = state.hunks.first().unwrap();
    assert_eq!(rollback_hunk_text(&state.worktree_text, hunk), "a\n");
}

#[test]
fn git_diff_rollback_deleted_restores_old_lines() {
    let state = build_diff_view("a\nb\n".to_string(), "a\n".to_string());
    let hunk = state.hunks.first().unwrap();
    assert_eq!(rollback_hunk_text(&state.worktree_text, hunk), "a\nb\n");
}

#[test]
fn git_diff_rollback_modified_replaces_new_with_old() {
    let state = build_diff_view("a\nold\n".to_string(), "a\nnew\n".to_string());
    let hunk = state.hunks.first().unwrap();
    assert_eq!(rollback_hunk_text(&state.worktree_text, hunk), "a\nold\n");
}

#[test]
fn inline_diff_hunk_match_prefers_line_ranges() {
    let state = build_diff_view(
        "a\nold\nb\nbefore\n".to_string(),
        "a\nnew\nb\nafter\n".to_string(),
    );
    let target = LineDiffHunk {
        before_start: 3,
        before_end: 4,
        after_start: 3,
        after_end: 4,
    };
    assert_eq!(
        App::inline_diff_hunk_index_for_target(&state, target, 0),
        Some(1)
    );
}

#[test]
fn git_diff_loader_normalizes_head_and_preserves_worktree_text_format() {
    let root = git_diff_test_root("git-diff-format");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("notes.txt");
    let head_format = crate::platform::TextFileFormat {
        encoding: crate::platform::TextEncoding::Utf8Bom,
        line_ending: crate::platform::LineEnding::CrLf,
    };
    std::fs::write(
        &path,
        crate::platform::encode_text("base\n😀line\n", head_format),
    )
    .unwrap();

    {
        let repo = git2::Repository::init(&root).unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(Path::new("notes.txt")).unwrap();
        let tree_id = index.write_tree().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();
        let signature = git2::Signature::now("RRiter Test", "rriter@example.invalid").unwrap();
        repo.commit(
            Some("HEAD"),
            &signature,
            &signature,
            "initial",
            &tree,
            &[],
        )
        .unwrap();
    }

    let worktree_format = crate::platform::TextFileFormat {
        encoding: crate::platform::TextEncoding::Utf16Le,
        line_ending: crate::platform::LineEnding::CrLf,
    };
    std::fs::write(
        &path,
        crate::platform::encode_text("changed\n😀line\n", worktree_format),
    )
    .unwrap();

    let payload = load_git_diff(
        root.clone(),
        "notes.txt".to_string(),
        None,
        GitFileStatus::Modified,
    )
    .unwrap();
    assert_eq!(payload.base_text, "base\n😀line\n");
    assert_eq!(payload.worktree_text, "changed\n😀line\n");
    assert_eq!(payload.worktree_format, worktree_format);

    std::fs::remove_dir_all(root).ok();
}

#[test]
fn git_diff_extract_reconstructs_worktree_for_mixed_changes() {
    let worktree = "same\nnew\nkept\nadded-without-newline";
    let state = build_diff_view(
        "same\nold\nremoved\nkept\n".to_string(),
        worktree.to_string(),
    );

    assert_eq!(
        extract_worktree_text(&state.displayed_text, &state.line_kinds),
        worktree
    );
}

#[test]
fn git_diff_rollback_preserves_missing_final_newline() {
    let state = build_diff_view("before".to_string(), "after".to_string());
    let hunk = state.hunks.first().unwrap();

    assert_eq!(rollback_hunk_text(&state.worktree_text, hunk), "before");
}

#[test]
fn git_diff_untracked_loader_preserves_utf16be_crlf_format() {
    let root = git_diff_test_root("git-diff-untracked-format");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    git2::Repository::init(&root).unwrap();
    let format = crate::platform::TextFileFormat {
        encoding: crate::platform::TextEncoding::Utf16Be,
        line_ending: crate::platform::LineEnding::CrLf,
    };
    std::fs::write(
        root.join("new.txt"),
        crate::platform::encode_text("first\nsecond\n", format),
    )
    .unwrap();

    let payload = load_git_diff(
        root.clone(),
        "new.txt".to_string(),
        None,
        GitFileStatus::Untracked,
    )
    .unwrap();

    assert!(payload.base_text.is_empty());
    assert_eq!(payload.worktree_text, "first\nsecond\n");
    assert_eq!(payload.worktree_format, format);
    std::fs::remove_dir_all(root).ok();
}

#[test]
fn git_diff_loader_rejects_invalid_text_without_lossy_rewrite() {
    let root = git_diff_test_root("git-diff-invalid-text");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    git2::Repository::init(&root).unwrap();
    std::fs::write(root.join("broken.txt"), [0xff, 0xfe, 0x41]).unwrap();

    let error = load_git_diff(
        root.clone(),
        "broken.txt".to_string(),
        None,
        GitFileStatus::Untracked,
    )
    .unwrap_err();

    assert!(error.contains("odd UTF-16 byte length"));
    std::fs::remove_dir_all(root).ok();
}

#[test]
fn git_diff_staged_loader_reads_index_instead_of_worktree() {
    let root = git_diff_test_root("git-diff-index-format");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let repo = git2::Repository::init(&root).unwrap();
    repo.config().unwrap().set_bool("core.autocrlf", false).unwrap();
    let format = crate::platform::TextFileFormat {
        encoding: crate::platform::TextEncoding::Utf8Bom,
        line_ending: crate::platform::LineEnding::CrLf,
    };
    let path = root.join("staged.txt");
    std::fs::write(&path, crate::platform::encode_text("staged\n", format)).unwrap();
    let mut index = repo.index().unwrap();
    index.add_path(Path::new("staged.txt")).unwrap();
    index.write().unwrap();
    std::fs::write(&path, "worktree\n").unwrap();

    let payload = load_git_diff_with_side(
        root.clone(),
        "staged.txt".to_string(),
        None,
        GitFileStatus::Added,
        true,
    )
    .unwrap();

    assert_eq!(payload.worktree_text, "staged\n");
    assert_eq!(payload.worktree_format, format);
    std::fs::remove_dir_all(root).ok();
}

#[test]
fn pending_inline_git_popup_does_not_force_a_busy_redraw_loop() {
    let source = include_str!("git_diff.rs");
    let empty_branch = source
        .split("Err(mpsc::TryRecvError::Empty) => {")
        .nth(1)
        .expect("inline Git popup empty branch");
    let branch = empty_branch
        .split("Err(mpsc::TryRecvError::Disconnected)")
        .next()
        .expect("empty branch body");
    assert!(branch.contains("self.inline_git_diff_rx = Some(rx);"));
    assert!(branch.contains("false"));
    assert!(!branch.contains("
                true
"));
}
