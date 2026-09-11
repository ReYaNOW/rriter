// Shared Markdown Read/Edit vertical-scroll transition state.
// Included by markdown.rs so the central mode API stays in one module while
// pending/carry-over physics remain isolated from Markdown parsing/selection.

const MARKDOWN_SCROLL_TARGET_EPSILON: f32 = 0.01;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MarkdownPendingNavigationPolicy {
    PreserveMotion,
    PreserveDestinationTarget,
}

#[derive(Clone, Debug)]
pub(crate) enum MarkdownAbsoluteScrollTarget {
    Source {
        source_range: Range<usize>,
        viewport_ratio: f32,
    },
    Start,
    End,
}

#[derive(Clone, Debug)]
pub(crate) struct MarkdownAbsoluteScrollTargetNavigation {
    revision: u64,
    mode: MarkdownMode,
    target: MarkdownAbsoluteScrollTarget,
    resolved_target_y: Option<f32>,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct MarkdownReadDisplayedGeometry {
    pub(crate) version: u64,
    pub(crate) width: f32,
    pub(crate) scale: f32,
    pub(crate) font_size: f32,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct MarkdownEditDisplayedGeometry {
    pub(crate) version: u64,
    pub(crate) line_height: f32,
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum MarkdownDeferredCurrentGeometry {
    Read(MarkdownReadDisplayedGeometry),
    Edit {
        geometry: MarkdownEditDisplayedGeometry,
        viewport_inset: f32,
    },
}

#[derive(Clone, Debug)]
pub(crate) struct MarkdownScrollTransition {
    pub(crate) from: MarkdownMode,
    pub(crate) to: MarkdownMode,
    pub(crate) version: u64,
    pub(crate) origin_scroll_y: f32,
    pub(crate) origin_sticky_lines: usize,
    pub(crate) origin_navigation_revision: u64,
    pub(crate) origin_read_width: Option<f32>,
    pub(crate) anchor: Option<MarkdownSourceAnchor>,
}

#[derive(Clone, Debug)]
pub(crate) struct MarkdownScrollCarry {
    pub(crate) anchor: MarkdownSourceAnchor,
    pub(crate) version: u64,
    pub(crate) edit_line_y: f32,
    pub(crate) expected_target_y: f32,
    pub(crate) navigation_revision: u64,
}

impl MarkdownTabState {
    pub(crate) fn scroll_navigation_revision(&self) -> u64 {
        self.scroll_navigation_revision
    }

    pub(crate) fn mark_absolute_scroll_navigation(&mut self) {
        self.scroll_navigation_revision = self.scroll_navigation_revision.wrapping_add(1);
        self.scroll_target_navigation_revision = None;
        self.scroll_target_navigation = None;
        self.scroll_transition = None;
        self.deferred_current_geometry = None;
        self.scroll_carry = None;
    }

    pub(crate) fn mark_absolute_scroll_navigation_with_scroll(
        &mut self,
        scroll: &mut crate::scroll::ScrollState,
    ) {
        self.mark_absolute_scroll_navigation();
        scroll.clear_deferred_current_rebase();
    }

    pub(crate) fn mark_absolute_scroll_target_navigation(&mut self) {
        self.mark_absolute_scroll_target_navigation_with(None);
    }

    pub(crate) fn mark_absolute_source_scroll_target_navigation(
        &mut self,
        source_range: Range<usize>,
        viewport_ratio: f32,
    ) {
        let target = viewport_ratio
            .is_finite()
            .then_some(MarkdownAbsoluteScrollTarget::Source {
                source_range,
                viewport_ratio: viewport_ratio.clamp(0.0, 1.0),
            });
        self.mark_absolute_scroll_target_navigation_with(target);
    }

    pub(crate) fn mark_absolute_scroll_start_navigation(&mut self) {
        self.mark_absolute_scroll_target_navigation_with(Some(MarkdownAbsoluteScrollTarget::Start));
    }

    pub(crate) fn mark_absolute_scroll_end_navigation(&mut self) {
        self.mark_absolute_scroll_target_navigation_with(Some(MarkdownAbsoluteScrollTarget::End));
    }

    fn mark_absolute_scroll_target_navigation_with(
        &mut self,
        target: Option<MarkdownAbsoluteScrollTarget>,
    ) {
        self.scroll_navigation_revision = self.scroll_navigation_revision.wrapping_add(1);
        self.scroll_target_navigation_revision = Some(self.scroll_navigation_revision);
        self.scroll_target_navigation =
            target.map(|target| MarkdownAbsoluteScrollTargetNavigation {
                revision: self.scroll_navigation_revision,
                mode: self.mode,
                target,
                resolved_target_y: None,
            });
        self.scroll_carry = None;
    }

    pub(crate) fn pending_absolute_scroll_target(
        &self,
        mode: MarkdownMode,
        scroll_target_y: f32,
    ) -> Option<(MarkdownAbsoluteScrollTarget, f32)> {
        let navigation = self.scroll_target_navigation.as_ref()?;
        if navigation.revision != self.scroll_navigation_revision
            || self.scroll_target_navigation_revision != Some(navigation.revision)
            || navigation.mode != mode
        {
            return None;
        }
        let relative_delta = navigation
            .resolved_target_y
            .and_then(|resolved| {
                let delta = scroll_target_y - resolved;
                delta.is_finite().then_some(delta)
            })
            .unwrap_or(0.0);
        Some((navigation.target.clone(), relative_delta))
    }

    pub(crate) fn remember_pending_absolute_scroll_target_y(&mut self, target_y: f32) {
        if !target_y.is_finite() {
            return;
        }
        if let Some(navigation) = self.scroll_target_navigation.as_mut()
            && navigation.revision == self.scroll_navigation_revision
            && self.scroll_target_navigation_revision == Some(navigation.revision)
            && navigation.mode == self.mode
        {
            navigation.resolved_target_y = Some(target_y);
        }
    }

    pub(crate) fn refresh_pending_read_absolute_target(
        &mut self,
        scroll: &mut crate::scroll::ScrollState,
        view_height: f32,
        max_scroll: f32,
    ) -> bool {
        let Some((target, relative_delta)) =
            self.pending_absolute_scroll_target(MarkdownMode::Read, scroll.target)
        else {
            return true;
        };
        if !view_height.is_finite() || !max_scroll.is_finite() {
            return false;
        }
        let target = match target {
            MarkdownAbsoluteScrollTarget::Source {
                source_range,
                viewport_ratio,
            } => {
                let Some(line_y) = self.read_layout.source_target_y(&source_range) else {
                    return false;
                };
                line_y - view_height * viewport_ratio
            }
            MarkdownAbsoluteScrollTarget::Start => 0.0,
            MarkdownAbsoluteScrollTarget::End => max_scroll,
        };
        if !target.is_finite() {
            return false;
        }
        let max_scroll = max_scroll.max(0.0);
        let resolved_target = target.clamp(0.0, max_scroll).round();
        let target = (resolved_target + relative_delta).clamp(0.0, max_scroll);
        scroll.set_target(target);
        self.remember_pending_absolute_scroll_target_y(resolved_target);
        true
    }

    pub(crate) fn settle_pending_target_navigation_as_motion(&mut self) {
        if self.scroll_target_navigation_revision == Some(self.scroll_navigation_revision) {
            if let Some(transition) = self.scroll_transition.as_mut() {
                transition.origin_navigation_revision = self.scroll_navigation_revision;
            }
            self.scroll_target_navigation_revision = None;
            self.scroll_target_navigation = None;
            self.scroll_carry = None;
        }
    }

    fn transition_navigation_policy(
        &self,
        transition: &MarkdownScrollTransition,
        editor_version: u64,
    ) -> Option<MarkdownPendingNavigationPolicy> {
        if transition.version != editor_version {
            return None;
        }
        if transition.origin_navigation_revision == self.scroll_navigation_revision {
            return Some(MarkdownPendingNavigationPolicy::PreserveMotion);
        }
        (self.scroll_target_navigation_revision == Some(self.scroll_navigation_revision))
            .then_some(MarkdownPendingNavigationPolicy::PreserveDestinationTarget)
    }

    pub(crate) fn pending_transition_is_valid(
        &self,
        transition: &MarkdownScrollTransition,
        editor_version: u64,
    ) -> bool {
        self.transition_navigation_policy(transition, editor_version)
            .is_some()
    }

    pub(crate) fn pending_read_accepts_stale_editor_surface(&self, editor_version: u64) -> bool {
        self.scroll_transition.as_ref().is_some_and(|transition| {
            transition.from == MarkdownMode::Edit
                && transition.to == MarkdownMode::Read
                && self.pending_transition_is_valid(transition, editor_version)
        })
    }

    pub(crate) fn remember_displayed_read_geometry(
        &mut self,
        version: u64,
        width: f32,
        scale: f32,
        font_size: f32,
    ) {
        if width.is_finite() && scale.is_finite() && font_size.is_finite() {
            self.last_read_geometry = Some(MarkdownReadDisplayedGeometry {
                version,
                width: width.max(1.0),
                scale,
                font_size,
            });
        }
    }

    pub(crate) fn remember_displayed_edit_geometry(&mut self, version: u64, line_height: f32) {
        if line_height.is_finite() && line_height > 0.0 {
            self.last_edit_geometry = Some(MarkdownEditDisplayedGeometry {
                version,
                line_height,
            });
        }
    }

    pub(crate) fn displayed_edit_line_height(&self, version: u64) -> Option<f32> {
        self.last_edit_geometry
            .filter(|geometry| geometry.version == version)
            .map(|geometry| geometry.line_height)
    }

    pub(crate) fn deferred_read_current_geometry(&self) -> Option<MarkdownReadDisplayedGeometry> {
        match self.deferred_current_geometry {
            Some(MarkdownDeferredCurrentGeometry::Read(geometry)) => Some(geometry),
            _ => None,
        }
    }

    pub(crate) fn deferred_edit_current_geometry(
        &self,
    ) -> Option<(MarkdownEditDisplayedGeometry, f32)> {
        match self.deferred_current_geometry {
            Some(MarkdownDeferredCurrentGeometry::Edit {
                geometry,
                viewport_inset,
            }) => Some((geometry, viewport_inset)),
            _ => None,
        }
    }

    pub(crate) fn deferred_current_geometry_matches_read(
        &self,
        version: u64,
        width: f32,
        scale: f32,
        font_size: f32,
    ) -> bool {
        self.deferred_read_current_geometry()
            .is_some_and(|geometry| {
                geometry.version == version
                    && geometry.width.max(1.0).round().to_bits() == width.max(1.0).round().to_bits()
                    && geometry.scale.to_bits() == scale.to_bits()
                    && geometry.font_size.to_bits() == font_size.to_bits()
            })
    }

    pub(crate) fn deferred_current_geometry_matches_edit(
        &self,
        version: u64,
        line_height: f32,
        viewport_inset: f32,
    ) -> bool {
        self.deferred_edit_current_geometry()
            .is_some_and(|(geometry, inset)| {
                geometry.version == version
                    && geometry.line_height.to_bits() == line_height.to_bits()
                    && inset.to_bits() == viewport_inset.to_bits()
            })
    }

    pub(crate) fn applied_read_current_anchor(
        &self,
        scroll: &crate::scroll::ScrollState,
        editor_version: u64,
    ) -> Option<MarkdownSourceAnchor> {
        if scroll.deferred_current_rebase_applied() != Some(true) {
            return None;
        }
        let geometry = self.deferred_read_current_geometry()?;
        if geometry.version != editor_version
            || !self.read_layout.is_valid_for_geometry(
                geometry.version,
                geometry.width,
                geometry.scale,
                geometry.font_size,
            )
        {
            return None;
        }
        self.read_layout.viewport_source_anchor(scroll.current)
    }

    pub(crate) fn reproject_applied_read_current(
        &mut self,
        scroll: &mut crate::scroll::ScrollState,
        editor_version: u64,
        anchor: &MarkdownSourceAnchor,
        destination_line_y: f32,
        destination_width: f32,
        destination_scale: f32,
        destination_font_size: f32,
    ) -> bool {
        let Some(transition) = self.scroll_transition.as_ref() else {
            return false;
        };
        if transition.to != MarkdownMode::Read
            || scroll.deferred_current_rebase_applied() != Some(true)
            || !destination_line_y.is_finite()
            || !destination_width.is_finite()
            || !destination_scale.is_finite()
            || !destination_font_size.is_finite()
        {
            return false;
        }
        let Some(policy) = self.transition_navigation_policy(transition, editor_version) else {
            return false;
        };
        let new_current = anchor.projected_scroll_y(destination_line_y);
        let preserve_destination_target = matches!(
            policy,
            MarkdownPendingNavigationPolicy::PreserveDestinationTarget
        );
        if !scroll.reproject_applied_deferred_current(new_current, preserve_destination_target) {
            return false;
        }
        self.deferred_current_geometry = Some(MarkdownDeferredCurrentGeometry::Read(
            MarkdownReadDisplayedGeometry {
                version: editor_version,
                width: destination_width.max(1.0),
                scale: destination_scale,
                font_size: destination_font_size,
            },
        ));
        true
    }

    pub(crate) fn reproject_applied_edit_current(
        &mut self,
        scroll: &mut crate::scroll::ScrollState,
        editor_version: u64,
        anchor: &MarkdownSourceAnchor,
        destination_line_y: f32,
        destination_line_height: f32,
        destination_viewport_inset: f32,
    ) -> bool {
        let Some(transition) = self.scroll_transition.as_ref() else {
            return false;
        };
        if transition.to != MarkdownMode::Edit
            || scroll.deferred_current_rebase_applied() != Some(true)
            || self
                .transition_navigation_policy(transition, editor_version)
                .is_none()
            || !destination_line_y.is_finite()
            || !destination_line_height.is_finite()
            || destination_line_height <= 0.0
            || !destination_viewport_inset.is_finite()
        {
            return false;
        }

        let new_current =
            anchor.projected_scroll_y(destination_line_y) - destination_viewport_inset;
        // This helper runs before a newer absolute target is assigned. Move the
        // old target by the same geometry delta for the tiny interval between
        // preparation and assignment; Search/Home/End then replace it normally.
        if !scroll.reproject_applied_deferred_current(new_current, false) {
            return false;
        }
        self.deferred_current_geometry = Some(MarkdownDeferredCurrentGeometry::Edit {
            geometry: MarkdownEditDisplayedGeometry {
                version: editor_version,
                line_height: destination_line_height,
            },
            viewport_inset: destination_viewport_inset,
        });
        true
    }

    pub(crate) fn shared_vertical_scroll_uses_editor_bounds(&self) -> bool {
        self.mode != MarkdownMode::Read && self.scroll_transition.is_none()
    }

    pub(crate) fn read_scroll_bounds(&self) -> Option<f32> {
        self.read_scroll_bounds_valid
            .then_some(self.read_max_scroll.max(0.0))
    }

    pub(crate) fn set_read_scroll_bounds(&mut self, max_scroll: f32) {
        self.read_max_scroll = if max_scroll.is_finite() {
            max_scroll.max(0.0)
        } else {
            0.0
        };
        self.read_scroll_bounds_valid = max_scroll.is_finite();
    }

    pub(crate) fn invalidate_read_scroll_bounds(&mut self) {
        self.read_max_scroll = 0.0;
        self.read_scroll_bounds_valid = false;
    }

    pub(crate) fn on_shared_vertical_scroll_changed(&mut self) {
        self.copied_code_block = None;
    }

    pub(crate) fn pending_transition_for(
        &self,
        mode: MarkdownMode,
    ) -> Option<MarkdownScrollTransition> {
        self.scroll_transition
            .as_ref()
            .filter(|transition| transition.to == mode)
            .cloned()
    }

    pub(crate) fn cancel_stale_scroll_transition(&mut self) {
        self.scroll_transition = None;
        self.scroll_target_navigation_revision = None;
        self.scroll_target_navigation = None;
        self.deferred_current_geometry = None;
        self.scroll_carry = None;
    }

    fn defer_scroll_transition_current_to_destination(
        &mut self,
        scroll: &mut crate::scroll::ScrollState,
        editor_version: u64,
        anchor: &MarkdownSourceAnchor,
        destination_line_y: f32,
        destination_viewport_inset: f32,
        destination_geometry: MarkdownDeferredCurrentGeometry,
    ) -> bool {
        let Some(transition) = self.scroll_transition.as_ref() else {
            return true;
        };
        if transition.to != self.mode
            || self
                .transition_navigation_policy(transition, editor_version)
                .is_none()
            || !destination_line_y.is_finite()
            || !destination_viewport_inset.is_finite()
        {
            return false;
        }
        if scroll.deferred_current_rebase_applied() == Some(true) {
            return true;
        }
        let progressed = scroll.current - transition.origin_scroll_y;
        let destination_at_capture =
            anchor.projected_scroll_y(destination_line_y) - destination_viewport_inset;
        let new_current = destination_at_capture + progressed;
        let deferred = progressed.is_finite()
            && destination_at_capture.is_finite()
            && scroll.defer_current_rebase_preserving_target(new_current);
        if deferred {
            self.deferred_current_geometry = Some(destination_geometry);
        }
        deferred
    }

    pub(crate) fn apply_scroll_transition(
        &mut self,
        scroll: &mut crate::scroll::ScrollState,
        editor_version: u64,
        anchor: MarkdownSourceAnchor,
        destination_line_y: f32,
        destination_viewport_inset: f32,
        max_scroll: f32,
    ) -> bool {
        self.apply_scroll_transition_with_reprojection(
            scroll,
            editor_version,
            anchor,
            destination_line_y,
            destination_viewport_inset,
            max_scroll,
            false,
        )
    }

    pub(crate) fn apply_scroll_transition_with_reprojection(
        &mut self,
        scroll: &mut crate::scroll::ScrollState,
        editor_version: u64,
        anchor: MarkdownSourceAnchor,
        destination_line_y: f32,
        destination_viewport_inset: f32,
        max_scroll: f32,
        reproject_applied_current: bool,
    ) -> bool {
        let Some(transition) = self.scroll_transition.take() else {
            return false;
        };
        let navigation_policy = self.transition_navigation_policy(&transition, editor_version);
        self.scroll_target_navigation_revision = None;
        self.scroll_target_navigation = None;
        if transition.to != self.mode
            || navigation_policy.is_none()
            || !destination_line_y.is_finite()
            || !destination_viewport_inset.is_finite()
            || !max_scroll.is_finite()
        {
            self.deferred_current_geometry = None;
            self.scroll_carry = None;
            return false;
        }

        let progressed = scroll.current - transition.origin_scroll_y;
        let destination_at_capture =
            anchor.projected_scroll_y(destination_line_y) - destination_viewport_inset;
        let deferred_applied = scroll.deferred_current_rebase_applied() == Some(true);
        let rebased = match navigation_policy {
            Some(MarkdownPendingNavigationPolicy::PreserveMotion) => {
                if deferred_applied && !reproject_applied_current {
                    scroll.complete_deferred_current_rebase()
                } else {
                    let new_current = if deferred_applied {
                        destination_at_capture
                    } else {
                        destination_at_capture + progressed
                    };
                    progressed.is_finite()
                        && destination_at_capture.is_finite()
                        && scroll.rebase_current_preserving_motion(new_current)
                }
            }
            Some(MarkdownPendingNavigationPolicy::PreserveDestinationTarget) => {
                if deferred_applied && !reproject_applied_current {
                    scroll.complete_deferred_current_rebase()
                } else {
                    let new_current = if deferred_applied {
                        destination_at_capture
                    } else {
                        destination_at_capture + progressed
                    };
                    progressed.is_finite()
                        && destination_at_capture.is_finite()
                        && scroll.rebase_current_preserving_target(new_current)
                }
            }
            None => false,
        };
        if !rebased {
            self.deferred_current_geometry = None;
            self.scroll_carry = None;
            return false;
        }
        self.deferred_current_geometry = None;

        let max_scroll = max_scroll.max(0.0);
        scroll.clamp_target(0.0, max_scroll);
        scroll.clamp_current(0.0, max_scroll);

        if transition.from == MarkdownMode::Read && transition.to == MarkdownMode::Edit {
            self.scroll_carry = Some(MarkdownScrollCarry {
                anchor,
                version: editor_version,
                edit_line_y: destination_line_y,
                expected_target_y: scroll.target,
                navigation_revision: self.scroll_navigation_revision,
            });
        } else {
            self.scroll_carry = None;
        }
        true
    }
}

impl App {
    pub(crate) fn markdown_read_content_width_for(
        &self,
        renderer_width: f32,
        scale: f32,
    ) -> f32 {
        let panel_left_w = if self.is_ide_mode {
            self.ide_panel.visible_left_width(scale)
        } else {
            0.0
        };
        let active_tab_is_git_diff = self
            .tabs
            .get(self.active_tab)
            .is_some_and(|tab| tab.kind.is_git_diff());
        let editor_text_x = crate::render_view::editor_left_padding_for(
            self.editor.line_offsets.len(),
            active_tab_is_git_diff,
            self.is_ide_mode,
            panel_left_w,
            scale,
        );
        let content_x =
            crate::render_view::markdown_read::markdown_read_frame_x_for_editor_text(
                editor_text_x,
                scale,
            );
        (renderer_width - content_x).max(1.0)
    }

    /// Before an absolute animated target is assigned, make sure the shared
    /// scroll current already belongs to the active Markdown mode. This keeps
    /// the single physics tick from integrating coordinates from two layouts.
    /// Read mode also prepares the current destination layout key so search/End
    /// never consume stale geometry from a previously displayed frame.
    pub(crate) fn prepare_markdown_absolute_scroll_target_navigation(&mut self) -> bool {
        if !self.active_document_is_markdown() {
            return true;
        }

        let mode = self.markdown.mode;
        if mode == MarkdownMode::Read && self.markdown.needs_read_model_refresh(self.editor.version)
        {
            self.finish_markdown_read_selection_gesture();
            let source = self.editor.get_full_text();
            if !self
                .markdown
                .refresh_read_model(self.editor.version, source)
            {
                return false;
            }
        }

        let Some(renderer) = self.renderer.as_ref() else {
            // Headless unit fixtures can provide a production-built Read layout
            // without a native renderer. Keep that existing search path usable;
            // real cold/stale destination geometry is handled below whenever a
            // renderer exists.
            return true;
        };
        let scale = renderer.scale_factor;
        let renderer_width = renderer.width;
        let renderer_height = renderer.height;
        let window_height = self
            .window
            .as_ref()
            .map(|window| window.inner_size().height as f32)
            .unwrap_or(renderer_height);
        let tab_bar_h = crate::render_view::editor_content_top_inset(
            self.show_welcome,
            self.is_ide_mode,
            self.active_tab_is_database_query(),
            scale,
        );
        let panel_bottom_h = if self.is_ide_mode {
            self.ide_panel.editor_reserved_bottom_height(scale)
        } else {
            0.0
        };
        let view_height = crate::render_view::editor_view_height(
            window_height,
            tab_bar_h,
            panel_bottom_h,
            self.is_ide_mode,
            scale,
        );

        match mode {
            MarkdownMode::Edit => {
                let Some(transition) = self.markdown.pending_transition_for(MarkdownMode::Edit)
                else {
                    return true;
                };
                if !self
                    .markdown
                    .pending_transition_is_valid(&transition, self.editor.version)
                {
                    self.markdown.cancel_stale_scroll_transition();
                    return false;
                }

                let Some(renderer) = self.renderer.as_mut() else {
                    return false;
                };
                let sticky_inset = self.current_sticky_lines.len() as f32 * renderer.line_height;
                if self.scroll_y.deferred_current_rebase_applied() == Some(true)
                    && !self.markdown.deferred_current_geometry_matches_edit(
                        self.editor.version,
                        renderer.line_height,
                        sticky_inset,
                    )
                {
                    let Some((owned_geometry, owned_viewport_inset)) =
                        self.markdown.deferred_edit_current_geometry()
                    else {
                        return false;
                    };
                    if owned_geometry.version != self.editor.version {
                        return false;
                    }
                    let Some(live_anchor) = renderer
                        .markdown_edit_viewport_anchor_with_line_height(
                            &self.editor,
                            self.scroll_y.current + owned_viewport_inset,
                            owned_geometry.line_height,
                        )
                    else {
                        return false;
                    };
                    let Some(destination_line_y) = renderer
                        .markdown_edit_source_y_with_line_height(
                            &self.editor,
                            &live_anchor.source_range,
                            renderer.line_height,
                        )
                    else {
                        return false;
                    };
                    if !self.markdown.reproject_applied_edit_current(
                        &mut self.scroll_y,
                        self.editor.version,
                        &live_anchor,
                        destination_line_y,
                        renderer.line_height,
                        sticky_inset,
                    ) {
                        return false;
                    }
                }

                let anchor = transition.anchor.clone().or_else(|| {
                    let geometry = self
                        .markdown
                        .last_read_geometry
                        .filter(|geometry| geometry.version == transition.version)?;
                    self.markdown
                        .read_layout
                        .is_valid_for_geometry(
                            geometry.version,
                            geometry.width,
                            geometry.scale,
                            geometry.font_size,
                        )
                        .then(|| {
                            self.markdown
                                .read_layout
                                .viewport_source_anchor(transition.origin_scroll_y)
                        })
                        .flatten()
                });
                let Some(anchor) = anchor else {
                    return false;
                };
                let Some(line_y) =
                    renderer.markdown_edit_source_y(&self.editor, &anchor.source_range)
                else {
                    return false;
                };
                self.markdown
                    .defer_scroll_transition_current_to_destination(
                        &mut self.scroll_y,
                        self.editor.version,
                        &anchor,
                        line_y,
                        sticky_inset,
                        MarkdownDeferredCurrentGeometry::Edit {
                            geometry: MarkdownEditDisplayedGeometry {
                                version: self.editor.version,
                                line_height: renderer.line_height,
                            },
                            viewport_inset: sticky_inset,
                        },
                    )
            }
            MarkdownMode::Read => {
                let content_width = self.markdown_read_content_width_for(renderer_width, scale);
                let Some(renderer) = self.renderer.as_mut() else {
                    return false;
                };
                if !renderer.prepare_markdown_read_layout_preserving_current_ownership(
                    &mut self.markdown,
                    &mut self.scroll_y,
                    self.editor.version,
                    content_width,
                ) {
                    return false;
                }
                let max_scroll =
                    (self.markdown.read_layout.content_height() - view_height).max(0.0);
                self.markdown.set_read_scroll_bounds(max_scroll);

                let Some(transition) = self.markdown.pending_transition_for(MarkdownMode::Read)
                else {
                    return true;
                };
                if !self
                    .markdown
                    .pending_transition_is_valid(&transition, self.editor.version)
                {
                    self.markdown.cancel_stale_scroll_transition();
                    return false;
                }

                let anchor = transition.anchor.clone().or_else(|| {
                    if transition.from != MarkdownMode::Edit {
                        return None;
                    }
                    let origin_line_height = self
                        .markdown
                        .displayed_edit_line_height(transition.version)
                        .unwrap_or(renderer.line_height);
                    let viewport_y = transition.origin_scroll_y
                        + transition.origin_sticky_lines as f32 * origin_line_height;
                    renderer.markdown_edit_viewport_anchor_with_line_height(
                        &self.editor,
                        viewport_y,
                        origin_line_height,
                    )
                });
                let Some(anchor) = anchor else {
                    return false;
                };
                let Some(line_y) = self
                    .markdown
                    .read_layout
                    .source_anchor_y(&anchor.source_range)
                else {
                    return false;
                };
                self.markdown
                    .defer_scroll_transition_current_to_destination(
                        &mut self.scroll_y,
                        self.editor.version,
                        &anchor,
                        line_y,
                        0.0,
                        MarkdownDeferredCurrentGeometry::Read(MarkdownReadDisplayedGeometry {
                            version: self.editor.version,
                            width: content_width,
                            scale: renderer.scale_factor,
                            font_size: renderer.font_size,
                        }),
                    )
            }
        }
    }

    fn capture_markdown_scroll_anchor(
        &mut self,
        from: MarkdownMode,
        to: MarkdownMode,
        deferred_current_geometry: Option<MarkdownDeferredCurrentGeometry>,
    ) -> (Option<MarkdownSourceAnchor>, Option<f32>, usize) {
        let origin_scroll_y = self.scroll_y.current;
        match from {
            MarkdownMode::Read => {
                let registry_width = self
                    .ui_registry
                    .rect_for(crate::ui_system::UiId::MarkdownReadBody)
                    .map(|(_, _, width, _)| width.max(1.0));
                let displayed_geometry = self
                    .markdown
                    .last_read_geometry
                    .filter(|geometry| geometry.version == self.editor.version);
                let owned_geometry = match deferred_current_geometry {
                    Some(MarkdownDeferredCurrentGeometry::Read(geometry))
                        if geometry.version == self.editor.version =>
                    {
                        Some(geometry)
                    }
                    _ => None,
                };
                let owned_anchor = owned_geometry.and_then(|geometry| {
                    self.markdown
                        .read_layout
                        .is_valid_for_geometry(
                            geometry.version,
                            geometry.width,
                            geometry.scale,
                            geometry.font_size,
                        )
                        .then(|| {
                            self.markdown
                                .read_layout
                                .viewport_source_anchor(origin_scroll_y)
                        })
                        .flatten()
                });
                let displayed_anchor = displayed_geometry.and_then(|geometry| {
                    self.markdown
                        .read_layout
                        .is_valid_for_geometry(
                            geometry.version,
                            geometry.width,
                            geometry.scale,
                            geometry.font_size,
                        )
                        .then(|| {
                            self.markdown
                                .read_layout
                                .viewport_source_anchor(origin_scroll_y)
                        })
                        .flatten()
                });
                let current_anchor = registry_width.and_then(|width| {
                    let renderer = self.renderer.as_ref()?;
                    self.markdown
                        .read_layout
                        .is_valid_for_geometry(
                            self.editor.version,
                            width,
                            renderer.scale_factor,
                            renderer.font_size,
                        )
                        .then(|| {
                            self.markdown
                                .read_layout
                                .viewport_source_anchor(origin_scroll_y)
                        })
                        .flatten()
                });
                let anchor = if deferred_current_geometry.is_some() {
                    owned_anchor
                } else {
                    displayed_anchor.or(current_anchor)
                };
                let origin_read_width = if deferred_current_geometry.is_some() {
                    owned_geometry.map(|geometry| geometry.width)
                } else {
                    displayed_geometry
                        .map(|geometry| geometry.width)
                        .or(registry_width)
                };
                (anchor, origin_read_width, 0)
            }
            MarkdownMode::Edit => {
                let sticky_lines = self.current_sticky_lines.len();
                let carry = self.markdown.scroll_carry.clone();
                let Some(renderer) = self.renderer.as_mut() else {
                    if to == MarkdownMode::Read {
                        self.markdown.scroll_carry = None;
                    }
                    return (None, None, sticky_lines);
                };
                let (origin_line_height, effective_viewport_y) = match deferred_current_geometry {
                    Some(MarkdownDeferredCurrentGeometry::Edit {
                        geometry,
                        viewport_inset,
                    }) if geometry.version == self.editor.version => {
                        (geometry.line_height, origin_scroll_y + viewport_inset)
                    }
                    _ => {
                        let line_height = self
                            .markdown
                            .displayed_edit_line_height(self.editor.version)
                            .unwrap_or(renderer.line_height);
                        (
                            line_height,
                            origin_scroll_y + sticky_lines as f32 * line_height,
                        )
                    }
                };
                let edit_anchor = renderer.markdown_edit_viewport_anchor_with_line_height(
                    &self.editor,
                    effective_viewport_y,
                    origin_line_height,
                );
                let anchor = if to == MarkdownMode::Read {
                    let carried = carry.and_then(|carry| {
                        if deferred_current_geometry.is_some()
                            || carry.version != self.editor.version
                            || carry.navigation_revision != self.markdown.scroll_navigation_revision
                            || !self.scroll_y.target.is_finite()
                            || (self.scroll_y.target - carry.expected_target_y).abs()
                                > MARKDOWN_SCROLL_TARGET_EPSILON
                        {
                            return None;
                        }
                        let current_line_y = renderer.markdown_edit_source_y_with_line_height(
                            &self.editor,
                            &carry.anchor.source_range,
                            origin_line_height,
                        )?;
                        if (current_line_y - carry.edit_line_y).abs() > 0.1 {
                            return None;
                        }

                        let projected_viewport_y =
                            carry.edit_line_y - carry.anchor.viewport_offset_y;
                        let projected_viewport_anchor = renderer
                            .markdown_edit_viewport_anchor_with_line_height(
                                &self.editor,
                                projected_viewport_y,
                                origin_line_height,
                            )?;
                        let visible_anchor = edit_anchor.as_ref()?;
                        if visible_anchor.source_range != projected_viewport_anchor.source_range {
                            return None;
                        }

                        let mut anchor = carry.anchor;
                        anchor.viewport_offset_y = current_line_y - effective_viewport_y;
                        Some(anchor)
                    });
                    if carried.is_none() {
                        self.markdown.scroll_carry = None;
                    }
                    carried.or(edit_anchor)
                } else {
                    edit_anchor
                };
                (anchor, None, sticky_lines)
            }
        }
    }
}

#[cfg(test)]
mod markdown_scroll_transition_tests {
    use super::*;

    #[test]
    fn markdown_pending_rebase_preserves_elapsed_motion_velocity_and_remaining_target() {
        let mut markdown = MarkdownTabState::default();
        markdown.mode = MarkdownMode::Read;
        markdown.scroll_transition = Some(MarkdownScrollTransition {
            from: MarkdownMode::Edit,
            to: MarkdownMode::Read,
            version: 9,
            origin_scroll_y: 100.0,
            origin_sticky_lines: 0,
            origin_navigation_revision: 0,
            origin_read_width: None,
            anchor: None,
        });
        let mut scroll = crate::scroll::ScrollState::new(7.0);
        scroll.current = 110.0;
        scroll.target = 190.0;
        scroll.velocity = 37.0;
        let anchor = MarkdownSourceAnchor {
            source_range: 20..30,
            viewport_offset_y: -5.0,
        };

        assert!(markdown.apply_scroll_transition(&mut scroll, 9, anchor, 600.0, 0.0, 2_000.0,));

        assert_eq!(scroll.current, 615.0);
        assert_eq!(scroll.target, 695.0);
        assert_eq!(scroll.target - scroll.current, 80.0);
        assert_eq!(scroll.velocity, 37.0);
        assert_eq!(scroll.anim_speed, 7.0);
        assert!(markdown.scroll_transition.is_none());
    }

    #[test]
    fn markdown_pending_rebase_preserves_negative_motion_without_new_acceleration() {
        let mut markdown = MarkdownTabState::default();
        markdown.mode = MarkdownMode::Edit;
        markdown.scroll_transition = Some(MarkdownScrollTransition {
            from: MarkdownMode::Read,
            to: MarkdownMode::Edit,
            version: 4,
            origin_scroll_y: 460.0,
            origin_sticky_lines: 0,
            origin_navigation_revision: 0,
            origin_read_width: Some(320.0),
            anchor: None,
        });
        let mut scroll = crate::scroll::ScrollState::new(7.0);
        scroll.current = 450.0;
        scroll.target = 300.0;
        scroll.velocity = -91.0;
        let anchor = MarkdownSourceAnchor {
            source_range: 40..50,
            viewport_offset_y: 8.0,
        };

        assert!(markdown.apply_scroll_transition(
            &mut scroll,
            4,
            anchor.clone(),
            400.0,
            20.0,
            2_000.0,
        ));

        assert_eq!(scroll.current, 362.0);
        assert_eq!(scroll.target, 212.0);
        assert_eq!(scroll.target - scroll.current, -150.0);
        assert_eq!(scroll.velocity, -91.0);
        assert_eq!(scroll.anim_speed, 7.0);
        let carry = markdown.scroll_carry.as_ref().expect("reader detail carry");
        assert_eq!(carry.anchor, anchor);
        assert_eq!(carry.edit_line_y, 400.0);
        assert_eq!(carry.expected_target_y, 212.0);
    }

    #[test]
    fn markdown_pending_transition_survives_relative_scroll_impulse() {
        let mut markdown = MarkdownTabState::default();
        markdown.mode = MarkdownMode::Read;
        markdown.scroll_transition = Some(MarkdownScrollTransition {
            from: MarkdownMode::Edit,
            to: MarkdownMode::Read,
            version: 3,
            origin_scroll_y: 80.0,
            origin_sticky_lines: 0,
            origin_navigation_revision: 0,
            origin_read_width: None,
            anchor: None,
        });
        let mut scroll = crate::scroll::ScrollState::new(7.0);
        scroll.current = 90.0;
        scroll.target = 160.0;
        scroll.velocity = 12.0;
        scroll.scroll_by(60.0);

        assert!(markdown.apply_scroll_transition(
            &mut scroll,
            3,
            MarkdownSourceAnchor {
                source_range: 0..1,
                viewport_offset_y: 0.0,
            },
            500.0,
            0.0,
            1_000.0,
        ));
        assert_eq!(scroll.current, 510.0);
        assert_eq!(scroll.target, 640.0);
        assert_eq!(scroll.target - scroll.current, 130.0);
        assert_eq!(scroll.velocity, 12.0);
        assert_eq!(scroll.anim_speed, 7.0);
    }

    #[test]
    fn markdown_pending_transition_is_discarded_after_absolute_navigation() {
        let mut markdown = MarkdownTabState::default();
        markdown.mode = MarkdownMode::Read;
        markdown.scroll_transition = Some(MarkdownScrollTransition {
            from: MarkdownMode::Edit,
            to: MarkdownMode::Read,
            version: 3,
            origin_scroll_y: 80.0,
            origin_sticky_lines: 0,
            origin_navigation_revision: 0,
            origin_read_width: None,
            anchor: None,
        });
        let mut scroll = crate::scroll::ScrollState::new(7.0);
        scroll.current = 90.0;
        scroll.target = 220.0;
        scroll.velocity = 12.0;
        markdown.mark_absolute_scroll_navigation();
        let before = (scroll.current, scroll.target, scroll.velocity);

        assert!(!markdown.apply_scroll_transition(
            &mut scroll,
            3,
            MarkdownSourceAnchor {
                source_range: 0..1,
                viewport_offset_y: 0.0,
            },
            500.0,
            0.0,
            1_000.0,
        ));
        assert_eq!((scroll.current, scroll.target, scroll.velocity), before);
        assert!(markdown.scroll_transition.is_none());
    }
    #[test]
    fn markdown_absolute_navigation_revision_invalidates_existing_carry() {
        let mut markdown = MarkdownTabState::default();
        markdown.scroll_carry = Some(MarkdownScrollCarry {
            anchor: MarkdownSourceAnchor {
                source_range: 10..20,
                viewport_offset_y: 5.0,
            },
            version: 2,
            edit_line_y: 240.0,
            expected_target_y: 160.0,
            navigation_revision: 0,
        });

        markdown.mark_absolute_scroll_navigation();

        assert_eq!(markdown.scroll_navigation_revision(), 1);
        assert!(markdown.scroll_carry.is_none());
    }

    #[test]
    fn markdown_read_bounds_distinguish_unknown_from_real_zero() {
        let mut markdown = MarkdownTabState::default();
        assert_eq!(markdown.read_scroll_bounds(), None);

        markdown.set_read_scroll_bounds(0.0);
        assert_eq!(markdown.read_scroll_bounds(), Some(0.0));

        markdown.invalidate_read_scroll_bounds();
        assert_eq!(markdown.read_scroll_bounds(), None);

        markdown.set_read_scroll_bounds(f32::NAN);
        assert_eq!(markdown.read_scroll_bounds(), None);
    }

    #[test]
    fn markdown_editor_bounds_wait_for_pending_rebase() {
        let mut markdown = MarkdownTabState::default();
        assert!(markdown.shared_vertical_scroll_uses_editor_bounds());

        markdown.mode = MarkdownMode::Read;
        assert!(!markdown.shared_vertical_scroll_uses_editor_bounds());

        markdown.mode = MarkdownMode::Edit;
        markdown.scroll_transition = Some(MarkdownScrollTransition {
            from: MarkdownMode::Read,
            to: MarkdownMode::Edit,
            version: 1,
            origin_scroll_y: 900.0,
            origin_sticky_lines: 0,
            origin_navigation_revision: 0,
            origin_read_width: Some(640.0),
            anchor: None,
        });
        assert!(!markdown.shared_vertical_scroll_uses_editor_bounds());
    }

    #[test]
    fn markdown_pending_rebase_keeps_zero_velocity_and_existing_target_delta() {
        let mut markdown = MarkdownTabState::default();
        markdown.mode = MarkdownMode::Read;
        markdown.scroll_transition = Some(MarkdownScrollTransition {
            from: MarkdownMode::Edit,
            to: MarkdownMode::Read,
            version: 12,
            origin_scroll_y: 80.0,
            origin_sticky_lines: 0,
            origin_navigation_revision: 0,
            origin_read_width: None,
            anchor: None,
        });
        let mut scroll = crate::scroll::ScrollState::new(7.0);
        scroll.current = 80.0;
        scroll.target = 160.0;
        scroll.velocity = 0.0;
        let anchor = MarkdownSourceAnchor {
            source_range: 5..15,
            viewport_offset_y: -4.0,
        };

        assert!(markdown.apply_scroll_transition(&mut scroll, 12, anchor, 500.0, 0.0, 2_000.0,));

        assert_eq!(scroll.current, 504.0);
        assert_eq!(scroll.target, 584.0);
        assert_eq!(scroll.target - scroll.current, 80.0);
        assert_eq!(scroll.velocity, 0.0);
        assert_eq!(scroll.anim_speed, 7.0);
    }

    #[test]
    fn markdown_pending_rebase_rejects_stale_revision_without_moving_scroll() {
        let mut markdown = MarkdownTabState::default();
        markdown.mode = MarkdownMode::Read;
        markdown.scroll_transition = Some(MarkdownScrollTransition {
            from: MarkdownMode::Edit,
            to: MarkdownMode::Read,
            version: 7,
            origin_scroll_y: 50.0,
            origin_sticky_lines: 0,
            origin_navigation_revision: 0,
            origin_read_width: None,
            anchor: None,
        });
        let mut scroll = crate::scroll::ScrollState::new(7.0);
        scroll.current = 60.0;
        scroll.target = 120.0;
        scroll.velocity = 18.0;
        let before = (
            scroll.current,
            scroll.target,
            scroll.velocity,
            scroll.anim_speed,
        );

        assert!(!markdown.apply_scroll_transition(
            &mut scroll,
            8,
            MarkdownSourceAnchor {
                source_range: 0..1,
                viewport_offset_y: 0.0,
            },
            300.0,
            0.0,
            1_000.0,
        ));
        assert_eq!(
            (
                scroll.current,
                scroll.target,
                scroll.velocity,
                scroll.anim_speed
            ),
            before
        );
        assert!(markdown.scroll_transition.is_none());
    }

    #[test]
    fn markdown_source_target_tracks_relative_delta_and_new_absolute_intent_resets_it() {
        let mut markdown = MarkdownTabState::default();
        markdown.mode = MarkdownMode::Read;
        markdown.mark_absolute_source_scroll_target_navigation(10..20, 0.5);
        markdown.remember_pending_absolute_scroll_target_y(100.0);

        let (_, relative_delta) = markdown
            .pending_absolute_scroll_target(MarkdownMode::Read, 64.0)
            .expect("source target is pending");
        assert_eq!(relative_delta, -36.0);

        markdown.mark_absolute_source_scroll_target_navigation(30..40, 0.5);
        markdown.remember_pending_absolute_scroll_target_y(200.0);
        let (_, relative_delta) = markdown
            .pending_absolute_scroll_target(MarkdownMode::Read, 200.0)
            .expect("replacement source target is pending");
        assert_eq!(relative_delta, 0.0);
    }

    #[test]
    fn markdown_unresolved_fifty_round_trips_do_not_drift_shared_scroll() {
        let Some(mut app) = crate::app::app_behavior_tests::test_app() else {
            return;
        };
        app.show_welcome = false;
        app.file_path = Some(std::path::PathBuf::from("/tmp/repeated.md"));
        app.file_extension = "md".to_string();
        app.editor = crate::app::app_behavior_tests::editor_with("# title\n\nparagraph\n");
        app.scroll_y.current = 123.25;
        app.scroll_y.target = 219.75;
        app.scroll_y.velocity = -17.5;
        app.scroll_y.anim_speed = 7.0;
        let before_text = app.editor.get_full_text();
        let before_version = app.editor.version;
        let before_cursor = app.editor.cursor;
        let before_selection = app.editor.selection_anchor;
        let before_scroll = (
            app.scroll_y.current,
            app.scroll_y.target,
            app.scroll_y.velocity,
            app.scroll_y.anim_speed,
        );

        for _ in 0..50 {
            app.set_markdown_mode(MarkdownMode::Read);
            assert!(app.markdown.scroll_transition.is_some());
            app.set_markdown_mode(MarkdownMode::Edit);
            assert!(app.markdown.scroll_transition.is_none());
        }

        assert_eq!(app.markdown_mode(), MarkdownMode::Edit);
        assert_eq!(
            (
                app.scroll_y.current,
                app.scroll_y.target,
                app.scroll_y.velocity,
                app.scroll_y.anim_speed,
            ),
            before_scroll
        );
        assert_eq!(app.editor.get_full_text(), before_text);
        assert_eq!(app.editor.version, before_version);
        assert_eq!(app.editor.cursor, before_cursor);
        assert_eq!(app.editor.selection_anchor, before_selection);
    }
}
