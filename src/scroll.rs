#[derive(Clone, Debug)]
pub struct ScrollState {
    pub current: f32,
    pub target: f32,
    pub velocity: f32,
    pub anim_speed: f32,
    pub is_dragging: bool,
    pub drag_offset: f32,
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
        }
    }

    pub fn update(&mut self, dt: f32) -> bool {
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
        false
    }

    pub fn clamp_target(&mut self, min: f32, max: f32) {
        self.target = self.target.clamp(min, max);
    }

    pub fn clamp_current(&mut self, min: f32, max: f32) {
        self.current = self.current.clamp(min, max);
    }

    pub fn scroll_by(&mut self, delta: f32) {
        self.target += delta;
    }

    pub fn set_target(&mut self, target: f32) {
        self.target = target;
    }

    pub fn stop_anim(&mut self) {
        self.target = self.current.round();
        self.current = self.target;
        self.velocity = 0.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scroll_state_clamps_animates_and_stops_end_to_end() {
        let mut scroll = ScrollState::new(15.0);

        scroll.scroll_by(120.0);
        scroll.clamp_target(0.0, 80.0);
        assert_eq!(scroll.target, 80.0);

        assert!(scroll.update(0.016));
        assert!(scroll.current > 0.0);
        assert!(scroll.current < 80.0);

        scroll.current = 90.0;
        scroll.clamp_current(0.0, 80.0);
        assert_eq!(scroll.current, 80.0);

        scroll.current = 12.4;
        scroll.velocity = 99.0;
        scroll.stop_anim();
        assert_eq!(scroll.current, 12.0);
        assert_eq!(scroll.target, 12.0);
        assert_eq!(scroll.velocity, 0.0);
        assert!(!scroll.update(0.016));
    }
}
