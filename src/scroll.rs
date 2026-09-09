#[derive(Clone, Debug)]
pub struct ScrollState {
    pub current: f32,
    pub target: f32,
    pub velocity: f32,
    pub anim_speed: f32,
    pub is_dragging: bool,
    pub drag_offset: f32,
    deferred_current_rebase: Option<DeferredCurrentRebase>,
}

#[derive(Clone, Copy, Debug)]
struct DeferredCurrentRebase {
    destination_current: f32,
    applied: bool,
}

impl ScrollState {
    pub fn new(anim_speed: f32) -> Self {
        Self {
            current: 0.0,
            target: 0.0,
            velocity: 0.0,
            anim_speed,
            is_dragging: false,
            drag_offset: 0.0,
            deferred_current_rebase: None,
        }
    }

    pub fn update(&mut self, dt: f32) -> bool {
        if self.sanitize_non_finite() {
            return true;
        }
        if !dt.is_finite() || dt <= 0.0 {
            return false;
        }
        let rebased = self.apply_deferred_current_rebase();
        let diff = self.target - self.current;
        let abs_diff = diff.abs();
        if abs_diff > 0.0 {
            let target_v = if abs_diff > 15.0 {
                diff * self.anim_speed
            } else {
                diff.signum() * abs_diff.sqrt() * (15.0_f32.sqrt() * self.anim_speed)
            };
            let v_factor = 1.0 - (-self.anim_speed * 4.0 * dt).exp();
            self.velocity += (target_v - self.velocity) * v_factor;
            let step = self.velocity * dt;

            if step.abs() >= abs_diff || diff.signum() != (diff - step).signum() || abs_diff < 0.01
            {
                self.current = self.target;
                self.velocity = 0.0;
            } else {
                self.current += step;
            }
            return true;
        }
        rebased
    }

    pub fn is_settled(&self) -> bool {
        !self.is_dragging
            && self.current.is_finite()
            && self.target.is_finite()
            && self.current == self.target
    }

    pub fn clamp_target(&mut self, min: f32, max: f32) {
        let (min, max) = finite_bounds(min, max);
        if !self.target.is_finite() {
            self.target = min;
            self.velocity = 0.0;
        }
        self.target = self.target.clamp(min, max);
        if self.target == self.current {
            self.velocity = 0.0;
        }
    }

    pub fn clamp_current(&mut self, min: f32, max: f32) {
        let (min, max) = finite_bounds(min, max);
        if !self.current.is_finite() {
            self.current = min;
            self.velocity = 0.0;
        }
        self.current = self.current.clamp(min, max);
        if self.current == self.target {
            self.velocity = 0.0;
        }
    }

    pub fn scroll_by(&mut self, delta: f32) {
        if delta.is_finite() {
            if !self.target.is_finite() {
                self.target = self.current.max(0.0);
            }
            self.target += delta;
        }
    }

    pub fn set_target(&mut self, target: f32) {
        if target.is_finite() {
            self.target = target;
        }
    }

    /// Schedules a current-only coordinate rebase for the next physics tick or
    /// transition resolve. The public `current` remains in the origin surface
    /// until one of those two events, so an immediate inverse mode toggle can
    /// still cancel the transition without decoding origin pixels as destination
    /// pixels.
    pub(crate) fn defer_current_rebase_preserving_target(&mut self, new_current: f32) -> bool {
        if !new_current.is_finite() || !self.current.is_finite() || !self.target.is_finite() {
            return false;
        }
        self.deferred_current_rebase = Some(DeferredCurrentRebase {
            destination_current: new_current,
            applied: false,
        });
        self.end_drag();
        true
    }

    fn apply_deferred_current_rebase(&mut self) -> bool {
        let Some(mut deferred) = self.deferred_current_rebase else {
            return false;
        };
        if deferred.applied {
            return false;
        }
        self.current = deferred.destination_current;
        deferred.applied = true;
        self.deferred_current_rebase = Some(deferred);
        self.end_drag();
        true
    }

    pub(crate) fn complete_deferred_current_rebase(&mut self) -> bool {
        let Some(deferred) = self.deferred_current_rebase else {
            return false;
        };
        if !deferred.applied {
            self.current = deferred.destination_current;
        }
        self.deferred_current_rebase = None;
        self.end_drag();
        true
    }

    pub(crate) fn deferred_current_rebase_applied(&self) -> Option<bool> {
        self.deferred_current_rebase
            .map(|deferred| deferred.applied)
    }

    /// Reprojects a current that already consumed its deferred rebase while
    /// keeping the applied ownership marker alive for the destination surface.
    ///
    /// Absolute destination navigation already owns `target`, so callers can
    /// preserve it. Otherwise the same coordinate delta is applied to target
    /// so unfinished physical motion remains unchanged.
    pub(crate) fn reproject_applied_deferred_current(
        &mut self,
        new_current: f32,
        preserve_destination_target: bool,
    ) -> bool {
        let Some(mut deferred) = self.deferred_current_rebase else {
            return false;
        };
        if !deferred.applied
            || !new_current.is_finite()
            || !self.current.is_finite()
            || !self.target.is_finite()
        {
            return false;
        }

        if !preserve_destination_target {
            let delta = new_current - self.current;
            let new_target = self.target + delta;
            if !delta.is_finite() || !new_target.is_finite() {
                return false;
            }
            self.target = new_target;
        }

        self.current = new_current;
        deferred.destination_current = new_current;
        self.deferred_current_rebase = Some(deferred);
        self.end_drag();
        true
    }

    /// Cancels an unapplied destination-current rebase while translating the
    /// absolute destination target back into the origin coordinate system by
    /// the same local delta. Used only when the user reverses the mode toggle
    /// before a destination frame/tick has consumed the rebase.
    pub(crate) fn cancel_unapplied_deferred_current_rebase(&mut self) -> bool {
        let Some(deferred) = self.deferred_current_rebase else {
            return false;
        };
        if deferred.applied || !self.target.is_finite() || !self.current.is_finite() {
            return false;
        }
        let reverse_delta = self.current - deferred.destination_current;
        let origin_target = self.target + reverse_delta;
        if !reverse_delta.is_finite() || !origin_target.is_finite() {
            return false;
        }
        self.target = origin_target;
        self.deferred_current_rebase = None;
        self.end_drag();
        true
    }

    pub(crate) fn clear_deferred_current_rebase(&mut self) {
        self.deferred_current_rebase = None;
    }

    /// Rebases the scroll coordinate system without changing the unfinished motion.
    ///
    /// The same delta is applied to `current` and `target`; velocity and animation
    /// speed are preserved. A scrollbar drag belongs to the old surface geometry,
    /// so only that pointer capture is released.
    pub(crate) fn rebase_current_preserving_motion(&mut self, new_current: f32) -> bool {
        if !new_current.is_finite() || !self.current.is_finite() || !self.target.is_finite() {
            return false;
        }
        let delta = new_current - self.current;
        let new_target = self.target + delta;
        if !delta.is_finite() || !new_target.is_finite() {
            return false;
        }
        self.current = new_current;
        self.target = new_target;
        self.deferred_current_rebase = None;
        self.end_drag();
        true
    }

    /// Rebases only the current position after an absolute destination target was
    /// already selected in the new coordinate system.
    pub(crate) fn rebase_current_preserving_target(&mut self, new_current: f32) -> bool {
        if !new_current.is_finite() || !self.target.is_finite() {
            return false;
        }
        self.current = new_current;
        self.deferred_current_rebase = None;
        self.end_drag();
        true
    }

    pub fn stop_anim(&mut self) {
        self.sanitize_non_finite();
        let applied_rebase = self
            .deferred_current_rebase
            .filter(|deferred| deferred.applied);
        self.target = self.current.round();
        self.current = self.target;
        self.velocity = 0.0;
        // Stopping cancels motion, but an already-applied deferred rebase is
        // still the ownership marker for `current` until the destination
        // transition resolves. An unapplied rebase is ordinary pending motion
        // and is intentionally cancelled by the stop.
        self.deferred_current_rebase = applied_rebase;
    }

    pub fn jump_to(&mut self, target: f32) {
        if !target.is_finite() {
            return;
        }
        self.current = target;
        self.target = target;
        self.velocity = 0.0;
        self.deferred_current_rebase = None;
        self.is_dragging = false;
        self.drag_offset = 0.0;
    }

    pub fn animate_to(&mut self, target: f32) {
        if !target.is_finite() {
            return;
        }
        self.target = target;
        self.velocity = 0.0;
        self.is_dragging = false;
        self.drag_offset = 0.0;
    }

    pub fn end_drag(&mut self) {
        self.is_dragging = false;
        self.drag_offset = 0.0;
    }

    pub fn reset(&mut self) {
        self.current = 0.0;
        self.target = 0.0;
        self.velocity = 0.0;
        self.deferred_current_rebase = None;
        self.is_dragging = false;
        self.drag_offset = 0.0;
    }

    fn sanitize_non_finite(&mut self) -> bool {
        let mut changed = false;
        if !self.current.is_finite() {
            self.current = 0.0;
            changed = true;
        }
        if !self.target.is_finite() {
            self.target = self.current;
            changed = true;
        }
        if !self.velocity.is_finite() {
            self.velocity = 0.0;
            changed = true;
        }
        if !self.anim_speed.is_finite() || self.anim_speed <= 0.0 {
            self.anim_speed = 15.0;
            changed = true;
        }
        if !self.drag_offset.is_finite() {
            self.drag_offset = 0.0;
            changed = true;
        }
        if self
            .deferred_current_rebase
            .is_some_and(|deferred| !deferred.destination_current.is_finite())
        {
            self.deferred_current_rebase = None;
            changed = true;
        }
        if changed {
            self.is_dragging = false;
            self.drag_offset = 0.0;
        }
        changed
    }
}

fn finite_bounds(min: f32, max: f32) -> (f32, f32) {
    let min = if min.is_finite() { min } else { 0.0 };
    let max = if max.is_finite() { max.max(min) } else { min };
    (min, max)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ScrollbarThumb {
    pub start: f32,
    pub len: f32,
}

pub(crate) fn scrollbar_thumb(
    track_start: f32,
    track_len: f32,
    viewport_len: f32,
    content_len: f32,
    current_scroll: f32,
    min_thumb_len: f32,
) -> Option<ScrollbarThumb> {
    if !track_start.is_finite()
        || !track_len.is_finite()
        || !viewport_len.is_finite()
        || !content_len.is_finite()
        || !current_scroll.is_finite()
        || !min_thumb_len.is_finite()
        || track_len <= 0.0
        || viewport_len <= 0.0
        || content_len <= viewport_len
    {
        return None;
    }
    let max_scroll = content_len - viewport_len;
    let len = (viewport_len / content_len * track_len)
        .max(min_thumb_len)
        .min(track_len);
    let ratio = (current_scroll / max_scroll).clamp(0.0, 1.0);
    Some(ScrollbarThumb {
        start: track_start + ratio * (track_len - len),
        len,
    })
}

pub(crate) fn scrollbar_drag_target(
    pointer: f32,
    track_start: f32,
    track_len: f32,
    thumb: ScrollbarThumb,
    max_scroll: f32,
    drag_offset: Option<f32>,
) -> Option<(f32, f32)> {
    if !pointer.is_finite()
        || !track_start.is_finite()
        || !track_len.is_finite()
        || !thumb.start.is_finite()
        || !thumb.len.is_finite()
        || !max_scroll.is_finite()
        || drag_offset.is_some_and(|offset| !offset.is_finite())
        || track_len <= 0.0
        || max_scroll <= 0.0
        || thumb.len >= track_len
    {
        return None;
    }
    let offset = drag_offset.unwrap_or_else(|| {
        if pointer >= thumb.start && pointer <= thumb.start + thumb.len {
            pointer - thumb.start
        } else {
            thumb.len * 0.5
        }
    });
    let ratio = (pointer - track_start - offset) / (track_len - thumb.len).max(0.0001);
    Some((offset, (ratio * max_scroll).clamp(0.0, max_scroll)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rebase_preserves_remaining_motion_velocity_and_speed() {
        let mut scroll = ScrollState::new(7.0);
        scroll.current = 120.5;
        scroll.target = 181.25;
        scroll.velocity = -42.0;
        scroll.is_dragging = true;
        scroll.drag_offset = 9.0;

        assert!(scroll.rebase_current_preserving_motion(512.75));

        assert_eq!(scroll.current, 512.75);
        assert_eq!(scroll.target - scroll.current, 60.75);
        assert_eq!(scroll.velocity, -42.0);
        assert_eq!(scroll.anim_speed, 7.0);
        assert!(!scroll.is_dragging);
        assert_eq!(scroll.drag_offset, 0.0);
    }

    #[test]
    fn rebase_rejects_non_finite_coordinates_without_mutation() {
        let mut scroll = ScrollState::new(7.0);
        scroll.current = 12.0;
        scroll.target = 30.0;
        scroll.velocity = 4.0;

        assert!(!scroll.rebase_current_preserving_motion(f32::NAN));
        assert_eq!(scroll.current, 12.0);
        assert_eq!(scroll.target, 30.0);
        assert_eq!(scroll.velocity, 4.0);
    }

    #[test]
    fn ending_drag_preserves_scroll_position_and_clears_pointer_offset() {
        let mut scroll = ScrollState::new(7.0);
        scroll.current = 12.0;
        scroll.target = 18.0;
        scroll.is_dragging = true;
        scroll.drag_offset = 5.0;

        scroll.end_drag();

        assert_eq!(scroll.current, 12.0);
        assert_eq!(scroll.target, 18.0);
        assert!(!scroll.is_dragging);
        assert_eq!(scroll.drag_offset, 0.0);
    }

    #[test]
    fn scroll_state_clamps_animates_and_stops_end_to_end() {
        let mut scroll = ScrollState::new(15.0);

        scroll.scroll_by(120.0);
        scroll.clamp_target(0.0, 80.0);
        assert_eq!(scroll.target, 80.0);

        assert!(scroll.update(0.016));
        assert!(scroll.current > 0.0);
        assert!(scroll.current < 80.0);
        assert!(!scroll.is_settled());

        scroll.current = 90.0;
        scroll.clamp_current(0.0, 80.0);
        assert_eq!(scroll.current, 80.0);

        scroll.current = 12.4;
        scroll.velocity = 99.0;
        scroll.stop_anim();
        assert_eq!(scroll.current, 12.0);
        assert_eq!(scroll.target, 12.0);
        assert_eq!(scroll.velocity, 0.0);
        assert!(scroll.is_settled());

        scroll.current = 8.0;
        scroll.target = 12.0;
        scroll.velocity = 3.0;
        scroll.is_dragging = true;
        scroll.drag_offset = 4.0;
        scroll.reset();
        assert_eq!(scroll.current, 0.0);
        assert_eq!(scroll.target, 0.0);
        assert_eq!(scroll.velocity, 0.0);
        assert!(!scroll.is_dragging);
        assert_eq!(scroll.drag_offset, 0.0);
        assert!(scroll.is_settled());

        scroll.is_dragging = true;
        assert!(!scroll.is_settled());
        assert!(!scroll.update(0.016));

        scroll.drag_offset = 3.0;
        scroll.jump_to(24.0);
        assert_eq!(scroll.current, 24.0);
        assert_eq!(scroll.target, 24.0);
        assert_eq!(scroll.velocity, 0.0);
        assert!(!scroll.is_dragging);
        assert_eq!(scroll.drag_offset, 0.0);

        scroll.is_dragging = true;
        scroll.drag_offset = 5.0;
        scroll.velocity = 8.0;
        scroll.animate_to(48.0);
        assert_eq!(scroll.current, 24.0);
        assert_eq!(scroll.target, 48.0);
        assert_eq!(scroll.velocity, 0.0);
        assert!(!scroll.is_dragging);
        assert_eq!(scroll.drag_offset, 0.0);

        scroll.is_dragging = true;
        scroll.drag_offset = f32::NAN;
        assert!(scroll.update(0.016));
        assert!(!scroll.is_dragging);
        assert_eq!(scroll.drag_offset, 0.0);
    }

    #[test]
    fn stop_anim_preserves_applied_rebase_ownership_but_cancels_unapplied_rebase() {
        let mut scroll = ScrollState::new(7.0);
        scroll.current = 100.0;
        scroll.target = 180.0;

        assert!(scroll.defer_current_rebase_preserving_target(50.0));
        scroll.stop_anim();
        assert_eq!(scroll.deferred_current_rebase_applied(), None);

        scroll.current = 100.0;
        scroll.target = 180.0;
        assert!(scroll.defer_current_rebase_preserving_target(50.0));
        assert!(scroll.apply_deferred_current_rebase());
        assert_eq!(scroll.deferred_current_rebase_applied(), Some(true));

        scroll.stop_anim();
        assert_eq!(scroll.current, 50.0);
        assert_eq!(scroll.target, 50.0);
        assert_eq!(scroll.velocity, 0.0);
        assert_eq!(scroll.deferred_current_rebase_applied(), Some(true));
        assert!(scroll.complete_deferred_current_rebase());
        assert_eq!(scroll.deferred_current_rebase_applied(), None);
        assert_eq!(scroll.current, 50.0);
        assert_eq!(scroll.target, 50.0);
    }

    #[test]
    fn scrollbar_geometry_reuses_same_ratio_for_both_axes() {
        let thumb =
            scrollbar_thumb(10.0, 200.0, 100.0, 400.0, 150.0, 20.0).expect("scrollbar visible");
        assert_eq!(thumb.len, 50.0);
        assert_eq!(thumb.start, 85.0);

        let (_, target) =
            scrollbar_drag_target(160.0, 10.0, 200.0, thumb, 300.0, None).expect("drag target");
        assert!(target > 150.0);
        assert!(target <= 300.0);

        assert!(scrollbar_thumb(0.0, 100.0, 100.0, 100.0, 0.0, 20.0).is_none());
    }

    #[test]
    fn scrollbar_thumb_reaches_track_end_for_subpixel_overflow() {
        let thumb = scrollbar_thumb(0.0, 100.0, 100.0, 100.5, 0.5, 20.0)
            .expect("subpixel overflow is still scrollable");

        assert!((thumb.start + thumb.len - 100.0).abs() < f32::EPSILON);
    }

    #[test]
    fn clamping_a_settled_scroll_clears_stale_velocity() {
        let mut scroll = ScrollState::new(15.0);
        scroll.current = 50.0;
        scroll.target = 100.0;
        scroll.velocity = 120.0;

        scroll.clamp_target(0.0, 50.0);
        assert_eq!(scroll.target, 50.0);
        assert_eq!(scroll.velocity, 0.0);

        scroll.scroll_by(-10.0);
        assert!(scroll.update(0.016));
        assert!(scroll.current < 50.0);

        scroll.current = 40.0;
        scroll.target = 30.0;
        scroll.velocity = -80.0;
        scroll.clamp_current(30.0, 30.0);
        assert_eq!(scroll.current, 30.0);
        assert_eq!(scroll.velocity, 0.0);
    }

    #[test]
    fn bug_37_non_finite_scroll_state_recovers_without_poisoning_future_frames() {
        let mut scroll = ScrollState::new(15.0);
        scroll.current = f32::NAN;
        scroll.target = f32::INFINITY;
        scroll.velocity = f32::NEG_INFINITY;
        scroll.is_dragging = true;
        scroll.drag_offset = 7.0;

        assert!(scroll.update(0.016));
        assert_eq!(scroll.current, 0.0);
        assert_eq!(scroll.target, 0.0);
        assert_eq!(scroll.velocity, 0.0);
        assert_eq!(scroll.drag_offset, 0.0);
        assert!(!scroll.is_dragging);
        assert!(scroll.is_settled());
    }

    #[test]
    fn bug_37_scrollbar_helpers_reject_zero_over_zero_and_non_finite_inputs() {
        assert!(scrollbar_thumb(0.0, 0.0, 0.0, 0.0, 0.0, 20.0).is_none());
        assert!(scrollbar_thumb(0.0, 100.0, 50.0, f32::NAN, 0.0, 20.0).is_none());
        assert!(
            scrollbar_drag_target(
                f32::NAN,
                0.0,
                100.0,
                ScrollbarThumb {
                    start: 0.0,
                    len: 20.0
                },
                100.0,
                None,
            )
            .is_none()
        );
    }
}
