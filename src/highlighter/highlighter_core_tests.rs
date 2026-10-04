use super::*;
use std::sync::atomic::AtomicUsize;

#[test]
fn same_color_roles_stay_distinct_during_merge_and_rainbow_brackets() {
    let one_dark = SyntaxPalette::for_id(ThemeId::OneDark);
    assert_eq!(
        one_dark.color(SyntaxRole::Keyword),
        one_dark.color(SyntaxRole::Class)
    );

    let merged = merge_partial_highlight_spans(
        vec![ColorSpan {
            start: 0,
            end: 2,
            role: SyntaxRole::Keyword,
        }],
        vec![ColorSpan {
            start: 2,
            end: 4,
            role: SyntaxRole::Class,
        }],
        4..5,
    );
    assert_eq!(merged.len(), 2);
    assert_eq!(merged[0].role, SyntaxRole::Keyword);
    assert_eq!(merged[1].role, SyntaxRole::Class);

    let mut byte_roles = Vec::new();
    let bracketed = flatten_spans_for_range(
        vec![ColorSpan {
            start: 0,
            end: 3,
            role: SyntaxRole::Class,
        }],
        0..3,
        "(x)",
        &mut byte_roles,
        true,
    );
    assert!(bracketed.iter().all(|span| span.role == SyntaxRole::Class));
}

#[cfg(test)]
static ACTIVE_HIGHLIGHTER_WORKERS: AtomicUsize = AtomicUsize::new(0);

#[cfg(test)]
pub(super) struct ActiveHighlighterWorkerGuard;

#[cfg(test)]
impl ActiveHighlighterWorkerGuard {
    pub(super) fn new() -> Self {
        ACTIVE_HIGHLIGHTER_WORKERS.fetch_add(1, Ordering::AcqRel);
        Self
    }
}

#[cfg(test)]
impl Drop for ActiveHighlighterWorkerGuard {
    fn drop(&mut self) {
        ACTIVE_HIGHLIGHTER_WORKERS.fetch_sub(1, Ordering::AcqRel);
    }
}

#[cfg(test)]
pub(super) fn active_highlighter_worker_count() -> usize {
    ACTIVE_HIGHLIGHTER_WORKERS.load(Ordering::Acquire)
}

#[test]
fn query_cache_compiles_a_key_once_under_concurrent_get() {
    let (lang, queries) = get_ts_config("py").expect("python config");
    let cache = QueryCache::new();
    let barrier = std::sync::Barrier::new(8);
    let results: Vec<Option<Arc<tree_sitter::Query>>> = thread::scope(|scope| {
        let handles: Vec<_> = (0..8)
            .map(|_| {
                scope.spawn(|| {
                    barrier.wait();
                    cache.get_or_compile("py", queries[0], &lang)
                })
            })
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });
    assert_eq!(cache.compile_count(), 1);
    assert_eq!(cache.len(), 1);
    let first = results[0].as_ref().expect("python query compiles");
    assert!(results
        .iter()
        .all(|r| Arc::ptr_eq(r.as_ref().expect("compiled"), first)));
}

#[test]
fn in_flight_version_is_published_only_while_the_job_runs() {
    let control = HighlighterWorkerControl::new();
    assert!(!control.is_in_flight(0));
    assert!(!control.is_in_flight(7));
    {
        let _job = control.begin_job(7);
        assert!(control.is_in_flight(7));
        assert!(!control.is_in_flight(6));
    }
    assert!(!control.is_in_flight(7));
    assert!(!Highlighter::new().is_worker_processing(1));
}

#[test]
fn query_cache_keeps_distinct_keys_and_caches_failed_compiles() {
    let (lang, queries) = get_ts_config("py").expect("python config");
    let cache = QueryCache::new();
    assert!(!cache.is_compiled("py", queries[0]));
    assert!(cache.get_or_compile("py", queries[0], &lang).is_some());
    assert!(cache.is_compiled("py", queries[0]));
    assert!(cache.get_or_compile("py", "(((", &lang).is_none());
    assert!(cache.get_or_compile("py", "(((", &lang).is_none());
    assert_eq!(cache.compile_count(), 2);
    assert_eq!(cache.len(), 2);
}

#[test]
fn prewarm_query_ignores_unknown_and_empty_extensions() {
    let highlighter = Highlighter::new();
    for ext in ["", "zzz-unknown", "log"] {
        highlighter.prewarm_query(ext);
    }
    assert_eq!(highlighter.query_cache.compile_count(), 0);
    assert_eq!(highlighter.query_cache.len(), 0);
}

#[test]
fn prewarm_query_lets_the_first_highlight_skip_compilation() {
    let mut highlighter = Highlighter::new();
    highlighter.prewarm_query("py");
    let (_, queries) = get_ts_config("py").expect("python config");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    while !all_query_sources("py", &queries)
        .iter()
        .all(|q| highlighter.query_cache.is_compiled("py", q))
    {
        assert!(std::time::Instant::now() < deadline, "prewarm did not finish");
        thread::sleep(std::time::Duration::from_millis(5));
    }
    let compiled = highlighter.query_cache.compile_count();
    assert!(compiled >= queries.len());

    // A second prewarm of the same extension is a no-op.
    highlighter.prewarm_query("py");
    highlighter.reset(1, "def f(a):\n    return a\n".to_string(), "py".to_string(), 0);
    assert!(highlighter.wait_for_first_result(1, std::time::Duration::from_secs(5)));
    assert!(!highlighter.spans.is_empty());
    assert_eq!(highlighter.query_cache.compile_count(), compiled);
}
