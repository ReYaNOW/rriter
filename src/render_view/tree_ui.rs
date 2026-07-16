use crate::renderer::Renderer;

pub(crate) const TREE_ROW_H: f32 = 28.0;
pub(crate) const TREE_INDENT_W: f32 = 18.0;
pub(crate) const TREE_TEXT_SCALE: f32 = 1.0;
pub(crate) const TREE_DISCLOSURE_SLOT: f32 = 16.0;
pub(crate) const TREE_ICON_X_OFFSET: f32 = 18.0;

pub(crate) struct TreeLabelResult {
    pub x: f32,
    pub y: f32,
    pub w: f32,
}

pub(crate) fn tree_dir_icon_y(row_y: f32, row_h: f32, scale: f32) -> f32 {
    let icon_size = 20.0 * scale;
    (row_y + (row_h - icon_size) / 2.0).round()
}

pub(crate) fn tree_icon_x(indent_x: f32, scale: f32) -> f32 {
    indent_x + TREE_ICON_X_OFFSET * scale
}

#[cfg_attr(coverage_nightly, coverage(off))]
impl Renderer {
    pub(crate) fn tree_row_text_y(row_y: f32, row_h: f32, scale: f32) -> f32 {
        (row_y + row_h * 0.5 + 5.5 * scale).round()
    }

    pub(crate) fn draw_tree_disclosure_icon(
        &mut self,
        expanded: bool,
        x: f32,
        row_y: f32,
        row_h: f32,
        color: [f32; 4],
    ) {
        let icon = if expanded { '▼' } else { '▶' };
        if let Some(g) = self.get_ui_glyph(icon) {
            let q_x = x.round() + g.offset_x;
            let q_y = (row_y + (row_h - g.height) / 2.0).round();
            self.push_quad(
                q_x, q_y, g.width, g.height, g.u, g.v, g.uw, g.vh, color, g.is_emoji,
            );
            return;
        }

        let mut buf = [0; 4];
        let icon_str = icon.encode_utf8(&mut buf);
        self.draw_string_scaled(
            icon_str,
            x,
            Self::tree_row_text_y(row_y, row_h, 1.0),
            color,
            TREE_TEXT_SCALE,
        );
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn draw_tree_dir_entry(
        &mut self,
        name: &str,
        icon_key: &'static str,
        indent_x: f32,
        row_y: f32,
        row_h: f32,
        right_x: f32,
        expanded: bool,
        color: [f32; 4],
        arrow_color: [f32; 4],
        scale: f32,
        text_scale: f32,
        scratch: &mut String,
    ) -> TreeLabelResult {
        let arrow_x = indent_x - 2.0 * scale;
        let text_y = Self::tree_row_text_y(row_y, row_h, scale);
        self.draw_tree_disclosure_icon(expanded, arrow_x, row_y, row_h, arrow_color);

        let icon_size = 20.0 * scale;
        let icon_y = tree_dir_icon_y(row_y, row_h, scale);
        let icon_x = tree_icon_x(indent_x, scale);
        self.draw_file_icon(icon_key, true, icon_x, icon_y, icon_size);

        let text_x = icon_x + icon_size + 4.0 * scale;
        let max_w = (right_x - text_x).max(0.0);
        let label_w =
            self.draw_tree_label_clipped(name, text_x, text_y, max_w, color, text_scale, scratch);
        TreeLabelResult {
            x: text_x,
            y: text_y,
            w: label_w,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn draw_tree_leaf_label(
        &mut self,
        name: &str,
        text_x: f32,
        row_y: f32,
        row_h: f32,
        right_x: f32,
        color: [f32; 4],
        scale: f32,
        text_scale: f32,
        scratch: &mut String,
    ) -> TreeLabelResult {
        let text_y = Self::tree_row_text_y(row_y, row_h, scale);
        let label_w = self.draw_tree_label_clipped(
            name,
            text_x,
            text_y,
            (right_x - text_x).max(0.0),
            color,
            text_scale,
            scratch,
        );
        TreeLabelResult {
            x: text_x,
            y: text_y,
            w: label_w,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tree_dir_icon_y_is_centered_for_open_and_closed_folders() {
        let closed_icon_y = tree_dir_icon_y(40.0, TREE_ROW_H, 1.0);
        let open_icon_y = tree_dir_icon_y(40.0, TREE_ROW_H, 1.0);

        assert_eq!(closed_icon_y, 44.0);
        assert_eq!(closed_icon_y, open_icon_y);
    }

    #[test]
    fn tree_icon_x_is_shared_by_folder_and_file_rows() {
        assert_eq!(tree_icon_x(40.0, 1.0), 58.0);
        assert_eq!(tree_icon_x(40.0, 1.25), 62.5);
    }
}
