/// Декларативная система UI для автоматической обработки кнопок и виджетов
/// Устраняет необходимость вручную прописывать координаты и биндинги для каждой кнопки

use crate::renderer::Renderer;
use crate::widgets::{Button, IconButton};

/// Уникальный идентификатор UI элемента
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UiId {
    // Welcome screen
    WelcomeNewFile,
    WelcomeOpenFile,
    WelcomeIdeMode,
    WelcomeRecentFile(usize),

    // Dialog
    DialogSave,
    DialogDiscard,
    DialogCancel,

    // Settings tabs
    SettingsTab(usize),

    // Settings IDE
    SettingsIdeAddWorkspace,
    SettingsIdeRemoveWorkspace(usize),
    SettingsIdeAddIgnore,
    SettingsIdeRemoveIgnore(usize),
    SettingsIdeIgnoreInput,

    // LSP panel
    LspServerRestart(usize),
    LspServerToggle(usize),
    LspServerStop(usize),
    LspServerLogs(usize),
    LspServerFixAll(usize),
    LspLogFoldToggle(usize, usize), // (server_idx, line_idx)

    // Sidebar
    SidebarSlot(crate::app::PanelId),

    // File tree
    FileTreeNode(usize),
}

/// Тип UI элемента с его геометрией
#[derive(Debug, Clone)]
pub enum UiElement {
    Button {
        id: UiId,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
    },
    IconButton {
        id: UiId,
        x: f32,
        y: f32,
        size: f32,
        active_square_width: Option<f32>,
    },
    TextInput {
        id: UiId,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
    },
    Rect {
        id: UiId,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
    },
}

impl UiElement {
    /// Проверяет, находится ли точка (mx, my) внутри элемента
    pub fn contains(&self, mx: f32, my: f32) -> bool {
        match self {
            UiElement::Button { x, y, w, h, .. } | UiElement::Rect { x, y, w, h, .. } => {
                mx >= *x && mx <= x + w && my >= *y && my <= y + h
            }
            UiElement::IconButton {
                x,
                y,
                size,
                active_square_width,
                ..
            } => {
                if let Some(sq_w) = active_square_width {
                    let icon_center = y + size / 2.0;
                    let sq_y = (icon_center - sq_w / 2.0).round();
                    mx >= 0.0 && mx <= *sq_w && my >= sq_y && my <= sq_y + sq_w
                } else {
                    mx >= *x && mx <= x + size && my >= *y && my <= y + size
                }
            }
            UiElement::TextInput { x, y, w, h, .. } => {
                mx >= *x && mx <= x + w && my >= *y && my <= y + h
            }
        }
    }

    pub fn id(&self) -> UiId {
        match self {
            UiElement::Button { id, .. }
            | UiElement::IconButton { id, .. }
            | UiElement::TextInput { id, .. }
            | UiElement::Rect { id, .. } => *id,
        }
    }
}

/// Реестр всех UI элементов на текущем кадре
pub struct UiRegistry {
    elements: Vec<UiElement>,
    hovered: Option<UiId>,
    wants_pointer: bool,
    wants_text: bool,
}

impl UiRegistry {
    pub fn new() -> Self {
        Self {
            elements: Vec::with_capacity(128),
            hovered: None,
            wants_pointer: false,
            wants_text: false,
        }
    }

    /// Очищает реестр перед новым кадром
    pub fn clear(&mut self) {
        self.elements.clear();
        self.hovered = None;
        self.wants_pointer = false;
        self.wants_text = false;
    }

    /// Регистрирует кнопку и возвращает, наведена ли на неё мышь
    pub fn register_button(
        &mut self,
        id: UiId,
        button: &Button,
        renderer: &mut Renderer,
        mx: f32,
        my: f32,
        scale: f32,
        pressed: bool,
    ) -> bool {
        let hovered = button.render(renderer, mx, my, scale, pressed);

        self.elements.push(UiElement::Button {
            id,
            x: button.x,
            y: button.y,
            w: button.w,
            h: button.h,
        });

        if hovered {
            self.hovered = Some(id);
            self.wants_pointer = true;
        }
        hovered
    }

    /// Регистрирует иконочную кнопку
    pub fn register_icon_button(
        &mut self,
        id: UiId,
        icon_button: &IconButton,
        renderer: &mut Renderer,
        mx: f32,
        my: f32,
        scale: f32,
        pressed: bool,
    ) -> bool {
        let hovered = icon_button.render(renderer, mx, my, scale, pressed);

        self.elements.push(UiElement::IconButton {
            id,
            x: icon_button.x,
            y: icon_button.y,
            size: icon_button.size,
            active_square_width: icon_button.active_square_width,
        });

        if hovered {
            self.hovered = Some(id);
            self.wants_pointer = true;
        }
        hovered
    }

    /// Регистрирует текстовое поле
    pub fn register_text_input(&mut self, id: UiId, x: f32, y: f32, w: f32, h: f32, mx: f32, my: f32) -> bool {
        let hovered = mx >= x && mx <= x + w && my >= y && my <= y + h;

        self.elements.push(UiElement::TextInput { id, x, y, w, h });

        if hovered {
            self.hovered = Some(id);
            self.wants_text = true;
        }
        hovered
    }

    /// Регистрирует кликабельную область (для элементов дерева файлов, чипов и т.д.)
    pub fn register_rect(&mut self, id: UiId, x: f32, y: f32, w: f32, h: f32, mx: f32, my: f32) -> bool {
        let hovered = mx >= x && mx <= x + w && my >= y && my <= y + h;

        self.elements.push(UiElement::Rect { id, x, y, w, h });

        if hovered {
            self.hovered = Some(id);
            self.wants_pointer = true;
        }
        hovered
    }

    /// Находит элемент под курсором мыши
    pub fn find_at(&self, mx: f32, my: f32) -> Option<UiId> {
        // Ищем с конца (последние элементы рисуются поверх)
        self.elements
            .iter()
            .rev()
            .find(|el| el.contains(mx, my))
            .map(|el| el.id())
    }

    /// Возвращает текущий наведённый элемент
    pub fn hovered(&self) -> Option<UiId> {
        self.hovered
    }

    /// Нужен ли курсор-указатель
    pub fn wants_pointer(&self) -> bool {
        self.wants_pointer
    }

    /// Нужен ли текстовый курсор
    pub fn wants_text(&self) -> bool {
        self.wants_text
    }

    /// Возвращает код курсора (0 = стрелка, 1 = указатель, 2 = текст)
    pub fn cursor_code(&self) -> u8 {
        if self.wants_text {
            2
        } else if self.wants_pointer {
            1
        } else {
            0
        }
    }
}
