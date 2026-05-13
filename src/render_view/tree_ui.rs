use crate::renderer::Renderer;

pub(crate) const TREE_ROW_H: f32 = 28.0;
pub(crate) const TREE_INDENT_W: f32 = 18.0;
pub(crate) const TREE_TEXT_SCALE: f32 = 1.0;

pub(crate) struct TreeLabelResult {
    pub x: f32,
    pub y: f32,
    pub w: f32,
}

#[cfg_attr(coverage_nightly, coverage(off))]
impl Renderer {
    pub(crate) fn tree_row_text_y(row_y: f32, row_h: f32, scale: f32) -> f32 {
        row_y + row_h / 2.0 + 5.5 * scale
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
        let arrow_str = if expanded { "▼" } else { "▶" };
        let arrow_x = indent_x - 2.0 * scale;
        let text_y = Self::tree_row_text_y(row_y, row_h, scale);
        self.draw_string_scaled(arrow_str, arrow_x, text_y, arrow_color, TREE_TEXT_SCALE);

        let icon_size = 20.0 * scale;
        let icon_y = row_y + (row_h - icon_size) / 2.0;
        let icon_x = indent_x + 18.0 * scale;
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
