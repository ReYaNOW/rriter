use crate::app::App;
use crate::theme::ThemeId;

impl App {
    pub(crate) fn theme_selection(&self) -> crate::theme::ThemeSelection {
        crate::theme::ThemeSelection {
            linked: self.theme_linked,
            editor: self.editor_theme_id,
            ui: self.ui_theme_id,
        }
    }

    pub(crate) fn apply_themes(&mut self, editor: ThemeId, ui: ThemeId) {
        if editor == self.editor_theme_id && ui == self.ui_theme_id {
            return;
        }

        let editor_changed = editor != self.editor_theme_id;
        let ui_variant_changed = ui.is_dark() != self.ui_theme_id.is_dark();
        self.editor_theme_id = editor;
        self.ui_theme_id = ui;
        self.theme = crate::renderer::Theme::for_id(editor, self.system_selection);
        if let Some(renderer) = self.renderer.as_mut() {
            renderer.set_themes(self.theme.clone(), ui, self.system_selection);
            renderer.theme_gen = renderer.theme_gen.wrapping_add(1);
            if ui_variant_changed {
                // File icons are rasterized per light/dark variant; the other variant's
                // raw rasters are dead weight (atlas entries stay for a quick switch back).
                let is_dark = renderer.ui.is_dark;
                renderer.rasterized_file_icons.retain(|(_, variant), _| *variant == is_dark);
            }
        }
        if ui_variant_changed {
            // Every icon key misses after a light/dark switch: prewarm the new variant in the
            // background (the scan redraws on IconsReady) instead of one icon per frame.
            self.refresh_file_tree();
        }
        if editor_changed {
            self.set_pdf_dark_pages(editor.is_dark());
        }
        self.persist_theme_settings();
        if let Some(window) = self.window.as_ref() {
            window.request_redraw();
        }
    }

    pub(crate) fn set_theme_linked(&mut self, on: bool) {
        if self.theme_linked == on {
            return;
        }
        self.theme_linked = on;
        if on && self.ui_theme_id != self.editor_theme_id {
            self.apply_themes(self.editor_theme_id, self.editor_theme_id);
        } else {
            self.persist_theme_settings();
            if let Some(window) = self.window.as_ref() {
                window.request_redraw();
            }
        }
    }

    pub(crate) fn persist_theme_settings(&self) {
        self.save_current_config();
    }
}
