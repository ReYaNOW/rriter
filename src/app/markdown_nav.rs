//! Navigation model of the Markdown reader: heading slugs, link targets and media paragraphs.
//!
//! Everything here is pure: it works on the parsed `MarkdownDocument`, the document source and the
//! document folder, and never touches the disk or the network. Destinations are untrusted text
//! from the document, so every unknown scheme resolves to `Unsupported` and percent-decoding
//! never fails (an invalid sequence stays as written).

use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::ops::Range;
use std::path::{Path, PathBuf};

use crate::languages::markdown::{
    MarkdownBlockKind, MarkdownDocument, MarkdownHeading, MarkdownInlineSpan, MarkdownInlineStyle,
    inline_plain_text, normalize_link_label,
};
use crate::markdown_media::{MediaKey, MediaSource};
use crate::platform::PathKey;

/// Where a link of the document leads.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum LinkTarget {
    /// `http`, `https` or `mailto` destination, as written.
    External(String),
    /// Decoded fragment of a `#fragment` link; matched against heading slugs by the caller.
    Anchor(String),
    /// Local path (joined to the document folder, not checked for existence) and optional
    /// decoded fragment.
    File {
        path: PathBuf,
        anchor: Option<String>,
    },
    /// Empty destination, undefined reference label or a scheme the reader does not open.
    Unsupported,
}

/// One picture of a media paragraph or a Mermaid block.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct MediaItem {
    pub(crate) key: MediaKey,
    pub(crate) source: MediaSource,
    pub(crate) alt: String,
    /// Target of the link wrapped around the image (badges), if any.
    pub(crate) link: Option<LinkTarget>,
    /// Source range of the whole element: the image, or the link that wraps it.
    pub(crate) source_range: Range<usize>,
}

/// GitHub-style slugs for `headings`, one per heading and all distinct. The first heading that
/// yields a slug keeps it; a later clash takes the first free `-1`, `-2`... suffix.
pub(crate) fn heading_slugs(headings: &[MarkdownHeading]) -> Vec<String> {
    let mut used: HashSet<String> = HashSet::new();
    let mut next_suffix: HashMap<String, usize> = HashMap::new();
    let mut slugs = Vec::with_capacity(headings.len());
    for heading in headings {
        let base = slugify(&heading.text);
        let slug = if used.contains(&base) {
            let counter = next_suffix.entry(base.clone()).or_insert(1);
            loop {
                let candidate = format!("{base}-{counter}");
                *counter += 1;
                if !used.contains(&candidate) {
                    break candidate;
                }
            }
        } else {
            base
        };
        used.insert(slug.clone());
        slugs.push(slug);
    }
    slugs
}

fn slugify(text: &str) -> String {
    let mut slug = String::with_capacity(text.len());
    for ch in text.chars() {
        if ch.is_whitespace() {
            slug.push('-');
        } else if ch.is_alphanumeric() {
            slug.extend(ch.to_lowercase());
        } else if ch == '-' || ch == '_' {
            slug.push(ch);
        }
    }
    slug
}

/// Resolves a link destination as written in the document. `[label]` destinations are looked up
/// in `defs` (normalized label, destination); `doc_dir` anchors relative paths.
pub(crate) fn resolve_link(dest: &str, doc_dir: &Path, defs: &[(String, String)]) -> LinkTarget {
    let Some(dest) = expand_reference(dest, defs) else {
        return LinkTarget::Unsupported;
    };
    if dest.is_empty() {
        return LinkTarget::Unsupported;
    }
    if let Some(scheme) = url_scheme(dest) {
        let external = ["http", "https", "mailto"]
            .iter()
            .any(|allowed| scheme.eq_ignore_ascii_case(allowed));
        return if external {
            LinkTarget::External(dest.to_string())
        } else {
            LinkTarget::Unsupported
        };
    }
    if let Some(fragment) = dest.strip_prefix('#') {
        return LinkTarget::Anchor(percent_decode(fragment));
    }
    let (path, fragment) = match dest.split_once('#') {
        Some((path, fragment)) => (path, Some(fragment)),
        None => (dest, None),
    };
    LinkTarget::File {
        path: doc_dir.join(percent_decode(path)),
        anchor: fragment.map(percent_decode).filter(|anchor| !anchor.is_empty()),
    }
}

/// Items of a paragraph made only of images, whitespace and links wrapped around one image;
/// `None` when `block_index` is not such a top-level paragraph.
pub(crate) fn media_paragraph(
    doc: &MarkdownDocument,
    block_index: usize,
    source: &str,
    doc_dir: &Path,
    defs: &[(String, String)],
) -> Option<Vec<MediaItem>> {
    let block = doc.blocks.get(block_index)?;
    let MarkdownBlockKind::Paragraph { inlines, .. } = &block.kind else {
        return None;
    };
    let mut items = Vec::new();
    for span in inlines {
        match &span.style {
            MarkdownInlineStyle::Image { .. } => {
                items.push(image_item(source, span, span, None, doc_dir, defs));
            }
            MarkdownInlineStyle::Link {
                destination_range,
                reference_range,
            } => {
                let mut images = Vec::new();
                if !collect_link_images(source, &span.children, &mut images) || images.len() != 1
                {
                    return None;
                }
                let dest = span_destination(
                    source,
                    span,
                    destination_range.as_ref(),
                    reference_range.as_ref(),
                );
                let link = resolve_link(&dest, doc_dir, defs);
                items.push(image_item(source, images[0], span, Some(link), doc_dir, defs));
            }
            MarkdownInlineStyle::HardBreak => {}
            MarkdownInlineStyle::Text if is_blank(source, span) => {}
            _ => return None,
        }
    }
    (!items.is_empty()).then_some(items)
}

/// Media item for the source of a Mermaid code block.
pub(crate) fn mermaid_item(code: &str, source_range: Range<usize>) -> MediaItem {
    // In-process identity only: the key never reaches the disk, so the std hasher is enough.
    let mut hasher = DefaultHasher::new();
    code.hash(&mut hasher);
    MediaItem {
        key: MediaKey::Mermaid(hasher.finish()),
        source: MediaSource::Mermaid(code.to_string()),
        alt: "mermaid".to_string(),
        link: None,
        source_range,
    }
}

fn image_item(
    source: &str,
    image: &MarkdownInlineSpan,
    element: &MarkdownInlineSpan,
    link: Option<LinkTarget>,
    doc_dir: &Path,
    defs: &[(String, String)],
) -> MediaItem {
    let (destination_range, reference_range) = match &image.style {
        MarkdownInlineStyle::Image {
            destination_range,
            reference_range,
        } => (destination_range.as_ref(), reference_range.as_ref()),
        _ => (None, None),
    };
    let dest = span_destination(source, image, destination_range, reference_range);
    let (key, media_source) = media_for_destination(&dest, doc_dir, defs);
    let mut alt = inline_plain_text(source, &image.children);
    // An image without a description node falls back to its whole source as text; that is not alt.
    if source.get(image.source_range.clone()) == Some(alt.as_str()) {
        alt.clear();
    }
    MediaItem {
        key,
        source: media_source,
        alt: alt.split_whitespace().collect::<Vec<_>>().join(" "),
        link,
        source_range: element.source_range.clone(),
    }
}

/// Local paths become file items; every URL, including schemes the loader cannot fetch, becomes
/// a URL item so the loader reports `Unsupported` and the block shows a frame.
fn media_for_destination(
    dest: &str,
    doc_dir: &Path,
    defs: &[(String, String)],
) -> (MediaKey, MediaSource) {
    let url_item = |url: &str| (MediaKey::Url(url.to_string()), MediaSource::Url(url.to_string()));
    let Some(dest) = expand_reference(dest, defs) else {
        return url_item(dest.trim());
    };
    if dest.is_empty() || dest.starts_with('#') || url_scheme(dest).is_some() {
        return url_item(dest);
    }
    let path = dest.split_once('#').map_or(dest, |(path, _)| path);
    let path = doc_dir.join(percent_decode(path));
    (MediaKey::File(PathKey::new(&path)), MediaSource::File(path))
}

/// Destination text of a link or image span: the inline destination, a reference label written
/// as `[label]`, or (collapsed and shortcut references) the span's own label text.
fn span_destination(
    source: &str,
    span: &MarkdownInlineSpan,
    destination_range: Option<&Range<usize>>,
    reference_range: Option<&Range<usize>>,
) -> String {
    if let Some(dest) = destination_range.and_then(|range| source.get(range.clone())) {
        return dest.to_string();
    }
    if let Some(label) = reference_range.and_then(|range| source.get(range.clone())) {
        if !normalize_link_label(label).is_empty() {
            return label.to_string();
        }
    }
    let whole = source.get(span.source_range.clone()).unwrap_or("");
    if reference_range.is_none() && whole.ends_with(')') {
        // `[text]()` / `![alt]()`: an inline form with an empty destination.
        return String::new();
    }
    let mut label = String::from("[");
    for range in &span.text_ranges {
        label.push_str(source.get(range.clone()).unwrap_or(""));
    }
    label.push(']');
    label
}

/// Collects the images under a link; false when anything but images and whitespace is present.
fn collect_link_images<'a>(
    source: &str,
    spans: &'a [MarkdownInlineSpan],
    images: &mut Vec<&'a MarkdownInlineSpan>,
) -> bool {
    for span in spans {
        match &span.style {
            MarkdownInlineStyle::Image { .. } => images.push(span),
            MarkdownInlineStyle::Text if !span.children.is_empty() => {
                if !collect_link_images(source, &span.children, images) {
                    return false;
                }
            }
            MarkdownInlineStyle::Text if is_blank(source, span) => {}
            MarkdownInlineStyle::HardBreak => {}
            _ => return false,
        }
    }
    true
}

fn is_blank(source: &str, span: &MarkdownInlineSpan) -> bool {
    span.text_ranges
        .iter()
        .all(|range| source.get(range.clone()).is_some_and(|text| text.trim().is_empty()))
}

/// The destination a reference resolves to, or the destination itself when it is not a
/// `[label]` reference; `None` for an undefined label.
fn expand_reference<'a>(dest: &'a str, defs: &'a [(String, String)]) -> Option<&'a str> {
    let trimmed = dest.trim();
    if trimmed.len() >= 2 && trimmed.starts_with('[') && trimmed.ends_with(']') {
        let label = normalize_link_label(trimmed);
        return defs
            .iter()
            .find(|(known, _)| *known == label)
            .map(|(_, target)| strip_angle_brackets(target));
    }
    Some(strip_angle_brackets(trimmed))
}

fn strip_angle_brackets(dest: &str) -> &str {
    let dest = dest.trim();
    dest.strip_prefix('<')
        .and_then(|rest| rest.strip_suffix('>'))
        .unwrap_or(dest)
        .trim()
}

/// URI scheme of `dest`, if it starts with one. A single letter is a Windows drive, not a scheme.
fn url_scheme(dest: &str) -> Option<&str> {
    let colon = dest.find(':')?;
    let scheme = &dest[..colon];
    let mut chars = scheme.chars();
    let first = chars.next()?;
    let valid = first.is_ascii_alphabetic()
        && chars.all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '+' | '.' | '-'));
    (valid && scheme.len() > 1).then_some(scheme)
}

/// Decodes `%XX` sequences. An invalid sequence stays as written; when the decoded bytes are not
/// UTF-8 the whole input is returned unchanged.
fn percent_decode(text: &str) -> String {
    if !text.contains('%') {
        return text.to_string();
    }
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        let decoded = match (bytes[index], bytes.get(index + 1..index + 3)) {
            (b'%', Some([high, low])) => hex_value(*high)
                .zip(hex_value(*low))
                .map(|(high, low)| high * 16 + low),
            _ => None,
        };
        match decoded {
            Some(byte) => {
                out.push(byte);
                index += 3;
            }
            None => {
                out.push(bytes[index]);
                index += 1;
            }
        }
    }
    String::from_utf8(out).unwrap_or_else(|_| text.to_string())
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::languages::markdown::MarkdownParseState;

    fn parse(source: &str) -> MarkdownDocument {
        let mut state = MarkdownParseState::default();
        state.parse(source).expect("markdown should parse")
    }

    fn heading(text: &str) -> MarkdownHeading {
        MarkdownHeading {
            level: 1,
            text: text.to_string(),
            source_range: 0..0,
        }
    }

    fn slugs(texts: &[&str]) -> Vec<String> {
        let headings: Vec<MarkdownHeading> = texts.iter().map(|text| heading(text)).collect();
        heading_slugs(&headings)
    }

    fn defs(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs
            .iter()
            .map(|(label, dest)| (label.to_string(), dest.to_string()))
            .collect()
    }

    fn dir() -> &'static Path {
        Path::new("/docs")
    }

    fn resolve(dest: &str) -> LinkTarget {
        resolve_link(dest, dir(), &[])
    }

    fn media(source: &str) -> Option<Vec<MediaItem>> {
        let document = parse(source);
        let definitions = document.link_definitions(source);
        media_paragraph(&document, 0, source, dir(), &definitions)
    }

    fn file_key(path: &str) -> MediaKey {
        MediaKey::File(PathKey::new(Path::new(path)))
    }

    #[test]
    fn slugs_follow_github_rules() {
        assert_eq!(slugs(&["Привет, мир!"]), vec!["привет-мир"]);
        assert_eq!(slugs(&["API v2.0"]), vec!["api-v20"]);
        assert_eq!(slugs(&["snake_case - dash"]), vec!["snake_case---dash"]);
        assert_eq!(slugs(&["ÉCOLE Ёж"]), vec!["école-ёж"]);
    }

    #[test]
    fn duplicate_slugs_get_suffixes_in_document_order() {
        assert_eq!(
            slugs(&["Intro", "Intro", "Intro"]),
            vec!["intro", "intro-1", "intro-2"]
        );
        assert_eq!(slugs(&["Тест", "Тест"]), vec!["тест", "тест-1"]);
    }

    #[test]
    fn slug_clashing_with_issued_suffix_stays_unique() {
        let all = slugs(&["Intro", "Intro", "Intro 1", "Intro", "Intro 1"]);
        assert_eq!(all[0], "intro");
        assert_eq!(all[1], "intro-1");
        let distinct: HashSet<&String> = all.iter().collect();
        assert_eq!(distinct.len(), all.len(), "{all:?}");
        // The earlier heading keeps the contested slug.
        assert_eq!(all[2], "intro-1-1");
        assert_eq!(all[3], "intro-2");
    }

    #[test]
    fn suffixed_heading_first_keeps_its_slug() {
        assert_eq!(
            slugs(&["Intro 1", "Intro", "Intro"]),
            vec!["intro-1", "intro", "intro-2"]
        );
    }

    #[test]
    fn empty_and_punctuation_only_headings_do_not_panic() {
        assert_eq!(slugs(&["", "!!!", ""]), vec!["", "-1", "-2"]);
        assert!(slugs(&[]).is_empty());
    }

    #[test]
    fn external_schemes_are_recognised_case_insensitively() {
        assert_eq!(
            resolve("https://e.com/a b"),
            LinkTarget::External("https://e.com/a b".to_string())
        );
        assert_eq!(
            resolve("HTTP://e.com"),
            LinkTarget::External("HTTP://e.com".to_string())
        );
        assert_eq!(
            resolve("mailto:a@b.c"),
            LinkTarget::External("mailto:a@b.c".to_string())
        );
        assert_eq!(
            resolve("<https://e.com>"),
            LinkTarget::External("https://e.com".to_string())
        );
    }

    #[test]
    fn dangerous_and_unknown_schemes_are_unsupported() {
        for dest in [
            "javascript:alert(1)",
            "JavaScript:alert(1)",
            "data:text/html;base64,AAAA",
            "file:///etc/passwd",
            "vbscript:x",
            "ftp://e.com/x",
        ] {
            assert_eq!(resolve(dest), LinkTarget::Unsupported, "{dest}");
        }
    }

    #[test]
    fn empty_and_whitespace_destinations_are_unsupported() {
        for dest in ["", "   ", "\t\n", "<>", "< >"] {
            assert_eq!(resolve(dest), LinkTarget::Unsupported, "{dest:?}");
        }
    }

    #[test]
    fn anchors_are_percent_decoded() {
        assert_eq!(resolve("#intro"), LinkTarget::Anchor("intro".to_string()));
        assert_eq!(
            resolve("#%D0%A0%D0%B0%D0%B7%D0%B4%D0%B5%D0%BB"),
            LinkTarget::Anchor("Раздел".to_string())
        );
        assert_eq!(resolve("#a%20b"), LinkTarget::Anchor("a b".to_string()));
        assert_eq!(resolve("#"), LinkTarget::Anchor(String::new()));
    }

    #[test]
    fn file_links_join_doc_dir_and_decode_path_and_fragment() {
        assert_eq!(
            resolve("a%20b.md#x%20y"),
            LinkTarget::File {
                path: PathBuf::from("/docs/a b.md"),
                anchor: Some("x y".to_string()),
            }
        );
        assert_eq!(
            resolve("other.md#Раздел"),
            LinkTarget::File {
                path: PathBuf::from("/docs/other.md"),
                anchor: Some("Раздел".to_string()),
            }
        );
        assert_eq!(
            resolve("sub/x.md#"),
            LinkTarget::File {
                path: PathBuf::from("/docs/sub/x.md"),
                anchor: None,
            }
        );
    }

    #[test]
    fn parent_dir_and_encoded_parent_dir_paths_stay_lexical() {
        let expected = LinkTarget::File {
            path: PathBuf::from("/docs/../x.md"),
            anchor: None,
        };
        assert_eq!(resolve("../x.md"), expected);
        assert_eq!(resolve("%2e%2e/x.md"), expected);
        assert_eq!(
            resolve("/abs/x.md"),
            LinkTarget::File {
                path: PathBuf::from("/abs/x.md"),
                anchor: None,
            }
        );
    }

    #[test]
    fn invalid_percent_sequences_stay_as_written() {
        assert_eq!(
            resolve("a%ZZ.md"),
            LinkTarget::File {
                path: PathBuf::from("/docs/a%ZZ.md"),
                anchor: None,
            }
        );
        assert_eq!(resolve("#100%"), LinkTarget::Anchor("100%".to_string()));
        assert_eq!(resolve("#a%2"), LinkTarget::Anchor("a%2".to_string()));
        // `%FF` alone is not UTF-8: the input is kept whole, including valid sequences around it.
        assert_eq!(resolve("#%FF%41"), LinkTarget::Anchor("%FF%41".to_string()));
    }

    #[test]
    fn reference_links_resolve_through_definitions() {
        let known = defs(&[("ref", "x.md"), ("site", "<https://e.com>")]);
        assert_eq!(
            resolve_link("[Ref]", dir(), &known),
            LinkTarget::File {
                path: PathBuf::from("/docs/x.md"),
                anchor: None,
            }
        );
        assert_eq!(
            resolve_link("[ SITE ]", dir(), &known),
            LinkTarget::External("https://e.com".to_string())
        );
        assert_eq!(
            resolve_link("[missing]", dir(), &known),
            LinkTarget::Unsupported
        );
        assert_eq!(resolve_link("[]", dir(), &known), LinkTarget::Unsupported);
        assert_eq!(resolve_link("[ref]", dir(), &[]), LinkTarget::Unsupported);
    }

    #[test]
    fn reference_to_dangerous_definition_is_unsupported() {
        let known = defs(&[("x", "javascript:alert(1)")]);
        assert_eq!(resolve_link("[x]", dir(), &known), LinkTarget::Unsupported);
    }

    #[test]
    fn definition_is_not_expanded_twice() {
        let known = defs(&[("a", "[b]"), ("b", "https://e.com")]);
        assert_eq!(
            resolve_link("[a]", dir(), &known),
            LinkTarget::File {
                path: PathBuf::from("/docs/[b]"),
                anchor: None,
            }
        );
    }

    #[test]
    fn reference_link_in_a_document_resolves_end_to_end() {
        let source = "[text][Ref]\n\n[ref]: x.md\n";
        let document = parse(source);
        let definitions = document.link_definitions(source);
        assert_eq!(definitions, defs(&[("ref", "x.md")]));
        assert_eq!(
            resolve_link("[Ref]", dir(), &definitions),
            LinkTarget::File {
                path: PathBuf::from("/docs/x.md"),
                anchor: None,
            }
        );
    }

    #[test]
    fn single_image_paragraph_is_a_local_file_item() {
        let source = "![a](x.png)\n";
        let items = media(source).expect("image paragraph");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].key, file_key("/docs/x.png"));
        assert_eq!(items[0].source, MediaSource::File(PathBuf::from("/docs/x.png")));
        assert_eq!(items[0].alt, "a");
        assert_eq!(items[0].link, None);
        assert_eq!(&source[items[0].source_range.clone()], "![a](x.png)");
    }

    #[test]
    fn percent_encoded_image_path_is_decoded() {
        let items = media("![a](my%20pic.png)\n").expect("image paragraph");
        assert_eq!(items[0].key, file_key("/docs/my pic.png"));
    }

    #[test]
    fn image_with_empty_alt_has_empty_alt() {
        let items = media("![](x.png)\n").expect("image paragraph");
        assert_eq!(items[0].alt, "");
    }

    #[test]
    fn badge_row_across_lines_gives_linked_url_items() {
        let source = "[![b1](https://img.shields.io/x.svg)](https://ci/1)\n[![b2](https://img.shields.io/y.svg)](https://ci/2)\n[![b3](https://img.shields.io/z.svg)](https://ci/3)\n";
        let items = media(source).expect("badge paragraph");
        assert_eq!(items.len(), 3);
        for (index, item) in items.iter().enumerate() {
            let number = index + 1;
            let image = format!("https://img.shields.io/{}.svg", ["x", "y", "z"][index]);
            assert_eq!(item.key, MediaKey::Url(image.clone()));
            assert_eq!(item.source, MediaSource::Url(image));
            assert_eq!(item.alt, format!("b{number}"));
            assert_eq!(
                item.link,
                Some(LinkTarget::External(format!("https://ci/{number}")))
            );
            assert!(source[item.source_range.clone()].starts_with("[!["));
        }
    }

    #[test]
    fn images_separated_by_spaces_keep_order() {
        let items = media("![a](x.png) ![b](y.png)   ![c](z.png)\n").expect("paragraph");
        let alts: Vec<&str> = items.iter().map(|item| item.alt.as_str()).collect();
        assert_eq!(alts, vec!["a", "b", "c"]);
    }

    #[test]
    fn image_mixed_with_text_is_not_a_media_paragraph() {
        assert_eq!(media("текст ![a](x.png)\n"), None);
        assert_eq!(media("![a](x.png) text\n"), None);
        assert_eq!(media("![a](x.png)\ntext\n"), None);
    }

    #[test]
    fn plain_text_empty_and_non_paragraph_blocks_are_none() {
        assert_eq!(media("just text\n"), None);
        assert_eq!(media("# ![a](x.png)\n"), None);
        assert_eq!(media("- ![a](x.png)\n"), None);
        let source = "![a](x.png)\n";
        let document = parse(source);
        assert_eq!(media_paragraph(&document, 5, source, dir(), &[]), None);
    }

    #[test]
    fn link_with_text_or_two_images_is_not_a_media_paragraph() {
        assert_eq!(media("[text ![a](x.png)](https://e.com)\n"), None);
        assert_eq!(media("[![a](x.png)![b](y.png)](https://e.com)\n"), None);
        assert_eq!(media("[plain](https://e.com)\n"), None);
    }

    #[test]
    fn unsupported_image_schemes_become_url_items() {
        let items = media("![a](data:image/png;base64,AAAA)\n").expect("paragraph");
        assert_eq!(
            items[0].source,
            MediaSource::Url("data:image/png;base64,AAAA".to_string())
        );
        let items = media("![a](javascript:alert(1))\n").expect("paragraph");
        assert!(matches!(items[0].key, MediaKey::Url(_)));
        assert!(matches!(items[0].source, MediaSource::Url(_)));
    }

    #[test]
    fn empty_image_destination_becomes_empty_url_item() {
        let items = media("![a]()\n").expect("paragraph");
        assert_eq!(items[0].source, MediaSource::Url(String::new()));
    }

    #[test]
    fn javascript_link_around_an_image_is_unsupported_link() {
        let items = media("[![a](x.png)](javascript:alert(1))\n").expect("paragraph");
        assert_eq!(items[0].link, Some(LinkTarget::Unsupported));
    }

    #[test]
    fn reference_images_and_links_resolve_through_definitions() {
        let source = "[![a][img]][site]\n\n[img]: pics/x.png\n[site]: https://e.com\n";
        let items = media(source).expect("paragraph");
        assert_eq!(items[0].key, file_key("/docs/pics/x.png"));
        assert_eq!(
            items[0].link,
            Some(LinkTarget::External("https://e.com".to_string()))
        );
        let items = media("![a][nope]\n").expect("paragraph");
        assert_eq!(items[0].source, MediaSource::Url("[nope]".to_string()));
    }

    #[test]
    fn collapsed_and_shortcut_image_references_use_the_alt_as_label() {
        let source = "![logo][]\n\n[logo]: l.png\n";
        let items = media(source).expect("paragraph");
        assert_eq!(items[0].key, file_key("/docs/l.png"));
        let source = "![logo]\n\n[logo]: l.png\n";
        let items = media(source).expect("paragraph");
        assert_eq!(items[0].key, file_key("/docs/l.png"));
    }

    #[test]
    fn mermaid_item_hashes_the_code() {
        let first = mermaid_item("graph TD; A-->B", 10..40);
        let same = mermaid_item("graph TD; A-->B", 50..80);
        let other = mermaid_item("graph TD; A-->C", 10..40);
        assert_eq!(first.key, same.key);
        assert_ne!(first.key, other.key);
        assert!(matches!(first.key, MediaKey::Mermaid(_)));
        assert_eq!(first.source, MediaSource::Mermaid("graph TD; A-->B".to_string()));
        assert_eq!(first.alt, "mermaid");
        assert_eq!(first.link, None);
        assert_eq!(first.source_range, 10..40);
    }

    #[test]
    fn headings_feed_unique_slugs_for_a_cyrillic_document() {
        let source = "# Привет\n\n## Привет\n\n### Привет, мир!\n";
        let document = parse(source);
        let all = heading_slugs(&document.headings(source));
        assert_eq!(all, vec!["привет", "привет-1", "привет-мир"]);
    }
}
