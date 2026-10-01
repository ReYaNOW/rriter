//! Glue between a click on a Reader link and the `App`: remembers the press, decides whether
//! the release is a click, runs the resulting `LinkAction` and applies the anchor of a link
//! that opened another document. The decisions themselves (`link_action`, `is_link_click`)
//! are pure and live in `markdown_nav`; hit-testing lives in the render view.

use std::ops::Range;
use std::path::PathBuf;

use super::markdown_nav::{LinkAction, heading_for_anchor, heading_slugs, is_link_click, link_action};
use super::{App, MarkdownMode};
use crate::ui_system::UiId;

impl App {
    /// Index of the layout link under `(x, y)`, `None` outside Read mode or off a link.
    pub(crate) fn markdown_read_link_at(&mut self, x: f32, y: f32) -> Option<u32> {
        if self.markdown_mode() != MarkdownMode::Read {
            return None;
        }
        let frame = self.ui_registry.rect_for(UiId::MarkdownReadBody)?;
        let version = self.editor.version;
        let scroll = self.scroll_y.current;
        let markdown = &self.markdown;
        self.renderer
            .as_mut()?
            .markdown_read_link_at(markdown, version, frame, scroll, x, y)
    }

    /// Called on the press that starts a Reader selection: remembers the link under it.
    pub(crate) fn remember_markdown_link_press(&mut self, x: f32, y: f32) {
        self.markdown.link_press = self.markdown_read_link_at(x, y).map(|index| (index, x, y));
    }

    /// Called on the release of a Reader selection. `Some` when the press and the release
    /// make a click on one link (the caller then drops the selection and runs the action);
    /// `None` leaves the gesture a normal selection.
    pub(crate) fn take_markdown_read_link_click(&mut self, x: f32, y: f32) -> Option<LinkAction> {
        let press = self.markdown.link_press.take()?;
        let scale = self.renderer.as_ref()?.scale_factor;
        let release = self.markdown_read_link_at(x, y);
        if !is_link_click(press, release, x, y, scale) {
            return None;
        }
        let target = self.markdown.read_layout.links().get(press.0 as usize)?.clone();
        let document = self.markdown.read_document(self.editor.version)?;
        let headings = document.headings(&self.markdown.read_source);
        let slugs = heading_slugs(&headings);
        Some(link_action(&target, &headings, &slugs))
    }

    pub(crate) fn run_markdown_link_action(&mut self, action: LinkAction) {
        match action {
            LinkAction::OpenUrl(url) => {
                if let Err(error) = crate::platform::open_url(self.external_requests.sink(), &url) {
                    eprintln!("cannot open link {url}: {error}");
                }
            }
            LinkAction::ScrollTo(range) => self.scroll_markdown_read_to(&range, false),
            // A link to a file that is not there must not create an empty tab for it.
            LinkAction::OpenMarkdown { path, .. } | LinkAction::OpenFile(path) if !path.is_file() => {
                eprintln!("link target is not a file: {}", path.display());
            }
            LinkAction::OpenMarkdown { path, anchor } => self.open_markdown_link(path, anchor),
            LinkAction::OpenFile(path) => self.open_file_in_tab(path, true),
            LinkAction::None => {}
        }
        if let Some(window) = self.window.as_ref() {
            window.request_redraw();
        }
    }

    /// Opens a linked Markdown file in Read mode; `anchor` is applied once its layout exists.
    /// A file that did not open (missing, unreadable) leaves the current document untouched.
    fn open_markdown_link(&mut self, path: PathBuf, anchor: Option<String>) {
        self.open_file_in_tab(path.clone(), true);
        let opened = self
            .file_path
            .as_deref()
            .is_some_and(|open| crate::platform::paths_equal(open, &path));
        if !opened || !self.active_document_is_markdown() {
            return;
        }
        self.set_markdown_mode(MarkdownMode::Read);
        self.markdown.pending_anchor = anchor;
    }

    /// Scrolls the Reader to the line of `range`; `jump` skips the animation.
    fn scroll_markdown_read_to(&mut self, range: &Range<usize>, jump: bool) {
        let Some(top) = self.markdown.read_layout.source_target_y(range) else {
            return;
        };
        self.markdown.mark_absolute_scroll_navigation_with_scroll(&mut self.scroll_y);
        if jump {
            self.scroll_y.jump_to(top);
        } else {
            self.scroll_y.animate_to(top);
        }
    }

    /// Frame step: scrolls to the heading of `pending_anchor` once the Reader has a layout of
    /// the current text. The anchor is dropped afterwards, also when no heading matches.
    pub(crate) fn apply_pending_markdown_anchor(&mut self) {
        if self.markdown.pending_anchor.is_none() {
            return;
        }
        let version = self.editor.version;
        if self.markdown_mode() != MarkdownMode::Read
            || !self.markdown.read_layout.is_for_version(version)
            || self.markdown.read_layout.content_height() <= 0.0
        {
            return;
        }
        let Some(anchor) = self.markdown.pending_anchor.take() else {
            return;
        };
        let Some(document) = self.markdown.read_document(version) else {
            return;
        };
        let headings = document.headings(&self.markdown.read_source);
        let slugs = heading_slugs(&headings);
        if let Some(range) = heading_for_anchor(&anchor, &headings, &slugs) {
            self.scroll_markdown_read_to(&range, true);
        }
    }
}
