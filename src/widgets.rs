use crate::renderer::Renderer;

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub enum IconType {
    Save,
    Discard,
    Cancel,
    Warning,
    CaseMatch,
    Up,
    Down,
    Close,
}

pub struct Button {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub text: String,
    pub icon: Option<IconType>,
}

impl Button {
    pub fn is_hovered(&self, mx: f32, my: f32) -> bool {
        mx >= self.x && mx <= self.x + self.w && my >= self.y && my <= self.y + self.h
    }

    pub fn render(
        &self,
        renderer: &mut Renderer,
        mx: f32,
        my: f32,
        scale: f32,
        pressed: bool,
    ) -> bool {
        let hovered = self.is_hovered(mx, my);

        let border_color = renderer.theme.sel;
        let mut bg_color = [0.22, 0.24, 0.26, 1.0];

        if hovered {
            if pressed {
                bg_color = renderer.theme.sel;
            } else {
                bg_color = [0.28, 0.30, 0.33, 1.0];
            }
        }

        let draw_x = self.x.round();
        let draw_y = self.y.round();
        let draw_w = self.w.round();
        let draw_h = self.h.round();
        let r = (4.0 * scale).round();
        let bw = (1.0 * scale).round().max(1.0);

        renderer.push_rounded_rect(draw_x, draw_y, draw_w, draw_h, r, border_color);
        renderer.push_rounded_rect(
            draw_x + bw,
            draw_y + bw,
            draw_w - bw * 2.0,
            draw_h - bw * 2.0,
            r - bw,
            bg_color,
        );

        let icon_size = (26.0 * scale).round();
        let text_scale = 1.0;
        let text_color = renderer.theme.fg;

        let icon_y = draw_y + ((draw_h - icon_size) / 2.0).round();
        let text_y = draw_y + (draw_h / 2.0).round() + (5.0 * scale).round();

        let mut content_w = renderer.measure_ui_width(&self.text, text_scale).round();
        if self.icon.is_some() {
            content_w += icon_size + (8.0 * scale).round();
        }

        let mut content_x = draw_x + ((draw_w - content_w) / 2.0).round();

        if let Some(icon_type) = self.icon {
            renderer.draw_atlas_icon(
                icon_type,
                content_x,
                icon_y,
                icon_size,
                [1.0, 1.0, 1.0, 1.0],
            );
            content_x += icon_size + (8.0 * scale).round();
        }

        renderer.draw_string_scaled(&self.text, content_x, text_y, text_color, text_scale);

        hovered
    }
}

pub struct IconButton {
    pub x: f32,
    pub y: f32,
    pub size: f32,
    pub icon: Option<IconType>,
    pub is_active: bool,
    pub icon_size: Option<f32>,
}

impl IconButton {
    pub fn is_hovered(&self, mx: f32, my: f32) -> bool {
        mx >= self.x && mx <= self.x + self.size && my >= self.y && my <= self.y + self.size
    }

    pub fn render(
        &self,
        renderer: &mut Renderer,
        mx: f32,
        my: f32,
        scale: f32,
        pressed: bool,
    ) -> bool {
        let hovered = self.is_hovered(mx, my);

        let mut bg_color = [0.0, 0.0, 0.0, 0.0];
        let mut draw_bg = false;

        if self.is_active {
            bg_color = renderer.theme.sel;
            draw_bg = true;
        } else if hovered {
            if pressed {
                bg_color = renderer.theme.sel;
            } else {
                bg_color = [0.26, 0.28, 0.30, 1.0];
            }
            draw_bg = true;
        }

        let draw_x = self.x.round();
        let draw_y = self.y.round();
        let draw_s = self.size.round();
        let r = (4.0 * scale).round();

        if draw_bg {
            renderer.push_rounded_rect(draw_x, draw_y, draw_s, draw_s, r, bg_color);
        }

        let icon_render_size = self.icon_size.unwrap_or((20.0 * scale).round());
        let offset = ((draw_s - icon_render_size) / 2.0).round();

        if let Some(icon_type) = self.icon {
            let icon_color = if self.is_active {
                [1.0, 1.0, 1.0, 1.0]
            } else {
                renderer.theme.fg
            };
            renderer.draw_atlas_icon(
                icon_type,
                draw_x + offset,
                draw_y + offset,
                icon_render_size,
                icon_color,
            );
        }

        hovered
    }
}

pub fn get_welcome_buttons(
    _width: f32,
    x: f32,
    y: f32,
    scale: f32,
    renderer: &mut Renderer,
) -> (Button, Button) {
    let bh = 40.0 * scale;
    let gap = 15.0 * scale;
    let icon_sz = 26.0 * scale;
    let padding = 32.0 * scale;

    let w_new = renderer.measure_ui_width("Новый файл", 1.0) + icon_sz + padding;
    let w_open = renderer.measure_ui_width("Открыть файл", 1.0) + icon_sz + padding;

    let btn_new = Button {
        x,
        y,
        w: w_new,
        h: bh,
        text: "Новый файл".to_string(),
        icon: None,
    };

    let btn_open = Button {
        x: x + w_new + gap,
        y,
        w: w_open,
        h: bh,
        text: "Открыть файл".to_string(),
        icon: None,
    };

    (btn_new, btn_open)
}

pub fn get_dialog_buttons(
    box_x: f32,
    box_y: f32,
    box_w: f32,
    box_h: f32,
    scale: f32,
    renderer: &mut Renderer,
) -> (Button, Button, Button) {
    let bh = (36.0 * scale).round();
    let gap = (12.0 * scale).round();
    let icon_sz = (26.0 * scale).round();
    let padding = (40.0 * scale).round();

    let w_save = renderer.measure_ui_width("Сохранить", 1.0).round() + icon_sz + padding;
    let w_discard = renderer.measure_ui_width("Отклонить", 1.0).round() + icon_sz + padding;
    let w_cancel = renderer.measure_ui_width("Отмена", 1.0).round() + icon_sz + padding;

    let total_w = w_save + w_discard + w_cancel + gap * 2.0;

    let mut current_x = box_x + (box_w - total_w) / 2.0;
    let offset_y = 15.0 * scale;
    let y = box_y + box_h - bh - offset_y;

    let btn_save = Button {
        x: current_x,
        y,
        w: w_save,
        h: bh,
        text: "Сохранить".to_string(),
        icon: Some(IconType::Save),
    };
    current_x += w_save + gap;

    let btn_discard = Button {
        x: current_x,
        y,
        w: w_discard,
        h: bh,
        text: "Отклонить".to_string(),
        icon: Some(IconType::Discard),
    };
    current_x += w_discard + gap;

    let btn_cancel = Button {
        x: current_x,
        y,
        w: w_cancel,
        h: bh,
        text: "Отмена".to_string(),
        icon: Some(IconType::Cancel),
    };

    (btn_save, btn_discard, btn_cancel)
}

pub fn get_faq_button(
    box_x: f32,
    box_y: f32,
    box_w: f32,
    box_h: f32,
    scale: f32,
    renderer: &mut Renderer,
) -> Button {
    let bh = (36.0 * scale).round();
    let w_ok = renderer.measure_ui_width("ОК", 1.0).round() + (40.0 * scale).round();
    let x = box_x + (box_w - w_ok) / 2.0;
    let offset_y = 20.0 * scale;
    let y = box_y + box_h - bh - offset_y;
    Button {
        x,
        y,
        w: w_ok,
        h: bh,
        text: "ОК".to_string(),
        icon: None,
    }
}
