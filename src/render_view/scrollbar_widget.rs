//! Shared scrollbar widget: one geometry, one draw, one hitbox, one press/drag mapping.
//!
//! A caller describes a bar with [`Scrollbar`] (style + axis + lane + extent), then
//! - render: `Renderer::draw_scrollbar` paints it, optionally registers the lane hitbox,
//!   and returns the [`ScrollbarGeometry`] so the caller can draw extras (git diff marks);
//! - input: `Scrollbar::geometry` gives the same geometry, and
//!   `crate::app::mouse::{press_scrollbar, drag_scrollbar}` (or `ScrollbarGeometry::press_target`
//!   / `drag_target`) map the pointer to a content offset.
//!
//! Render and input build the `Scrollbar` through one caller-side function (for example
//! `crate::app::file_tree::file_tree_scrollbar`), so the thumb under the pointer is the drawn one.

use crate::renderer::Renderer;
use crate::scroll::ScrollbarThumb;
use crate::ui_system::{UiId, UiRegistry};

/// `(x, y, w, h)` in physical pixels.
pub(crate) type ScrollbarRect = (f32, f32, f32, f32);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ScrollbarAxis {
    Vertical,
    Horizontal,
}

/// Look of a scrollbar. Lengths are logical pixels (multiplied by the UI scale).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ScrollbarStyle {
    /// Thumb size across the axis; `0.0` fills the lane minus `edge_gap` on both sides.
    pub thumb_thickness: f32,
    /// Gap between the thumb and the far lane edge (right for vertical, bottom for
    /// horizontal); `None` centres the thumb across the lane.
    pub edge_gap: Option<f32>,
    /// Track inset from both lane ends along the axis (the lane stays the hitbox).
    pub track_pad: f32,
    /// Minimum thumb length along the axis.
    pub min_thumb: f32,
    /// Thumb corner radius; `None` is half the thumb thickness.
    pub radius: Option<f32>,
    /// Filled over the whole lane when set.
    pub track_color: Option<[f32; 4]>,
    pub thumb_color: [f32; 4],
}

impl ScrollbarStyle {
    /// Neutral base for presets: full-lane thumb, no track, 20 px minimum thumb.
    pub(crate) const BASE: Self = Self {
        thumb_thickness: 0.0,
        edge_gap: None,
        track_pad: 0.0,
        min_thumb: 20.0,
        radius: None,
        track_color: None,
        thumb_color: [1.0, 1.0, 1.0, 0.22],
    };
    /// Editor vertical bar: 10 px lane, 8 px accent thumb.
    pub(crate) const EDITOR_Y: Self = Self {
        edge_gap: Some(1.0),
        thumb_color: [0.7, 0.33, 0.54, 0.8],
        ..Self::BASE
    };
    /// Editor horizontal bar: 14 px lane with a theme-coloured track (set by the caller).
    pub(crate) const EDITOR_X: Self = Self {
        thumb_thickness: 6.0,
        min_thumb: 40.0,
        thumb_color: [0.7, 0.33, 0.54, 1.0],
        ..Self::BASE
    };
    pub(crate) const TERMINAL: Self = Self {
        thumb_color: [0.7, 0.33, 0.54, 0.8],
        ..Self::BASE
    };
    /// Thin white thumb of side panels (file tree).
    pub(crate) const FILE_TREE: Self = Self {
        thumb_thickness: 3.0,
        ..Self::BASE
    };
    pub(crate) const GIT_LOGS: Self = Self {
        thumb_thickness: 3.0,
        min_thumb: 10.0,
        ..Self::BASE
    };
    pub(crate) const GIT_GRAPH: Self = Self {
        thumb_thickness: 6.0,
        track_pad: 4.0,
        min_thumb: 10.0,
        ..Self::BASE
    };
    /// 12 px lane, 6 px thumb flush with the lane start.
    pub(crate) const PROBLEMS: Self = Self {
        thumb_thickness: 6.0,
        edge_gap: Some(6.0),
        thumb_color: [0.45, 0.45, 0.55, 0.5],
        ..Self::BASE
    };
    /// Hover/detail popups: white thumb, alpha comes from the caller's fade multiplier.
    pub(crate) const HOVER_POPUP: Self = Self {
        thumb_thickness: 4.0,
        track_pad: 8.0,
        thumb_color: [1.0, 1.0, 1.0, 1.0],
        ..Self::BASE
    };
    /// Markdown Reader vertical bar; thumb colour follows the theme (set by the caller).
    pub(crate) const MARKDOWN_READ: Self = Self {
        edge_gap: Some(1.0),
        ..Self::BASE
    };
}

/// What the bar scrolls. `offset` is the caller's scroll value in `[0, max_scroll]`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ScrollbarExtent {
    pub viewport: f32,
    pub content: f32,
    pub max_scroll: f32,
    pub offset: f32,
    /// `offset` counts from the end (terminal: 0 = bottom); the thumb still runs top-down.
    pub from_end: bool,
}

impl ScrollbarExtent {
    /// Thumb length is `viewport / content` of the track; `max_scroll = content - viewport`.
    pub(crate) fn new(viewport: f32, content: f32, offset: f32) -> Self {
        Self {
            viewport,
            content,
            max_scroll: (content - viewport).max(0.0),
            offset,
            from_end: false,
        }
    }

    /// For callers that only know the scroll range: `content = viewport + max_scroll`.
    pub(crate) fn with_max(viewport: f32, max_scroll: f32, offset: f32) -> Self {
        let max_scroll = max_scroll.max(0.0);
        Self::new(viewport, viewport + max_scroll, offset)
    }

    pub(crate) fn from_end(self) -> Self {
        Self {
            from_end: true,
            ..self
        }
    }
}

/// A scrollbar description; `geometry` is the only place its layout is computed.
///
/// `lane` is the bar's hitbox (and track background). Input-only callers may pass a zero
/// cross-axis size: only the along-axis span affects press and drag.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Scrollbar {
    pub style: ScrollbarStyle,
    pub axis: ScrollbarAxis,
    pub lane: ScrollbarRect,
    pub extent: ScrollbarExtent,
}

/// Resolved, pixel-rounded layout of a visible scrollbar.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ScrollbarGeometry {
    pub axis: ScrollbarAxis,
    /// Rounded lane: hitbox and track background.
    pub lane: ScrollbarRect,
    pub track_start: f32,
    pub track_len: f32,
    /// Exact thumb along the axis; press/drag math uses it so a grab never jumps.
    pub thumb: ScrollbarThumb,
    /// Painted thumb, pixel-rounded.
    pub thumb_rect: ScrollbarRect,
    pub radius: f32,
    pub max_scroll: f32,
    /// Caller's offset clamped to `[0, max_scroll]`.
    pub offset: f32,
    pub from_end: bool,
}

/// Hitbox registration for `Renderer::draw_scrollbar` / `ScrollbarGeometry::register`.
pub(crate) struct ScrollbarHit<'a> {
    pub ui: &'a mut UiRegistry,
    pub id: UiId,
    pub mx: f32,
    pub my: f32,
    /// `true` registers a blocker (arrow cursor) instead of a pointer rect.
    pub blocker: bool,
}

#[inline]
fn round_rect((x, y, w, h): ScrollbarRect) -> ScrollbarRect {
    let x0 = x.round();
    let y0 = y.round();
    (x0, y0, ((x + w).round() - x0).max(0.0), ((y + h).round() - y0).max(0.0))
}

impl Scrollbar {
    /// Geometry of the bar at `scale`; `None` when nothing overflows or inputs are invalid.
    pub(crate) fn geometry(&self, scale: f32) -> Option<ScrollbarGeometry> {
        let (x, y, w, h) = self.lane;
        if !scale.is_finite()
            || scale <= 0.0
            || !(x.is_finite() && y.is_finite() && w.is_finite() && h.is_finite())
        {
            return None;
        }
        let style = &self.style;
        let extent = self.extent;
        let lane = round_rect(self.lane);
        let (lane_start, lane_len, cross_start, cross_len) = match self.axis {
            ScrollbarAxis::Vertical => (lane.1, lane.3, lane.0, lane.2),
            ScrollbarAxis::Horizontal => (lane.0, lane.2, lane.1, lane.3),
        };
        let pad = (style.track_pad * scale).round();
        let track_start = lane_start + pad;
        let track_len = (lane_len - 2.0 * pad).max(0.0);
        let max_scroll = extent.max_scroll;
        if !max_scroll.is_finite() || max_scroll <= 0.0 || !extent.offset.is_finite() {
            return None;
        }
        let offset = extent.offset.clamp(0.0, max_scroll);
        let along = if extent.from_end {
            max_scroll - offset
        } else {
            offset
        };
        // `scrollbar_thumb` positions by `scroll / (content - viewport)`; feed it the same
        // ratio of `max_scroll` so callers with a padded range keep their thumb position.
        let thumb_scroll = along / max_scroll * (extent.content - extent.viewport);
        let thumb = crate::scroll::scrollbar_thumb(
            track_start,
            track_len,
            extent.viewport,
            extent.content,
            thumb_scroll,
            (style.min_thumb * scale).round(),
        )?;

        let track_end = track_start + track_len;
        let thumb_start = thumb.start.round().clamp(track_start, track_end);
        let thumb_end = (thumb.start + thumb.len)
            .round()
            .max(thumb_start + 1.0)
            .min(track_end);
        let thickness = if style.thumb_thickness > 0.0 {
            (style.thumb_thickness * scale).round().max(1.0)
        } else {
            cross_len - 2.0 * style.edge_gap.map_or(0.0, |gap| (gap * scale).round())
        }
        .clamp(0.0, cross_len);
        let cross0 = match style.edge_gap {
            Some(gap) => cross_start + cross_len - (gap * scale).round() - thickness,
            None => cross_start + ((cross_len - thickness) * 0.5).round(),
        };
        let thumb_rect = match self.axis {
            ScrollbarAxis::Vertical => (cross0, thumb_start, thickness, thumb_end - thumb_start),
            ScrollbarAxis::Horizontal => (thumb_start, cross0, thumb_end - thumb_start, thickness),
        };
        Some(ScrollbarGeometry {
            axis: self.axis,
            lane,
            track_start,
            track_len,
            thumb,
            thumb_rect,
            radius: style.radius.map_or(thickness * 0.5, |radius| radius * scale),
            max_scroll,
            offset,
            from_end: extent.from_end,
        })
    }
}

impl ScrollbarGeometry {
    /// Pointer coordinate along the bar's axis.
    #[inline]
    pub(crate) fn pointer(&self, x: f32, y: f32) -> f32 {
        match self.axis {
            ScrollbarAxis::Vertical => y,
            ScrollbarAxis::Horizontal => x,
        }
    }

    #[inline]
    pub(crate) fn lane_contains(&self, x: f32, y: f32) -> bool {
        crate::ui_system::point_in_rect(x, y, self.lane)
    }

    #[inline]
    pub(crate) fn on_thumb(&self, pointer: f32) -> bool {
        pointer >= self.thumb.start && pointer <= self.thumb.start + self.thumb.len
    }

    /// Content offset that puts the thumb `grab_offset` px before `pointer` (drag update).
    pub(crate) fn drag_target(&self, pointer: f32, grab_offset: f32) -> Option<f32> {
        let (_, target) = crate::scroll::scrollbar_drag_target(
            pointer,
            self.track_start,
            self.track_len,
            self.thumb,
            self.max_scroll,
            Some(grab_offset),
        )?;
        Some(if self.from_end {
            self.max_scroll - target
        } else {
            target
        })
    }

    /// Press on the lane: `(grab_offset, target)`. On the thumb the grab keeps the pointer's
    /// place and the offset stays; on the track the thumb centre jumps under the pointer.
    pub(crate) fn press_target(&self, pointer: f32) -> Option<(f32, f32)> {
        if !pointer.is_finite() {
            return None;
        }
        if self.on_thumb(pointer) {
            return Some((pointer - self.thumb.start, self.offset));
        }
        let grab_offset = self.thumb.len * 0.5;
        Some((grab_offset, self.drag_target(pointer, grab_offset)?))
    }

    /// Registers the lane as the bar's hitbox; returns whether it is hovered.
    pub(crate) fn register(&self, hit: ScrollbarHit<'_>) -> bool {
        let (x, y, w, h) = self.lane;
        if hit.blocker {
            hit.ui.register_blocker(hit.id, x, y, w, h, hit.mx, hit.my)
        } else {
            hit.ui.register_rect(hit.id, x, y, w, h, hit.mx, hit.my)
        }
    }
}

#[inline]
fn with_alpha(color: [f32; 4], alpha: f32) -> [f32; 4] {
    [color[0], color[1], color[2], color[3] * alpha]
}

impl Renderer {
    /// Paints `bar` (track when styled, then thumb) with its alpha scaled by `alpha`
    /// (fade), registers the lane when `hit` is set, and returns the geometry for extras.
    pub(crate) fn draw_scrollbar(
        &mut self,
        bar: &Scrollbar,
        scale: f32,
        alpha: f32,
        hit: Option<ScrollbarHit<'_>>,
    ) -> Option<ScrollbarGeometry> {
        let geometry = bar.geometry(scale)?;
        let alpha = if alpha.is_finite() {
            alpha.clamp(0.0, 1.0)
        } else {
            0.0
        };
        if alpha > 0.0 {
            if let Some(color) = bar.style.track_color {
                let (x, y, w, h) = geometry.lane;
                self.push_rect(x, y, w, h, with_alpha(color, alpha));
            }
            let (x, y, w, h) = geometry.thumb_rect;
            if w > 0.0 && h > 0.0 {
                self.push_rounded_rect(
                    x,
                    y,
                    w,
                    h,
                    geometry.radius,
                    with_alpha(bar.style.thumb_color, alpha),
                );
            }
        }
        if let Some(hit) = hit {
            geometry.register(hit);
        }
        Some(geometry)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vertical(lane: ScrollbarRect, extent: ScrollbarExtent) -> Scrollbar {
        Scrollbar {
            style: ScrollbarStyle::BASE,
            axis: ScrollbarAxis::Vertical,
            lane,
            extent,
        }
    }

    #[test]
    fn geometry_is_pixel_rounded_and_inside_track() {
        let bar = Scrollbar {
            style: ScrollbarStyle::GIT_GRAPH,
            ..vertical((10.3, 20.6, 10.0, 300.2), ScrollbarExtent::new(300.0, 1234.5, 77.7))
        };
        let g = bar.geometry(1.25).expect("overflowing bar");
        let (x, y, w, h) = g.thumb_rect;
        for value in [x, y, w, h, g.track_start, g.track_len] {
            assert_eq!(value, value.round());
        }
        assert!(y >= g.track_start && y + h <= g.track_start + g.track_len);
        assert!(x >= g.lane.0 && x + w <= g.lane.0 + g.lane.2);
        assert_eq!(g.track_start, g.lane.1 + 5.0);
    }

    #[test]
    fn no_overflow_or_bad_scale_has_no_geometry() {
        let bar = vertical((0.0, 0.0, 10.0, 200.0), ScrollbarExtent::new(200.0, 200.0, 0.0));
        assert!(bar.geometry(1.0).is_none());
        let bar = vertical((0.0, 0.0, 10.0, 200.0), ScrollbarExtent::new(200.0, 500.0, 0.0));
        assert!(bar.geometry(0.0).is_none());
        assert!(bar.geometry(f32::NAN).is_none());
    }

    #[test]
    fn press_on_thumb_keeps_offset_and_track_press_centres_thumb() {
        let bar = vertical((0.0, 0.0, 10.0, 200.0), ScrollbarExtent::with_max(200.0, 300.0, 100.0));
        let g = bar.geometry(1.0).expect("bar");
        assert_eq!(g.press_target(75.0), Some((35.0, 100.0)));
        let (grab, target) = g.press_target(180.0).expect("track press");
        assert_eq!(grab, 40.0);
        assert_eq!(target, 300.0);
        assert_eq!(g.drag_target(95.0, 35.0), Some(150.0));
    }

    #[test]
    fn from_end_extent_maps_back_to_callers_offset() {
        let bar = vertical(
            (0.0, 0.0, 8.0, 200.0),
            ScrollbarExtent::with_max(200.0, 300.0, 0.0).from_end(),
        );
        let g = bar.geometry(1.0).expect("bar");
        // Offset 0 from the end: thumb sits at the bottom of the track.
        assert_eq!(g.thumb.start + g.thumb.len, 200.0);
        assert_eq!(g.drag_target(0.0, 0.0), Some(300.0));
        assert_eq!(g.drag_target(200.0, g.thumb.len), Some(0.0));
    }

    #[test]
    fn padded_range_keeps_thumb_ratio_of_max_scroll() {
        let extent = ScrollbarExtent {
            max_scroll: 120.0,
            ..ScrollbarExtent::new(100.0, 200.0, 120.0)
        };
        let g = vertical((0.0, 0.0, 10.0, 100.0), extent).geometry(1.0).expect("bar");
        assert_eq!(g.thumb.start + g.thumb.len, 100.0);
        assert_eq!(g.drag_target(100.0, g.thumb.len), Some(120.0));
    }

    #[test]
    fn cross_axis_placement_follows_style() {
        let extent = ScrollbarExtent::new(100.0, 400.0, 0.0);
        let centred = Scrollbar {
            style: ScrollbarStyle::HOVER_POPUP,
            ..vertical((100.0, 0.0, 12.0, 100.0), extent)
        };
        assert_eq!(centred.geometry(1.0).expect("bar").thumb_rect.0, 104.0);
        let gap = Scrollbar {
            style: ScrollbarStyle::PROBLEMS,
            ..vertical((100.0, 0.0, 12.0, 100.0), extent)
        };
        let rect = gap.geometry(1.0).expect("bar").thumb_rect;
        assert_eq!((rect.0, rect.2), (100.0, 6.0));
        let fill = Scrollbar {
            style: ScrollbarStyle::EDITOR_Y,
            axis: ScrollbarAxis::Horizontal,
            ..vertical((0.0, 50.0, 100.0, 10.0), extent)
        };
        let rect = fill.geometry(1.0).expect("bar").thumb_rect;
        assert_eq!((rect.1, rect.3), (51.0, 8.0));
    }
}
