use crate::renderer::Renderer;

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub enum IconType {
    Save,
    Discard,
    Cancel,
    Warning,
    Error,
    CaseMatch,
    Up,
    Down,
    Close,
    Plus,
    GitPlus,
    GitMinus,
    Terminal,
    Explorer,
    Git,
    Branch,
    Problems,
    LspServers,
    Copy,
    Check,
    Rollback,
    Reload,
    Person,
    Time,
    GithubDark,
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

    #[cfg_attr(coverage_nightly, coverage(off))]
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
        renderer.push_rounded_rect_border(x, y, w, h, r, bw, border_color, bg_color);

        let icon_size = self.icon_size;
        let text_scale = self.text_scale;
        let text_color = renderer.theme.fg;

        let icon_y = y + (h - icon_size) / 2.0;
        let text_y = y + h / 2.0 + 5.0 * scale;

        let text_w = renderer.measure_ui_width(&self.text, text_scale);
        let mut content_w = text_w;
        if self.icon.is_some() {
            content_w += icon_size;
            if !self.text.is_empty() {
                content_w += 8.0 * scale;
            }
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
            content_x += icon_size;
            if !self.text.is_empty() {
                content_x += 8.0 * scale;
            }
        }

        if !self.text.is_empty() {
            renderer.draw_string_scaled(&self.text, content_x, text_y, text_color, text_scale);
        }

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
    pub custom_color: Option<[f32; 4]>,
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

    #[cfg_attr(coverage_nightly, coverage(off))]
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
                    let icon_col = self.custom_color.unwrap_or([1.0, 1.0, 1.0, 1.0]);
                    renderer.draw_atlas_icon(
                        icon_type,
                        (self.x + offset).round(),
                        (self.y + offset).round(),
                        icon_render_size,
                        icon_col,
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
            let icon_color = if let Some(color) = self.custom_color {
                color
            } else if self.is_active {
                [1.0, 1.0, 1.0, 1.0]
            } else {
                renderer.theme.fg
            };
            renderer.draw_atlas_icon(
                icon_type,
                (self.x + offset).round(),
                (self.y + offset).round(),
                icon_render_size,
                icon_color,
            );
        }

        hovered
    }
}

#[cfg_attr(coverage_nightly, coverage(off))]
pub fn get_welcome_buttons(
    _width: f32,
    x: f32,
    y: f32,
    scale: f32,
    renderer: &mut Renderer,
) -> (Button, Button, Button) {
    let w_new_text = renderer.measure_ui_width("Новый файл", 1.0);
    let w_open_text = renderer.measure_ui_width("Открыть файл", 1.0);
    let w_ide_text = renderer.measure_ui_width("Режим IDE", 1.0);
    welcome_buttons_from_widths(x, y, scale, w_new_text, w_open_text, w_ide_text)
}

fn welcome_buttons_from_widths(
    x: f32,
    y: f32,
    scale: f32,
    w_new_text: f32,
    w_open_text: f32,
    w_ide_text: f32,
) -> (Button, Button, Button) {
    let bh = 40.0 * scale;
    let gap = 15.0 * scale;
    let icon_sz = 26.0 * scale;
    let padding = 32.0 * scale;

    let w_new = w_new_text + icon_sz + padding;
    let w_open = w_open_text + icon_sz + padding;

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

    let w_ide = w_ide_text + icon_sz + padding;

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

#[cfg_attr(coverage_nightly, coverage(off))]
pub fn get_dialog_buttons(
    box_x: f32,
    box_y: f32,
    box_w: f32,
    box_h: f32,
    scale: f32,
    renderer: &mut Renderer,
) -> (Button, Button, Button) {
    let w_save_text = renderer.measure_ui_width("Сохранить", 1.0);
    let w_discard_text = renderer.measure_ui_width("Отклонить", 1.0);
    let w_cancel_text = renderer.measure_ui_width("Отмена", 1.0);
    dialog_buttons_from_widths(
        box_x,
        box_y,
        box_w,
        box_h,
        scale,
        w_save_text,
        w_discard_text,
        w_cancel_text,
    )
}

fn dialog_buttons_from_widths(
    box_x: f32,
    box_y: f32,
    box_w: f32,
    box_h: f32,
    scale: f32,
    w_save_text: f32,
    w_discard_text: f32,
    w_cancel_text: f32,
) -> (Button, Button, Button) {
    let bh = 44.0 * scale;
    let gap = 14.0 * scale;
    let icon_sz_calc = 24.0 * scale;
    let text_scale_calc = 1.0;
    let padding = 12.0 * scale + 30.0 * scale;

    // Считаем габариты со старым масштабом 1.0, чтобы размер кнопок не менялся
    let w_save = w_save_text * text_scale_calc + icon_sz_calc + padding;
    let w_discard = w_discard_text * text_scale_calc + icon_sz_calc + padding;
    let w_cancel = w_cancel_text * text_scale_calc + icon_sz_calc + padding;

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn widget_hitboxes_cover_regular_and_sidebar_buttons() {
        let button = Button {
            x: 10.0,
            y: 20.0,
            w: 120.0,
            h: 40.0,
            text: "Save".to_string(),
            icon: None,
            text_scale: 1.0,
            icon_size: 16.0,
        };
        assert!(button.is_hovered(10.0, 20.0));
        assert!(button.is_hovered(130.0, 60.0));
        assert!(!button.is_hovered(131.0, 60.0));

        let icon = IconButton {
            x: 12.0,
            y: 100.0,
            size: 24.0,
            icon: Some(IconType::Explorer),
            is_active: false,
            icon_size: None,
            active_square_width: Some(48.0),
            custom_color: None,
        };
        assert!(icon.is_hovered(0.0, 88.0));
        assert!(icon.is_hovered(48.0, 136.0));
        assert!(!icon.is_hovered(49.0, 112.0));

        let regular_icon = IconButton {
            active_square_width: None,
            ..icon
        };
        assert!(regular_icon.is_hovered(12.0, 100.0));
        assert!(regular_icon.is_hovered(36.0, 124.0));
        assert!(!regular_icon.is_hovered(36.1, 124.0));
    }

    #[test]
    fn button_layout_helpers_scale_text_and_icons_consistently() {
        let (new_btn, open_btn, ide_btn) =
            welcome_buttons_from_widths(20.0, 30.0, 2.0, 80.0, 100.0, 60.0);
        assert_eq!(new_btn.text, "Новый файл");
        assert_eq!(open_btn.text, "Открыть файл");
        assert_eq!(ide_btn.text, "Режим IDE");
        assert!(new_btn.icon.is_none());
        assert!(open_btn.icon.is_none());
        assert!(ide_btn.icon.is_none());
        assert_eq!(new_btn.h, 80.0);
        assert_eq!(new_btn.icon_size, 52.0);
        assert_eq!(new_btn.text_scale, 1.0);
        assert!(new_btn.x < open_btn.x);
        assert!(open_btn.x < ide_btn.x);

        let (save, discard, cancel) =
            dialog_buttons_from_widths(10.0, 20.0, 500.0, 240.0, 1.5, 90.0, 110.0, 70.0);
        assert_eq!(save.text, "Сохранить");
        assert_eq!(discard.text, "Отклонить");
        assert_eq!(cancel.text, "Отмена");
        assert_eq!(save.h, 66.0);
        assert_eq!(save.text_scale, 1.04);
        assert_eq!(save.icon_size, 42.0);
        assert!(matches!(save.icon, Some(IconType::Save)));
        assert!(matches!(discard.icon, Some(IconType::Discard)));
        assert!(matches!(cancel.icon, Some(IconType::Cancel)));
    }

    #[test]
    fn layout_helpers_do_not_overlap_at_small_widths() {
        let (new_btn, open_btn, ide_btn) =
            welcome_buttons_from_widths(0.0, 0.0, 1.0, 44.0, 240.0, 80.0);
        assert!(new_btn.w > 0.0);
        assert_eq!(open_btn.x, new_btn.x + new_btn.w + 15.0);
        assert_eq!(ide_btn.x, open_btn.x + open_btn.w + 15.0);
        assert!(new_btn.is_hovered(new_btn.x, new_btn.y));
        assert!(ide_btn.is_hovered(ide_btn.x + ide_btn.w, ide_btn.y + ide_btn.h));

        let (save, discard, cancel) =
            dialog_buttons_from_widths(0.0, 0.0, 300.0, 120.0, 0.8, 40.0, 50.0, 30.0);
        assert_eq!(save.h, 35.2);
        assert!((discard.x - (save.x + save.w + 11.2)).abs() < 0.001);
        assert!((cancel.x - (discard.x + discard.w + 11.2)).abs() < 0.001);
        assert!(save.is_hovered(save.x + save.w, save.y + save.h));
        assert!(!cancel.is_hovered(cancel.x + cancel.w + 0.1, cancel.y));
    }

    #[test]
    fn welcome_and_dialog_button_layouts_are_stable() {
        let (new_btn, open_btn, ide_btn) =
            welcome_buttons_from_widths(20.0, 30.0, 2.0, 80.0, 100.0, 60.0);
        assert_eq!(
            (new_btn.x, new_btn.y, new_btn.w, new_btn.h),
            (20.0, 30.0, 196.0, 80.0)
        );
        assert_eq!(open_btn.x, 246.0);
        assert_eq!(open_btn.w, 216.0);
        assert_eq!(ide_btn.x, 492.0);
        assert_eq!(ide_btn.w, 176.0);

        let (save, discard, cancel) =
            dialog_buttons_from_widths(10.0, 20.0, 500.0, 240.0, 1.5, 90.0, 110.0, 70.0);
        assert_eq!(save.text, "Сохранить");
        assert_eq!(discard.text, "Отклонить");
        assert_eq!(cancel.text, "Отмена");
        assert!(matches!(save.icon, Some(IconType::Save)));
        assert!(matches!(discard.icon, Some(IconType::Discard)));
        assert!(matches!(cancel.icon, Some(IconType::Cancel)));
        assert_eq!(save.y, 161.0);
        assert_eq!(save.w, 189.0);
        assert_eq!(discard.x, save.x + save.w + 21.0);
        assert_eq!(cancel.x, discard.x + discard.w + 21.0);
        assert_eq!(cancel.w, 169.0);
    }
}
