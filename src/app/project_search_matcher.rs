struct SearchIgnoreMatcher {
    patterns: Vec<String>,
}

impl SearchIgnoreMatcher {
    fn new(mut patterns: Vec<String>) -> Self {
        for pattern in crate::app::file_tree::DEFAULT_IGNORE_PATTERNS {
            if !patterns.iter().any(|existing| existing == pattern) {
                patterns.push((*pattern).to_string());
            }
        }
        if !patterns.iter().any(|existing| existing == ".git") {
            patterns.push(".git".to_string());
        }
        Self { patterns }
    }

    fn matches_path(&self, path: &Path, workspaces: &[PathBuf]) -> bool {
        if self.patterns.is_empty() {
            return false;
        }
        let Some(rel) = workspaces
            .iter()
            .find_map(|workspace| platform::relative_to(path, workspace))
        else {
            return false;
        };
        rel.components().any(|component| {
            let std::path::Component::Normal(name) = component else {
                return false;
            };
            name.to_str().is_some_and(|name| {
                crate::app::file_tree::matches_ignore_pattern_strings(name, &self.patterns)
            })
        })
    }
}

#[derive(Default)]
struct ProjectSearchLineCursor {
    scan: usize,
    line: u32,
    line_start: usize,
}

impl ProjectSearchLineCursor {
    fn lsp_pos(&mut self, text: &str, offset: usize) -> (u32, u32, usize) {
        let offset = offset.min(text.len());
        if offset < self.scan {
            *self = Self::default();
        }
        let bytes = text.as_bytes();
        while self.scan < offset {
            if bytes.get(self.scan) == Some(&b'\n') {
                self.line = self.line.saturating_add(1);
                self.line_start = self.scan + 1;
            }
            self.scan += 1;
        }
        (
            self.line,
            utf16_units_between(text, self.line_start, offset),
            self.line_start,
        )
    }
}

fn push_match(
    text: &str,
    start: usize,
    end: usize,
    cursor: &mut ProjectSearchLineCursor,
    matches: &mut Vec<ProjectSearchMatch>,
) {
    let start = floor_char_boundary(text, start.min(text.len()));
    let end = ceil_char_boundary(text, end.min(text.len()));
    let (start_line, start_col, line_start) = cursor.lsp_pos(text, start);
    let (end_line, end_col, _) = cursor.lsp_pos(text, end);
    let extra_lines = end_line.saturating_sub(start_line) as usize;
    matches.push(ProjectSearchMatch {
        byte_start: start,
        byte_end: end,
        line_byte_start: line_start,
        start_line,
        start_col,
        end_line,
        end_col,
        preview: String::new(),
        preview_match_start: 0,
        preview_match_end: 0,
        preview_ready: false,
        extra_lines,
    });
}

fn utf16_units_between(text: &str, start: usize, end: usize) -> u32 {
    let Some(slice) = text.get(start.min(text.len())..end.min(text.len())) else {
        return 0;
    };
    if slice.is_ascii() {
        slice.len() as u32
    } else {
        slice.chars().map(|ch| ch.len_utf16() as u32).sum()
    }
}

fn floor_char_boundary(text: &str, mut idx: usize) -> usize {
    while idx > 0 && !text.is_char_boundary(idx) {
        idx -= 1;
    }
    idx
}

fn ceil_char_boundary(text: &str, mut idx: usize) -> usize {
    while idx < text.len() && !text.is_char_boundary(idx) {
        idx += 1;
    }
    idx
}

fn preview_line_with_match(line: &str, start: usize, end: usize) -> (String, usize, usize) {
    let start = floor_char_boundary(line, start.min(line.len()));
    let end = ceil_char_boundary(line, end.min(line.len())).max(start);
    let (segment_start, segment_end) = if line.len() <= PROJECT_SEARCH_PREVIEW_CHARS {
        (0, line.len())
    } else {
        let available = PROJECT_SEARCH_PREVIEW_CHARS.saturating_sub(6).max(32);
        let mut first = start.saturating_sub(PROJECT_SEARCH_PREVIEW_CONTEXT_CHARS);
        let mut last = (first + available).min(line.len());
        if end > last {
            last = end.min(line.len());
            first = last.saturating_sub(available);
        }
        (
            floor_char_boundary(line, first),
            ceil_char_boundary(line, last),
        )
    };
    let mut preview = String::with_capacity((segment_end - segment_start).min(line.len()) + 6);
    if segment_start > 0 {
        preview.push_str("...");
    }
    let prefix_len = preview.len();
    if let Some(segment) = line.get(segment_start..segment_end) {
        if segment.as_bytes().contains(&b'\t') {
            for ch in segment.chars() {
                preview.push(if ch == '\t' { ' ' } else { ch });
            }
        } else {
            preview.push_str(segment);
        }
    }
    let match_start = prefix_len + start.max(segment_start).min(segment_end) - segment_start;
    let match_end = prefix_len + end.max(segment_start).min(segment_end) - segment_start;
    if segment_end < line.len() {
        preview.push_str("...");
    }
    (preview, match_start, match_end.max(match_start))
}

struct SearchPatternPlan {
    workspaces: Vec<PathBuf>,
    include_roots: Vec<PathBuf>,
    include_globs: Option<GlobSet>,
    exclude_roots: Vec<PathBuf>,
    exclude_globs: Option<GlobSet>,
    include_has_glob: bool,
    include_all: bool,
}

impl SearchPatternPlan {
    fn new(workspaces: &[PathBuf], include: &str, exclude: &str) -> Result<Self, String> {
        let mut plan = Self {
            workspaces: normalized_workspaces(workspaces),
            include_roots: Vec::new(),
            include_globs: None,
            exclude_roots: Vec::new(),
            exclude_globs: None,
            include_has_glob: false,
            include_all: include.trim().is_empty(),
        };
        let include_tokens = split_pattern_tokens(include);
        let exclude_tokens = split_pattern_tokens(exclude);
        let mut include_builder = GlobSetBuilder::new();
        let mut include_glob_count = 0usize;
        for token in include_tokens {
            if token_has_glob(token) {
                plan.include_has_glob = true;
                for pattern in glob_patterns_for_token(token, &plan.workspaces) {
                    include_builder.add(Glob::new(&pattern).map_err(|err| err.to_string())?);
                    include_glob_count += 1;
                }
            } else {
                for path in expand_path_token(token, &plan.workspaces) {
                    push_unique_path(&mut plan.include_roots, path);
                }
            }
        }
        if include_glob_count > 0 {
            plan.include_globs = Some(include_builder.build().map_err(|err| err.to_string())?);
        }
        if plan.include_roots.is_empty() && !plan.include_has_glob {
            plan.include_all = true;
        }

        let mut exclude_builder = GlobSetBuilder::new();
        let mut exclude_glob_count = 0usize;
        for token in exclude_tokens {
            if token_has_glob(token) {
                for pattern in glob_patterns_for_token(token, &plan.workspaces) {
                    exclude_builder.add(Glob::new(&pattern).map_err(|err| err.to_string())?);
                    exclude_glob_count += 1;
                }
            } else {
                for path in expand_path_token(token, &plan.workspaces) {
                    push_unique_path(&mut plan.exclude_roots, path);
                }
            }
        }
        if exclude_glob_count > 0 {
            plan.exclude_globs = Some(exclude_builder.build().map_err(|err| err.to_string())?);
        }
        Ok(plan)
    }

    fn walk_roots(&self) -> Vec<&Path> {
        if self.include_all || self.include_has_glob {
            self.workspaces.iter().map(PathBuf::as_path).collect()
        } else {
            self.include_roots.iter().map(PathBuf::as_path).collect()
        }
    }

    fn is_file_allowed(&self, path: &Path) -> bool {
        let Some((workspace, rel)) = self.workspace_relative(path) else {
            return false;
        };
        if !self.include_all && self.include_has_glob {
            let prefix_match = self
                .include_roots
                .iter()
                .any(|root| platform::path_is_within(path, root));
            let glob_match = self
                .include_globs
                .as_ref()
                .is_some_and(|set| set.is_match(to_slash(&rel)));
            if !prefix_match && !glob_match {
                return false;
            }
        } else if !self.include_all
            && !self
                .include_roots
                .iter()
                .any(|root| platform::path_is_within(path, root))
        {
            return false;
        }
        if !platform::path_is_within(path, workspace) {
            return false;
        }
        if self
            .exclude_roots
            .iter()
            .any(|root| platform::path_is_within(path, root))
        {
            return false;
        }
        if self
            .exclude_globs
            .as_ref()
            .is_some_and(|set| set.is_match(to_slash(&rel)))
        {
            return false;
        }
        true
    }

    fn workspace_relative<'a>(&'a self, path: &Path) -> Option<(&'a Path, PathBuf)> {
        self.workspaces.iter().find_map(|workspace| {
            platform::relative_to(path, workspace).map(|relative| (workspace.as_path(), relative))
        })
    }

    fn relative_display(&self, path: &Path) -> String {
        if let Some((workspace, rel)) = self.workspace_relative(path) {
            let workspace_name = workspace
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("workspace");
            let rel = to_slash(&rel);
            if rel.is_empty() {
                workspace_name.to_string()
            } else {
                format!("{}/{}", workspace_name, rel)
            }
        } else {
            path.to_string_lossy().replace('\\', "/")
        }
    }
}

fn normalized_workspaces(workspaces: &[PathBuf]) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for workspace in workspaces {
        let path = platform::canonicalize_or_absolutize(workspace);
        if path.is_dir() {
            push_unique_path(&mut out, path);
        }
    }
    out
}

fn split_pattern_tokens(text: &str) -> Vec<&str> {
    text.split(',')
        .map(str::trim)
        .filter(|token| !token.is_empty())
        .collect()
}

fn token_has_glob(token: &str) -> bool {
    token
        .bytes()
        .any(|b| matches!(b, b'*' | b'?' | b'[' | b']' | b'{' | b'}'))
}

fn expand_path_token(token: &str, workspaces: &[PathBuf]) -> Vec<PathBuf> {
    let token = token.trim();
    if token.is_empty() {
        return Vec::new();
    }
    let raw = Path::new(token);
    if platform::is_absolute(raw) {
        let path = platform::canonicalize_or_absolutize(raw);
        if workspaces
            .iter()
            .any(|workspace| platform::path_is_within(&path, workspace))
        {
            return vec![path];
        }
        return Vec::new();
    }
    let rel = token.strip_prefix("./").unwrap_or(token);
    let rel = if rel == "." { "" } else { rel };
    workspaces
        .iter()
        .map(|workspace| platform::canonicalize_or_absolutize(&workspace.join(rel)))
        .collect()
}

fn glob_patterns_for_token(token: &str, workspaces: &[PathBuf]) -> Vec<String> {
    let token = token.trim();
    let raw = Path::new(token);
    if platform::is_absolute(raw) {
        let mut out = Vec::new();
        for workspace in workspaces {
            if let Some(rel) = platform::relative_to(raw, workspace) {
                let pattern = to_slash(&rel);
                if !pattern.is_empty() {
                    out.push(pattern);
                }
            }
        }
        out
    } else {
        vec![to_slash(Path::new(
            token.strip_prefix("./").unwrap_or(token),
        ))]
    }
}

fn push_unique_path(paths: &mut Vec<PathBuf>, path: PathBuf) {
    if !paths
        .iter()
        .any(|existing| platform::paths_equal(existing, &path))
    {
        paths.push(path);
    }
}

fn to_slash(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn project_search_filter_matches_path(relative_path: &str, filter: &str) -> bool {
    let tokens = split_pattern_tokens(filter);
    if tokens.is_empty() {
        return true;
    }
    let path = relative_path.to_lowercase();
    tokens
        .iter()
        .any(|token| project_search_filter_token_matches_path(&path, token))
}

fn project_search_filter_token_matches_path(path: &str, token: &str) -> bool {
    let token = token.trim().to_lowercase();
    if token.is_empty() {
        return true;
    }
    if let Some(suffix) = token.strip_prefix("*.").filter(|suffix| {
        !suffix.is_empty()
            && !suffix.as_bytes().contains(&b'*')
            && !suffix.contains('/')
            && !suffix.contains('\\')
    }) {
        return path
            .strip_suffix(suffix)
            .is_some_and(|prefix| prefix.as_bytes().last() == Some(&b'.'));
    }
    if token.as_bytes().contains(&b'*') {
        return false;
    }
    path.contains(&token)
}

#[cfg(test)]
mod matcher_tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};
        fn temp_workspace(name: &str) -> PathBuf {
            let nanos = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            std::env::temp_dir().join(format!("rriter_project_search_{name}_{nanos}"))
        }

        #[test]
        fn unicode_case_insensitive_match_maps_lowercase_expansion_to_source_character() {
            let text = "İstanbul";
            let mut ranges = Vec::new();

            collect_unicode_case_insensitive_matches(text, b"i", |start, end| {
                ranges.push((start, end));
                true
            });

            assert_eq!(ranges, vec![(0, "İ".len())]);
            assert_eq!(&text[ranges[0].0..ranges[0].1], "İ");
        }

        #[test]
        fn project_search_pattern_plan_clamps_absolute_paths_to_workspace() {
            let root = temp_workspace("pattern");
            std::fs::create_dir_all(root.join("src")).unwrap();
            let outside = root
                .parent()
                .unwrap_or_else(|| Path::new(std::path::MAIN_SEPARATOR_STR))
                .join("rriter-not-in-workspace");
            let excluded = outside.join("also-outside");
            let plan = SearchPatternPlan::new(
                &[root.clone()],
                &format!("{}, {}", root.join("src").display(), outside.display()),
                &excluded.to_string_lossy(),
            )
            .unwrap();

            assert_eq!(plan.include_roots, vec![root.join("src")]);
            assert!(plan.exclude_roots.is_empty());
            let _ = std::fs::remove_dir_all(root);
        }

        #[test]
        fn project_search_preview_keeps_match_visible_and_ranged() {
            let line = format!("{}needle{}", "a".repeat(120), "b".repeat(120));
            let (preview, start, end) = preview_line_with_match(&line, 120, 126);

            assert!(preview.starts_with("..."));
            assert!(preview.contains("needle"));
            assert_eq!(&preview[start..end], "needle");
        }
}
