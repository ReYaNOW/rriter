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
        self.editor_theme_id = editor;
        self.ui_theme_id = ui;
        self.theme = crate::renderer::Theme::for_id(editor, self.system_selection);
        if let Some(renderer) = self.renderer.as_mut() {
            renderer.set_themes(self.theme.clone(), ui, self.system_selection);
            renderer.theme_gen = renderer.theme_gen.wrapping_add(1);
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
