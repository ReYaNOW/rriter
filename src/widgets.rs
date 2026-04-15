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
    Plus,
    Terminal,
    Explorer,
    Problems,
    LspServers,
    Copy,
    Check,
}

pub struct Button {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub text: String,
    pub icon: Option<IconType>,
    pub text_scale: f32,
    pub icon_size: f32,
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
        let x = self.x.round();
        let y = self.y.round();
        let w = self.w.round();
        let h = self.h.round();

        let hovered = mx >= x && mx <= x + w && my >= y && my <= y + h;

        let border_color = renderer.theme.sel;
        let mut bg_color = [0.22, 0.24, 0.26, 1.0];

        if hovered {
            if pressed {
                bg_color = renderer.theme.sel;
            } else {
                bg_color = [0.28, 0.30, 0.33, 1.0];
            }
        }

        let r = 4.0 * scale;
        let bw = (1.0 * scale).round().max(1.0);
        renderer.push_rounded_rect(x, y, w, h, r, border_color);
        renderer.push_rounded_rect(
            x + bw,
            y + bw,
            w - bw * 2.0,
            h - bw * 2.0,
            (r - bw).max(1.0),
            bg_color,
        );

        let icon_size = self.icon_size;
        let text_scale = self.text_scale;
        let text_color = renderer.theme.fg;

        let icon_y = y + (h - icon_size) / 2.0;
        let text_y = y + h / 2.0 + 5.0 * scale;

        let mut content_w = renderer.measure_ui_width(&self.text, text_scale);
        if self.icon.is_some() {
            content_w += icon_size + 8.0 * scale;
        }

        let mut content_x = x + (w - content_w) / 2.0;

        if let Some(icon_type) = self.icon {
            renderer.draw_atlas_icon(
                icon_type,
                content_x,
                icon_y,
                icon_size,
                [1.0, 1.0, 1.0, 1.0],
            );
            content_x += icon_size + 8.0 * scale;
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
    pub active_square_width: Option<f32>,
}

impl IconButton {
    pub fn is_hovered(&self, mx: f32, my: f32) -> bool {
        if let Some(sq_w) = self.active_square_width {
            let icon_center = self.y + self.size / 2.0;
            let sq_y = (icon_center - sq_w / 2.0).round();
            mx >= 0.0 && mx <= sq_w && my >= sq_y && my <= sq_y + sq_w
        } else {
            mx >= self.x && mx <= self.x + self.size && my >= self.y && my <= self.y + self.size
        }
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

        if self.is_active {
            if let Some(sq_w) = self.active_square_width {
                // Центрируем квадрат точно по центру иконки
                let icon_center = self.y + self.size / 2.0;
                let sq_h = sq_w;
                let sq_y = (icon_center - sq_h / 2.0).round();
                let bg_color = [0.35, 0.26, 0.48, 1.0];
                renderer.push_rect(0.0, sq_y, sq_w, sq_h, bg_color);

                // Вертикальная акцентная полоска у левого края
                let stripe_w = (3.0 * scale).max(2.0);
                let stripe_color = [0.60, 0.35, 0.85, 1.0];
                renderer.push_rect(0.0, sq_y, stripe_w, sq_h, stripe_color);

                let icon_render_size = self.icon_size.unwrap_or(20.0 * scale);
                let offset = (self.size - icon_render_size) / 2.0;
                if let Some(icon_type) = self.icon {
                    renderer.draw_atlas_icon(
                        icon_type,
                        self.x + offset,
                        self.y + offset,
                        icon_render_size,
                        [1.0, 1.0, 1.0, 1.0],
                    );
                }
                return false;
            }
        }

        let mut bg_color = [0.0, 0.0, 0.0, 0.0];
        let mut draw_bg = false;
        let mut radius = 4.0 * scale;

        if self.is_active {
            bg_color = renderer.theme.sel;
            draw_bg = true;
        } else if hovered {
            if pressed {
                bg_color = renderer.theme.sel;
                radius = 4.0 * scale;
            } else {
                bg_color = [0.26, 0.28, 0.30, 1.0];
                radius = self.size / 2.0;
            }
            draw_bg = true;
        }

        if draw_bg {
            if let Some(sq_w) = self.active_square_width {
                let icon_center = self.y + self.size / 2.0;
                let sq_y = (icon_center - sq_w / 2.0).round();
                renderer.push_rect(0.0, sq_y, sq_w, sq_w, bg_color);
            } else {
                renderer.push_rounded_rect(self.x, self.y, self.size, self.size, radius, bg_color);
            }
        }

        let icon_render_size = self.icon_size.unwrap_or(20.0 * scale);
        let offset = (self.size - icon_render_size) / 2.0;

        if let Some(icon_type) = self.icon {
            let icon_color = if self.is_active {
                [1.0, 1.0, 1.0, 1.0]
            } else {
                renderer.theme.fg
            };
            renderer.draw_atlas_icon(
                icon_type,
                self.x + offset,
                self.y + offset,
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
) -> (Button, Button, Button) {
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
        text_scale: 1.0,
        icon_size: icon_sz,
    };

    let btn_open = Button {
        x: x + w_new + gap,
        y,
        w: w_open,
        h: bh,
        text: "Открыть файл".to_string(),
        icon: None,
        text_scale: 1.0,
        icon_size: icon_sz,
    };

    let w_ide = renderer.measure_ui_width("Режим IDE", 1.0) + icon_sz + padding;

    let btn_ide = Button {
        x: x + w_new + gap + w_open + gap,
        y,
        w: w_ide,
        h: bh,
        text: "Режим IDE".to_string(),
        icon: None,
        text_scale: 1.0,
        icon_size: icon_sz,
    };

    (btn_new, btn_open, btn_ide)
}

pub fn get_dialog_buttons(
    box_x: f32,
    box_y: f32,
    box_w: f32,
    box_h: f32,
    scale: f32,
    renderer: &mut Renderer,
) -> (Button, Button, Button) {
    let bh = 44.0 * scale;
    let gap = 14.0 * scale;
    let icon_sz_calc = 24.0 * scale;
    let text_scale_calc = 1.0;
    let padding = 12.0 * scale + 30.0 * scale;

    // Считаем габариты со старым масштабом 1.0, чтобы размер кнопок не менялся
    let w_save = renderer.measure_ui_width("Сохранить", text_scale_calc) + icon_sz_calc + padding;
    let w_discard =
        renderer.measure_ui_width("Отклонить", text_scale_calc) + icon_sz_calc + padding;
    let w_cancel = renderer.measure_ui_width("Отмена", text_scale_calc) + icon_sz_calc + padding;

    let total_w = w_save + w_discard + w_cancel + gap * 2.0;

    let mut current_x = box_x + (box_w - total_w) / 2.0;
    let y = box_y + box_h - bh - 22.0 * scale;

    // Для отрисовки передаем скорректированные размеры
    let render_icon_sz = 28.0 * scale;
    let render_text_scale = 1.04;

    let btn_save = Button {
        x: current_x,
        y,
        w: w_save,
        h: bh,
        text: "Сохранить".to_string(),
        icon: Some(IconType::Save),
        text_scale: render_text_scale,
        icon_size: render_icon_sz,
    };
    current_x += w_save + gap;

    let btn_discard = Button {
        x: current_x,
        y,
        w: w_discard,
        h: bh,
        text: "Отклонить".to_string(),
        icon: Some(IconType::Discard),
        text_scale: render_text_scale,
        icon_size: render_icon_sz,
    };
    current_x += w_discard + gap;

    let btn_cancel = Button {
        x: current_x,
        y,
        w: w_cancel,
        h: bh,
        text: "Отмена".to_string(),
        icon: Some(IconType::Cancel),
        text_scale: render_text_scale,
        icon_size: render_icon_sz,
    };

    (btn_save, btn_discard, btn_cancel)
}
