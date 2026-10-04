use crate::renderer::Theme;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ThemeId {
    Dracula,
    OneDark,
    Forest,
    Sepia,
    OneLight,
}

impl ThemeId {
    pub(crate) const ALL: [Self; 5] = [
        Self::Dracula,
        Self::OneDark,
        Self::Forest,
        Self::Sepia,
        Self::OneLight,
    ];

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Dracula => "Dracula",
            Self::OneDark => "One Dark",
            Self::Forest => "Forest",
            Self::Sepia => "Sepia",
            Self::OneLight => "One Light",
        }
    }

    pub(crate) fn key(self) -> &'static str {
        match self {
            Self::Dracula => "dracula",
            Self::OneDark => "one_dark",
            Self::Forest => "forest",
            Self::Sepia => "sepia",
            Self::OneLight => "one_light",
        }
    }

    pub(crate) fn from_key(key: &str) -> Self {
        match key {
            "dracula" => Self::Dracula,
            "one_dark" => Self::OneDark,
            "forest" => Self::Forest,
            "sepia" => Self::Sepia,
            "one_light" => Self::OneLight,
            _ => Self::Dracula,
        }
    }

    pub(crate) fn is_dark(self) -> bool {
        matches!(self, Self::Dracula | Self::OneDark | Self::Forest)
    }
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SyntaxRole {
    Fg,
    Comment,
    Keyword,
    KeywordControl,
    Function,
    String,
    Constant,
    Parameter,
    Class,
    MdCode,
    Delimiter,
    TreeGuide,
    LogText,
    Interpolation,
}

impl SyntaxRole {
    pub(crate) const COUNT: usize = 14;
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct SyntaxPalette {
    pub(crate) colors: [[f32; 4]; SyntaxRole::COUNT],
}

impl SyntaxPalette {
    pub(crate) fn color(&self, role: SyntaxRole) -> [f32; 4] {
        match role {
            SyntaxRole::Interpolation => self.colors[SyntaxRole::Fg as usize],
            _ => self.colors[role as usize],
        }
    }

    pub(crate) fn for_id(id: ThemeId) -> Self {
        let theme = theme_values(id);
        let fg = theme.fg;
        let comment = theme.line_num;
        let keyword = theme.keyword;
        let keyword_control = theme.modified_unsaved;
        let function = theme.function;
        let string = theme.string;
        let constant = theme.constant;
        let parameter = theme.parameter;
        let class = theme.class;
        let md_code = theme.md_code;
        let (delimiter, tree_guide, log_text) = match id {
            ThemeId::Dracula => (
                [0.6, 0.6, 0.65, 1.0],
                [0.45, 0.45, 0.50, 1.0],
                [0.875, 0.882, 0.902, 1.0],
            ),
            _ => (comment, theme.line_num, fg),
        };

        Self {
            colors: [
                fg,
                comment,
                keyword,
                keyword_control,
                function,
                string,
                constant,
                parameter,
                class,
                md_code,
                delimiter,
                tree_guide,
                log_text,
                fg,
            ],
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct UiPalette {
    pub(crate) syntax: SyntaxPalette,
    pub(crate) is_dark: bool,
    pub(crate) text: [f32; 4],
    pub(crate) text_dim: [f32; 4],
    pub(crate) bg_raised: [f32; 4],
    pub(crate) border: [f32; 4],
    pub(crate) accent: [f32; 4],
}

impl UiPalette {
    pub(crate) fn for_id(_id: ThemeId) -> Self {
        Self {
            syntax: SyntaxPalette::for_id(ThemeId::Dracula),
            is_dark: true,
            text: [1.0, 1.0, 1.0, 1.0],
            text_dim: [0.7, 0.7, 0.7, 1.0],
            bg_raised: [0.15, 0.16, 0.20, 1.0],
            border: [0.224, 0.231, 0.251, 1.0],
            accent: [0.35, 0.26, 0.48, 1.0],
        }
    }

    pub(crate) fn ink(&self, alpha: f32) -> [f32; 4] {
        if self.is_dark {
            [1.0, 1.0, 1.0, alpha]
        } else {
            [self.text[0], self.text[1], self.text[2], alpha]
        }
    }
}

#[derive(Clone, Copy)]
struct ThemeValues {
    bg: [f32; 4],
    fg: [f32; 4],
    minimap_bg: [f32; 4],
    line_num: [f32; 4],
    selection: [f32; 4],
    modified_unsaved: [f32; 4],
    modified_saved: [f32; 4],
    diag_warn: [f32; 4],
    diag_error: [f32; 4],
    unused: [f32; 4],
    surface_bg: [f32; 4],
    keyword: [f32; 4],
    function: [f32; 4],
    string: [f32; 4],
    constant: [f32; 4],
    parameter: [f32; 4],
    class: [f32; 4],
    md_code: [f32; 4],
    terminal: [[f32; 4]; 16],
}

const fn rgb(hex: u32) -> [f32; 4] {
    [
        ((hex >> 16) & 0xff) as f32 / 255.0,
        ((hex >> 8) & 0xff) as f32 / 255.0,
        (hex & 0xff) as f32 / 255.0,
        1.0,
    ]
}

fn theme_values(id: ThemeId) -> ThemeValues {
    match id {
        ThemeId::Dracula => ThemeValues {
            bg: [0.156, 0.164, 0.211, 1.0],
            fg: [0.972, 0.972, 0.949, 1.0],
            minimap_bg: [0.129, 0.133, 0.172, 1.0],
            line_num: [0.384, 0.447, 0.643, 1.0],
            selection: [0.0; 4],
            modified_unsaved: [1.0, 0.474, 0.776, 1.0],
            modified_saved: [0.313, 0.980, 0.482, 1.0],
            diag_warn: [0.945, 0.980, 0.549, 1.0],
            diag_error: [1.0, 0.333, 0.333, 1.0],
            unused: [0.48, 0.48, 0.48, 0.6],
            surface_bg: [0.173, 0.180, 0.224, 1.0],
            keyword: [0.545, 0.913, 0.992, 1.0],
            function: [0.313, 0.980, 0.482, 1.0],
            string: [0.945, 0.980, 0.549, 1.0],
            constant: [0.741, 0.576, 0.976, 1.0],
            parameter: [0.973, 0.584, 0.502, 1.0],
            class: [0.45, 0.85, 0.90, 1.0],
            md_code: [0.902, 0.714, 0.451, 1.0],
            terminal: crate::app::terminal::ANSI_16_COLORS,
        },
        ThemeId::OneDark => ThemeValues {
            bg: rgb(0x282c34), fg: rgb(0xabb2bf), minimap_bg: rgb(0x21252b),
            line_num: rgb(0x636d83), selection: rgb(0x3e4451), modified_unsaved: rgb(0xc678dd), modified_saved: rgb(0x98c379),
            diag_warn: rgb(0xe5c07b), diag_error: rgb(0xe06c75), unused: [0.48, 0.48, 0.48, 0.6],
            surface_bg: rgb(0x2c313a), keyword: rgb(0xe5c07b), function: rgb(0x61afef),
            string: rgb(0x98c379), constant: rgb(0xd19a66), parameter: rgb(0xe06c75),
            class: rgb(0xe5c07b), md_code: rgb(0xd19a66),
            terminal: [0x3f4451,0xe06c75,0x98c379,0xe5c07b,0x61afef,0xc678dd,0x56b6c2,0xd7dae0,0x5c6370,0xef7f88,0xa9d48a,0xf0cc8c,0x74bdf5,0xd38be6,0x6bc6d2,0xf0f2f5].map(rgb),
        },
        ThemeId::Forest => ThemeValues {
            bg: rgb(0x2d353b), fg: rgb(0xd3c6aa), minimap_bg: rgb(0x272e33),
            line_num: rgb(0x7a8478), selection: rgb(0x475258), modified_unsaved: rgb(0xd699b6), modified_saved: rgb(0xa7c080),
            diag_warn: rgb(0xdbbc7f), diag_error: rgb(0xe67e80), unused: [0.48, 0.48, 0.48, 0.6],
            surface_bg: rgb(0x343f44), keyword: rgb(0x7fbbb3), function: rgb(0xa7c080),
            string: rgb(0xdbbc7f), constant: rgb(0xd699b6), parameter: rgb(0xe69875),
            class: rgb(0x83c092), md_code: rgb(0xdbbc7f),
            terminal: [0x475258,0xe67e80,0xa7c080,0xdbbc7f,0x7fbbb3,0xd699b6,0x83c092,0xd3c6aa,0x859289,0xf0959a,0xb8d08f,0xe6c98e,0x93c9c1,0xe2a9c4,0x96d0a4,0xe8dcc0].map(rgb),
        },
        ThemeId::Sepia => ThemeValues {
            bg: rgb(0xf4ecd8), fg: rgb(0x5b4636), minimap_bg: rgb(0xebe1c8),
            line_num: rgb(0x9c8c74), selection: rgb(0xe3d3b0), modified_unsaved: rgb(0xa8467a), modified_saved: rgb(0x56701a),
            diag_warn: rgb(0x9a6a00), diag_error: rgb(0xb23a2a), unused: [0.45, 0.45, 0.45, 0.6],
            surface_bg: rgb(0xede3cb), keyword: rgb(0x2f6f7e), function: rgb(0x56701a),
            string: rgb(0x86600c), constant: rgb(0x7a4e8c), parameter: rgb(0xa85a14),
            class: rgb(0x2e7562), md_code: rgb(0x86600c),
            terminal: [0x3b2e24,0xa0442c,0x56701a,0x86600c,0x2f6f7e,0x7a4e8c,0x2e7562,0x5b4636,0x8a7a62,0xb5512f,0x627f1f,0x9a6f10,0x2b7f92,0x8c5aa0,0x33856f,0x3b2e24].map(rgb),
        },
        ThemeId::OneLight => ThemeValues {
            bg: rgb(0xfafafa), fg: rgb(0x383a42), minimap_bg: rgb(0xeeeeef),
            line_num: rgb(0x9d9d9f), selection: rgb(0xdcdde3), modified_unsaved: rgb(0xa626a4), modified_saved: rgb(0x478f46),
            diag_warn: rgb(0xa96f00), diag_error: rgb(0xd84a3d), unused: [0.45, 0.45, 0.45, 0.6],
            surface_bg: rgb(0xf0f0f1), keyword: rgb(0xa96f00), function: rgb(0x4078f2),
            string: rgb(0x478f46), constant: rgb(0x986801), parameter: rgb(0xd84a3d),
            class: rgb(0xa96f00), md_code: rgb(0x986801),
            terminal: [0x383a42,0xd84a3d,0x478f46,0xa96f00,0x4078f2,0xa626a4,0x0184bc,0x383a42,0x84858c,0xc4433a,0x3f7f3e,0x956200,0x3366d6,0x921f90,0x0172a3,0x202227].map(rgb),
        },
    }
}

impl Theme {
    pub(crate) fn for_id(id: ThemeId, system_selection: [f32; 4]) -> Self {
        let values = theme_values(id);
        let (search_match, search_match_active, diff_added) = if id.is_dark() {
            ([0.6, 0.6, 0.6, 0.35], [1.0, 0.6, 0.0, 0.5], [0.18, 0.82, 0.34, 0.26])
        } else {
            ([0.85, 0.65, 0.13, 0.35], [0.95, 0.55, 0.0, 0.45], [0.20, 0.60, 0.25, 0.22])
        };
        Self {
            bg: values.bg,
            fg: values.fg,
            sel: if id == ThemeId::Dracula { system_selection } else { values.selection },
            minimap_bg: values.minimap_bg,
            line_num: values.line_num,
            minimap_cursor: if id == ThemeId::Dracula { system_selection } else { values.selection },
            modified_unsaved: values.modified_unsaved,
            modified_saved: values.modified_saved,
            diag_warn: values.diag_warn,
            diag_error: values.diag_error,
            unused: values.unused,
            surface_bg: values.surface_bg,
            syntax: SyntaxPalette::for_id(id),
            terminal: values.terminal,
            search_match,
            search_match_active,
            diff_added,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn luminance(color: [f32; 4]) -> f64 {
        let channel = |value: f32| {
            let value = f64::from(value);
            if value <= 0.04045 { value / 12.92 } else { ((value + 0.055) / 1.055).powf(2.4) }
        };
        0.2126 * channel(color[0]) + 0.7152 * channel(color[1]) + 0.0722 * channel(color[2])
    }

    fn contrast(a: [f32; 4], b: [f32; 4]) -> f64 {
        let (lighter, darker) = { let x = luminance(a); let y = luminance(b); if x >= y { (x, y) } else { (y, x) } };
        (lighter + 0.05) / (darker + 0.05)
    }

    #[test]
    fn ids_round_trip_and_fallback() {
        for id in ThemeId::ALL { assert_eq!(ThemeId::from_key(id.key()), id); }
        assert_eq!(ThemeId::from_key("garbage"), ThemeId::Dracula);
        assert_eq!(ThemeId::from_key(""), ThemeId::Dracula);
        assert_eq!(ThemeId::ALL.map(ThemeId::label), ["Dracula", "One Dark", "Forest", "Sepia", "One Light"]);
    }

    #[test]
    fn dracula_values_match_legacy_literals_and_ui_stays_dracula() {
        let selection = [0.21, 0.43, 0.65, 0.87];
        let theme = Theme::for_id(ThemeId::Dracula, selection);
        assert_eq!(theme.bg, [0.156, 0.164, 0.211, 1.0]);
        assert_eq!(theme.fg, [0.972, 0.972, 0.949, 1.0]);
        assert_eq!(theme.sel, selection);
        assert_eq!(theme.minimap_bg, [0.129, 0.133, 0.172, 1.0]);
        assert_eq!(theme.line_num, [0.384, 0.447, 0.643, 1.0]);
        assert_eq!(theme.minimap_cursor, selection);
        assert_eq!(theme.modified_unsaved, [1.0, 0.474, 0.776, 1.0]);
        assert_eq!(theme.modified_saved, [0.313, 0.980, 0.482, 1.0]);
        assert_eq!(theme.diag_warn, [0.945, 0.980, 0.549, 1.0]);
        assert_eq!(theme.diag_error, [1.0, 0.333, 0.333, 1.0]);
        assert_eq!(theme.unused, [0.48, 0.48, 0.48, 0.6]);
        assert_eq!(theme.surface_bg, [0.173, 0.180, 0.224, 1.0]);
        assert_eq!(theme.search_match, [0.6, 0.6, 0.6, 0.35]);
        assert_eq!(theme.search_match_active, [1.0, 0.6, 0.0, 0.5]);
        assert_eq!(theme.diff_added, [0.18, 0.82, 0.34, 0.26]);
        assert_eq!(theme.terminal, [
            [0.10, 0.10, 0.10, 1.0], [0.95, 0.30, 0.30, 1.0],
            [0.30, 0.85, 0.30, 1.0], [0.90, 0.85, 0.20, 1.0],
            [0.30, 0.60, 1.00, 1.0], [0.90, 0.35, 0.90, 1.0],
            [0.20, 0.85, 0.85, 1.0], [0.90, 0.90, 0.90, 1.0],
            [0.45, 0.45, 0.45, 1.0], [1.00, 0.40, 0.40, 1.0],
            [0.40, 1.00, 0.40, 1.0], [1.00, 1.00, 0.40, 1.0],
            [0.50, 0.70, 1.00, 1.0], [1.00, 0.50, 1.00, 1.0],
            [0.40, 1.00, 1.00, 1.0], [1.00, 1.00, 1.00, 1.0],
        ]);
        let syntax = SyntaxPalette::for_id(ThemeId::Dracula);
        let expected = [
            [0.972, 0.972, 0.949, 1.0], [0.384, 0.447, 0.643, 1.0],
            [0.545, 0.913, 0.992, 1.0], [1.0, 0.474, 0.776, 1.0],
            [0.313, 0.980, 0.482, 1.0], [0.945, 0.980, 0.549, 1.0],
            [0.741, 0.576, 0.976, 1.0], [0.973, 0.584, 0.502, 1.0],
            [0.45, 0.85, 0.90, 1.0], [0.902, 0.714, 0.451, 1.0],
            [0.6, 0.6, 0.65, 1.0], [0.45, 0.45, 0.50, 1.0],
            [0.875, 0.882, 0.902, 1.0], [0.972, 0.972, 0.949, 1.0],
        ];
        for (index, expected) in expected.into_iter().enumerate() { assert_eq!(syntax.colors[index], expected); }
        for id in ThemeId::ALL { assert_eq!(UiPalette::for_id(id), UiPalette::for_id(ThemeId::Dracula)); }
    }

    #[test]
    fn theme_colors_meet_contrast_constraints() {
        for id in ThemeId::ALL {
            let theme = Theme::for_id(id, [0.0; 4]);
            assert!(contrast(theme.fg, theme.bg) >= 4.5, "{:?} fg/bg", id);
            assert!(contrast(theme.line_num, theme.bg) >= 2.5, "{:?} line_num/bg", id);
            for role in [SyntaxRole::Keyword, SyntaxRole::KeywordControl, SyntaxRole::Function, SyntaxRole::String, SyntaxRole::Constant, SyntaxRole::Parameter, SyntaxRole::Class, SyntaxRole::MdCode] {
                assert!(contrast(theme.syntax.color(role), theme.bg) >= 3.5, "{:?} {:?}/bg", id, role);
            }
            assert!(contrast(theme.syntax.color(SyntaxRole::Comment), theme.bg) >= 2.5, "{:?} comment/bg", id);
            for index in [1, 2, 3, 4, 5, 6, 9, 10, 11, 12, 13, 14] {
                assert!(contrast(theme.terminal[index], theme.bg) >= 3.0, "{:?} ANSI {index}/bg", id);
            }
            assert!(contrast(theme.terminal[7], theme.bg) >= 4.5, "{:?} ANSI 7/bg", id);
        }
    }

    #[test]
    fn interpolation_uses_fg_and_ink_obeys_brightness() {
        for id in ThemeId::ALL {
            let palette = SyntaxPalette::for_id(id);
            assert_eq!(palette.color(SyntaxRole::Interpolation), palette.color(SyntaxRole::Fg));
            let ui = UiPalette::for_id(id);
            assert_eq!(ui.ink(0.4), [1.0, 1.0, 1.0, 0.4]);
        }
        let light_ui = UiPalette { is_dark: false, text: [0.2, 0.3, 0.4, 0.9], ..UiPalette::for_id(ThemeId::OneLight) };
        assert_eq!(light_ui.ink(0.4), [0.2, 0.3, 0.4, 0.4]);
    }
}
