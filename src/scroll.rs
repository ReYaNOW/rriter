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
