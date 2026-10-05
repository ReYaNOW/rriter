use crate::renderer::Theme;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ThemeSelection {
    pub linked: bool,
    pub editor: ThemeId,
    pub ui: ThemeId,
}

impl Default for ThemeSelection {
    fn default() -> Self {
        Self {
            linked: true,
            editor: ThemeId::Dracula,
            ui: ThemeId::Dracula,
        }
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
    pub(crate) const COUNT: usize = Self::Interpolation as usize + 1;
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
        let comment = theme.comment;
        let keyword = theme.keyword;
        let keyword_control = theme.keyword_control;
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

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum UiRole {
    BgPanel,
    BgPanelAlt,
    BgDialog,
    BgInput,
    BgTooltip,
    Scrim,
    RowHover,
    RowActive,
    BgChip,
    BgCode,
    BgMedia,
    BgPdfPaper,
    Border,
    TextPrimary,
    TextSecondary,
    TextMuted,
    TextOnAccent,
    Accent,
    AccentHover,
    AccentSoft,
    Selection,
    InputSelection,
    ScrollbarThumb,
    ScrollbarThumbHover,
    Icon,
    Error,
    Warning,
    Success,
    Info,
    Danger,
    Link,
    GitBranch,
    GitCommit,
    GitHunkAdded,
    GitHunkDeleted,
    GitGraphNode,
    GitGraphEdge,
    GitAdded,
    GitModified,
    GitDeleted,
    GitRenamed,
    HttpGet,
    HttpPost,
    HttpPut,
    HttpPatch,
    HttpDelete,
    HttpStatusSuccess,
    HttpStatusRedirect,
    HttpStatusError,
    DatabaseConnected,
    DatabaseDisconnected,
    LspRunning,
    LspStarting,
    LspDisabled,
    LspMissing,
    LspCrashed,
    SearchMatch,
    PdfSearchMatch,
    PdfSearchMatchActive,
    ApiMockRoute,
    UnsavedMarker,
    TokenProperty,
    TokenPunctuation,
    MarkdownQuote,
    MarkdownHeading,
    PdfText,
}

impl UiRole {
    pub(crate) const COUNT: usize = Self::PdfText as usize + 1;
    pub(crate) const ALL: [Self; Self::COUNT] = [
        Self::BgPanel,
        Self::BgPanelAlt,
        Self::BgDialog,
        Self::BgInput,
        Self::BgTooltip,
        Self::Scrim,
        Self::RowHover,
        Self::RowActive,
        Self::BgChip,
        Self::BgCode,
        Self::BgMedia,
        Self::BgPdfPaper,
        Self::Border,
        Self::TextPrimary,
        Self::TextSecondary,
        Self::TextMuted,
        Self::TextOnAccent,
        Self::Accent,
        Self::AccentHover,
        Self::AccentSoft,
        Self::Selection,
        Self::InputSelection,
        Self::ScrollbarThumb,
        Self::ScrollbarThumbHover,
        Self::Icon,
        Self::Error,
        Self::Warning,
        Self::Success,
        Self::Info,
        Self::Danger,
        Self::Link,
        Self::GitBranch,
        Self::GitCommit,
        Self::GitHunkAdded,
        Self::GitHunkDeleted,
        Self::GitGraphNode,
        Self::GitGraphEdge,
        Self::GitAdded,
        Self::GitModified,
        Self::GitDeleted,
        Self::GitRenamed,
        Self::HttpGet,
        Self::HttpPost,
        Self::HttpPut,
        Self::HttpPatch,
        Self::HttpDelete,
        Self::HttpStatusSuccess,
        Self::HttpStatusRedirect,
        Self::HttpStatusError,
        Self::DatabaseConnected,
        Self::DatabaseDisconnected,
        Self::LspRunning,
        Self::LspStarting,
        Self::LspDisabled,
        Self::LspMissing,
        Self::LspCrashed,
        Self::SearchMatch,
        Self::PdfSearchMatch,
        Self::PdfSearchMatchActive,
        Self::ApiMockRoute,
        Self::UnsavedMarker,
        Self::TokenProperty,
        Self::TokenPunctuation,
        Self::MarkdownQuote,
        Self::MarkdownHeading,
        Self::PdfText,
    ];
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
    pub(crate) roles: [[f32; 4]; UiRole::COUNT],
    pub(crate) dracula: bool,
}

impl UiPalette {
    pub(crate) fn for_id(id: ThemeId) -> Self {
        if id == ThemeId::Dracula {
            return Self {
                syntax: SyntaxPalette::for_id(ThemeId::Dracula),
                is_dark: true,
                text: [1.0, 1.0, 1.0, 1.0],
                text_dim: [0.7, 0.7, 0.7, 1.0],
                bg_raised: [0.15, 0.16, 0.20, 1.0],
                border: [0.224, 0.231, 0.251, 1.0],
                accent: [0.35, 0.26, 0.48, 1.0],
                roles: [
                [0.04,0.05,0.07,1.0],
                [0.10,0.11,0.14,0.98],
                [0.12,0.13,0.17,1.0],
                [0.12,0.13,0.17,1.0],
                [0.11,0.12,0.16,0.98],
                [0.0,0.0,0.0,0.42],
                [0.28,0.30,0.33,1.0],
                [0.28,0.24,0.34,1.0],
                [0.3, 0.27, 0.38, 1.0],
                [0.07, 0.073, 0.092, 1.0],
                [0.11,0.12,0.15,0.96],
                [0.11,0.12,0.13,1.0],
                [0.224,0.231,0.251,1.0],
                [1.0,1.0,1.0,1.0],
                [0.78,0.80,0.88,1.0],
                [0.55,0.57,0.64,1.0],
                [0.9,0.9,0.9,1.0],
                [0.35,0.26,0.48,1.0],
                [0.60,0.35,0.85,1.0],
                [0.60,0.35,0.85,0.24],
                [0.60, 0.35, 0.85, 0.24],
                [0.55, 0.36, 0.90, 0.36],
                [0.7,0.33,0.54,0.8],
                [0.7,0.33,0.54,1.0],
                [1.0,1.0,1.0,1.0],
                [1.0, 0.38, 0.38, 1.0],
                [1.0, 0.7, 0.42, 1.0],
                [0.46, 0.82, 0.58, 1.0],
                [0.35,0.75,1.0,1.0],
                [1.0, 0.58, 0.62, 1.0],
                [0.72,0.52,1.0,1.0],
                [0.78,0.68,1.0,1.0],
                [0.86,0.90,1.0,1.0],
                [0.52,0.82,0.58,1.0],
                [0.95,0.42,0.46,1.0],
                [0.28,0.24,0.40,1.0],
                [0.38,0.62,1.0,1.0],
                [0.48, 0.82, 0.52, 1.0],
                [0.97, 0.76, 0.38, 1.0],
                [0.95, 0.42, 0.46, 1.0],
                [0.48, 0.74, 1.0, 1.0],
                [0.35,0.75,1.0,1.0],
                [0.48,0.86,0.52,1.0],
                [1.0, 0.76, 0.32, 1.0],
                [0.78, 0.58, 1.0, 1.0],
                [1.0,0.42,0.42,1.0],
                [0.48,0.86,0.52,1.0],
                [0.35,0.75,1.0,1.0],
                [1.0,0.42,0.42,1.0],
                [0.35,0.85,0.48,1.0],
                [0.45, 0.47, 0.53, 1.0],
                [0.28,0.85,0.45,1.0],
                [0.85,0.75,0.25,1.0],
                [0.45,0.45,0.45,1.0],
                [0.95,0.45,0.30,1.0],
                [0.90,0.30,0.30,1.0],
                [0.18,0.20,0.22,1.0],
                [1.0,0.85,0.2,0.35],
                [1.0,0.55,0.1,0.55],
                [0.50,0.90,0.55,1.0],
                [1.0, 0.67, 0.16, 1.0],
                [1.0,0.68,0.26,1.0],
                [0.86, 0.87, 0.91, 1.0],
                [0.78,0.75,0.87,1.0],
                [0.95,0.93,0.98,1.0],
                [0.972, 0.972, 0.949, 1.0],
                ],
                dracula: true,
            };
        }

        let values = theme_values(id);
        let syntax = SyntaxPalette::for_id(id);
        let text = values.fg;
        let text_dim = values.fg;
        let bg_panel = mix(values.bg, text, 0.04);
        let bg_panel_alt = mix(values.bg, text, 0.06);
        let bg_dialog = mix(values.bg, text, 0.04);
        let bg_input = mix(values.bg, text, 0.02);
        let bg_tooltip = mix(values.bg, text, 0.03);
        let bg_code = values.surface_bg;
        let accent = if matches!(id, ThemeId::OneDark | ThemeId::OneLight) {
            values.function
        } else {
            values.keyword_control
        };
        let roles = UiRole::ALL.map(|role| ui_theme_color(
            role, id, values, syntax, bg_panel, bg_panel_alt, bg_dialog,
            bg_input, bg_tooltip, bg_code, accent,
        ));
        Self {
            syntax,
            is_dark: id.is_dark(),
            text,
            text_dim,
            bg_raised: bg_dialog,
            border: mix(values.bg, text, 0.12),
            accent,
            roles,
            dracula: false,
        }
    }

    #[inline]
    pub(crate) fn pick(&self, role: UiRole, dracula: [f32; 4]) -> [f32; 4] {
        if self.dracula { dracula } else { self.roles[role as usize] }
    }

    pub(crate) fn ink(&self, alpha: f32) -> [f32; 4] {
        if self.is_dark {
            [1.0, 1.0, 1.0, alpha]
        } else {
            [self.text[0], self.text[1], self.text[2], alpha]
        }
    }

    pub(crate) fn shadow_alpha(&self, alpha: f32) -> [f32; 4] {
        [0.0, 0.0, 0.0, alpha]
    }
}

fn mix(from: [f32; 4], to: [f32; 4], amount: f32) -> [f32; 4] {
    [
        from[0] + (to[0] - from[0]) * amount,
        from[1] + (to[1] - from[1]) * amount,
        from[2] + (to[2] - from[2]) * amount,
        1.0,
    ]
}

fn ui_theme_color(
    role: UiRole,
    id: ThemeId,
    values: ThemeValues,
    syntax: SyntaxPalette,
    bg_panel: [f32; 4],
    bg_panel_alt: [f32; 4],
    bg_dialog: [f32; 4],
    bg_input: [f32; 4],
    bg_tooltip: [f32; 4],
    bg_code: [f32; 4],
    accent: [f32; 4],
) -> [f32; 4] {
    let color = |role: SyntaxRole| syntax.color(role);
    let alpha = |mut color: [f32; 4], a: f32| { color[3] = a; color };
    match role {
        UiRole::BgPanel => bg_panel,
        UiRole::BgPanelAlt => bg_panel_alt,
        UiRole::BgDialog => bg_dialog,
        UiRole::BgInput => bg_input,
        UiRole::BgTooltip => bg_tooltip,
        UiRole::Scrim => [0.0, 0.0, 0.0, 0.42],
        UiRole::RowHover => mix(bg_panel, accent, 0.06),
        UiRole::RowActive => mix(bg_panel, accent, 0.08),
        UiRole::BgChip => mix(bg_panel, accent, 0.12),
        UiRole::BgCode => bg_code,
        UiRole::BgMedia => bg_panel_alt,
        UiRole::BgPdfPaper => [0.972, 0.972, 0.949, 1.0],
        UiRole::Border => mix(values.bg, values.fg, 0.12),
        UiRole::TextPrimary | UiRole::TextSecondary => values.fg,
        UiRole::TextMuted => mix(values.fg, values.bg, 0.28),
        UiRole::TextOnAccent => if id.is_dark() { [0.98, 0.98, 0.98, 1.0] } else { values.bg },
        UiRole::Accent => accent,
        UiRole::AccentHover => mix(accent, values.fg, 0.16),
        UiRole::AccentSoft => alpha(accent, 0.24),
        UiRole::Selection | UiRole::InputSelection => alpha(values.selection, 0.42),
        UiRole::ScrollbarThumb => alpha(mix(values.fg, values.bg, 0.40), 0.72),
        UiRole::ScrollbarThumbHover => alpha(mix(values.fg, values.bg, 0.22), 0.90),
        UiRole::Icon => values.fg,
        UiRole::Error | UiRole::Danger | UiRole::HttpStatusError | UiRole::GitDeleted => values.diag_error,
        UiRole::Warning => values.diag_warn,
        UiRole::Success | UiRole::HttpStatusSuccess | UiRole::GitAdded | UiRole::DatabaseConnected => values.modified_saved,
        UiRole::Info | UiRole::HttpStatusRedirect | UiRole::GitBranch | UiRole::GitGraphNode | UiRole::GitGraphEdge => color(SyntaxRole::Keyword),
        UiRole::Link => color(SyntaxRole::Function),
        UiRole::GitCommit | UiRole::GitHunkAdded | UiRole::GitRenamed | UiRole::HttpGet | UiRole::HttpPut | UiRole::HttpPatch | UiRole::DatabaseDisconnected | UiRole::LspRunning | UiRole::LspStarting => color(SyntaxRole::Function),
        UiRole::GitHunkDeleted | UiRole::GitModified | UiRole::HttpDelete | UiRole::LspCrashed => values.diag_error,
        UiRole::HttpPost | UiRole::LspMissing => color(SyntaxRole::KeywordControl),
        UiRole::LspDisabled => color(SyntaxRole::Comment),
        UiRole::SearchMatch | UiRole::PdfSearchMatch => alpha(values.diag_warn, 0.30),
        UiRole::PdfSearchMatchActive => alpha(values.diag_warn, 0.60),
        UiRole::ApiMockRoute | UiRole::TokenProperty | UiRole::MarkdownHeading => color(SyntaxRole::KeywordControl),
        UiRole::UnsavedMarker => values.modified_unsaved,
        UiRole::TokenPunctuation => color(SyntaxRole::Delimiter),
        UiRole::MarkdownQuote => color(SyntaxRole::Comment),
        UiRole::PdfText => values.fg,
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
    comment: [f32; 4],
    keyword: [f32; 4],
    keyword_control: [f32; 4],
    function: [f32; 4],
    string: [f32; 4],
    constant: [f32; 4],
    parameter: [f32; 4],
    class: [f32; 4],
    md_code: [f32; 4],
    terminal: [[f32; 4]; 16],
    terminal_bg: [f32; 4],
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
            comment: [0.384, 0.447, 0.643, 1.0],
            keyword: [0.545, 0.913, 0.992, 1.0],
            keyword_control: [1.0, 0.474, 0.776, 1.0],
            function: [0.313, 0.980, 0.482, 1.0],
            string: [0.945, 0.980, 0.549, 1.0],
            constant: [0.741, 0.576, 0.976, 1.0],
            parameter: [0.973, 0.584, 0.502, 1.0],
            class: [0.45, 0.85, 0.90, 1.0],
            md_code: [0.902, 0.714, 0.451, 1.0],
            terminal: [
                [0.10, 0.10, 0.10, 1.0], [0.95, 0.30, 0.30, 1.0],
                [0.30, 0.85, 0.30, 1.0], [0.90, 0.85, 0.20, 1.0],
                [0.30, 0.60, 1.00, 1.0], [0.90, 0.35, 0.90, 1.0],
                [0.20, 0.85, 0.85, 1.0], [0.90, 0.90, 0.90, 1.0],
                [0.45, 0.45, 0.45, 1.0], [1.00, 0.40, 0.40, 1.0],
                [0.40, 1.00, 0.40, 1.0], [1.00, 1.00, 0.40, 1.0],
                [0.50, 0.70, 1.00, 1.0], [1.00, 0.50, 1.00, 1.0],
                [0.40, 1.00, 1.00, 1.0], [1.00, 1.00, 1.00, 1.0],
            ],
            terminal_bg: [0.129, 0.133, 0.173, 1.0],
        },
        ThemeId::OneDark => ThemeValues {
            bg: rgb(0x282c34), fg: rgb(0xabb2bf), minimap_bg: rgb(0x21252b),
            line_num: rgb(0x636d83), selection: rgb(0x3e4451), modified_unsaved: rgb(0xc678dd), modified_saved: rgb(0x98c379),
            diag_warn: rgb(0xe5c07b), diag_error: rgb(0xe06c75), unused: [0.48, 0.48, 0.48, 0.6],
            surface_bg: rgb(0x2c313a), comment: rgb(0x7f848e), keyword: rgb(0xe5c07b), keyword_control: rgb(0xc678dd), function: rgb(0x61afef),
            string: rgb(0x98c379), constant: rgb(0xd19a66), parameter: rgb(0xe06c75),
            class: rgb(0xe5c07b), md_code: rgb(0xd19a66),
            terminal: [0x3f4451,0xe06c75,0x98c379,0xe5c07b,0x61afef,0xc678dd,0x56b6c2,0xd7dae0,0x5c6370,0xef7f88,0xa9d48a,0xf0cc8c,0x74bdf5,0xd38be6,0x6bc6d2,0xf0f2f5].map(rgb),
            terminal_bg: rgb(0x282c34),
        },
        ThemeId::Forest => ThemeValues {
            bg: rgb(0x2d353b), fg: rgb(0xd3c6aa), minimap_bg: rgb(0x272e33),
            line_num: rgb(0x7a8478), selection: rgb(0x475258), modified_unsaved: rgb(0xd699b6), modified_saved: rgb(0xa7c080),
            diag_warn: rgb(0xdbbc7f), diag_error: rgb(0xe67e80), unused: [0.48, 0.48, 0.48, 0.6],
            surface_bg: rgb(0x343f44), comment: rgb(0x859289), keyword: rgb(0x7fbbb3), keyword_control: rgb(0xe67e80), function: rgb(0xa7c080),
            string: rgb(0xdbbc7f), constant: rgb(0xd699b6), parameter: rgb(0xe69875),
            class: rgb(0x83c092), md_code: rgb(0xdbbc7f),
            terminal: [0x475258,0xe67e80,0xa7c080,0xdbbc7f,0x7fbbb3,0xd699b6,0x83c092,0xd3c6aa,0x859289,0xf0959a,0xb8d08f,0xe6c98e,0x93c9c1,0xe2a9c4,0x96d0a4,0xe8dcc0].map(rgb),
            terminal_bg: rgb(0x2d353b),
        },
        ThemeId::Sepia => ThemeValues {
            bg: rgb(0xf4ecd8), fg: rgb(0x5b4636), minimap_bg: rgb(0xebe1c8),
            line_num: rgb(0x9c8c74), selection: rgb(0xe3d3b0), modified_unsaved: rgb(0xa8467a), modified_saved: rgb(0x56701a),
            diag_warn: rgb(0x9a6a00), diag_error: rgb(0xb23a2a), unused: [0.45, 0.45, 0.45, 0.6],
            surface_bg: rgb(0xede3cb), comment: rgb(0x857560), keyword: rgb(0x2f6f7e), keyword_control: rgb(0xa0442c), function: rgb(0x56701a),
            string: rgb(0x86600c), constant: rgb(0x7a4e8c), parameter: rgb(0xa85a14),
            class: rgb(0x2e7562), md_code: rgb(0x86600c),
            terminal: [0x3b2e24,0xa0442c,0x56701a,0x86600c,0x2f6f7e,0x7a4e8c,0x2e7562,0x5b4636,0x8a7a62,0xb5512f,0x627f1f,0x9a6f10,0x2b7f92,0x8c5aa0,0x33856f,0x3b2e24].map(rgb),
            terminal_bg: rgb(0xf4ecd8),
        },
        ThemeId::OneLight => ThemeValues {
            bg: rgb(0xfafafa), fg: rgb(0x383a42), minimap_bg: rgb(0xeeeeef),
            line_num: rgb(0x9d9d9f), selection: rgb(0xdcdde3), modified_unsaved: rgb(0xa626a4), modified_saved: rgb(0x478f46),
            diag_warn: rgb(0xa96f00), diag_error: rgb(0xd84a3d), unused: [0.45, 0.45, 0.45, 0.6],
            surface_bg: rgb(0xf0f0f1), comment: rgb(0x8a8b92), keyword: rgb(0xa96f00), keyword_control: rgb(0xa626a4), function: rgb(0x4078f2),
            string: rgb(0x478f46), constant: rgb(0x986801), parameter: rgb(0xd84a3d),
            class: rgb(0xa96f00), md_code: rgb(0x986801),
            terminal: [0x383a42,0xd84a3d,0x478f46,0xa96f00,0x4078f2,0xa626a4,0x0184bc,0x383a42,0x84858c,0xc4433a,0x3f7f3e,0x956200,0x3366d6,0x921f90,0x0172a3,0x202227].map(rgb),
            terminal_bg: rgb(0xfafafa),
        },
    }
}

impl Theme {
    pub(crate) fn background_for_id(id: ThemeId) -> [f32; 4] {
        theme_values(id).bg
    }

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
            terminal_bg: [values.terminal_bg[0], values.terminal_bg[1], values.terminal_bg[2], 0.80],
            diff_deleted: [0.76, 0.78, 0.84, 0.24],
            diff_added_gutter: [diff_added[0], diff_added[1], diff_added[2], 0.95],
            diff_deleted_gutter: [0.76, 0.78, 0.84, 0.90],
            cursor_line: if id.is_dark() { [0.9, 0.9, 0.9, 0.12] } else { [values.fg[0], values.fg[1], values.fg[2], 0.12] },
            bracket_match: [0.6, 0.6, 0.6, 0.3],
            definition_underline: [0.545, 0.913, 0.992, 0.95],
            folded_keyword: [0.55, 0.62, 0.80, 1.0],
            markdown_code_bg: match id {
                ThemeId::Sepia => [0.902, 0.855, 0.741, 0.96],
                ThemeId::OneLight => [0.898, 0.898, 0.902, 0.96],
                _ => [0.11, 0.12, 0.15, 0.96],
            },
            markdown_quote_guide: [0.52, 0.46, 0.72, 0.72],
            markdown_check: [0.45, 0.86, 0.60, 1.0],
            sticky_shadow: [0.0, 0.0, 0.0, 1.0],
            terminal_search_bg: [0.18, 0.20, 0.22, 1.0],
            terminal_text_dim: [0.6, 0.6, 0.6, 1.0],
            terminal_cursor: if id.is_dark() { [1.0, 1.0, 1.0, 0.5] } else { [values.fg[0], values.fg[1], values.fg[2], 0.5] },
            terminal_close_hover: if id.is_dark() { [1.0, 1.0, 1.0, 1.0] } else { values.fg },
            rollback_hover: if id.is_dark() { [0.92, 0.96, 1.0, 1.0] } else { values.fg },
            markdown_copy_success: [0.3, 0.9, 0.4, 1.0],
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
        assert_eq!(theme.terminal_bg, [0.129, 0.133, 0.173, 0.80]);
        assert_eq!(theme.diff_deleted, [0.76, 0.78, 0.84, 0.24]);
        assert_eq!(theme.diff_added_gutter, [0.18, 0.82, 0.34, 0.95]);
        assert_eq!(theme.diff_deleted_gutter, [0.76, 0.78, 0.84, 0.90]);
        assert_eq!(theme.cursor_line, [0.9, 0.9, 0.9, 0.12]);
        assert_eq!(theme.bracket_match, [0.6, 0.6, 0.6, 0.3]);
        assert_eq!(theme.definition_underline, [0.545, 0.913, 0.992, 0.95]);
        assert_eq!(theme.folded_keyword, [0.55, 0.62, 0.80, 1.0]);
        assert_eq!(theme.markdown_code_bg, [0.11, 0.12, 0.15, 0.96]);
        assert_eq!(theme.markdown_quote_guide, [0.52, 0.46, 0.72, 0.72]);
        assert_eq!(theme.markdown_check, [0.45, 0.86, 0.60, 1.0]);
        assert_eq!(theme.sticky_shadow, [0.0, 0.0, 0.0, 1.0]);
        assert_eq!(theme.terminal_search_bg, [0.18, 0.20, 0.22, 1.0]);
        assert_eq!(theme.terminal_text_dim, [0.6, 0.6, 0.6, 1.0]);
        assert_eq!(theme.terminal_cursor, [1.0, 1.0, 1.0, 0.5]);
        assert_eq!(theme.terminal_close_hover, [1.0, 1.0, 1.0, 1.0]);
        assert_eq!(theme.rollback_hover, [0.92, 0.96, 1.0, 1.0]);
        assert_eq!(theme.markdown_copy_success, [0.3, 0.9, 0.4, 1.0]);
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
        assert_eq!(UiPalette::for_id(ThemeId::Dracula).syntax, SyntaxPalette::for_id(ThemeId::Dracula));
    }

    #[test]
    fn ui_palette_pick_preserves_dracula_argument_bits_and_stage_one_fields() {
        let ui = UiPalette::for_id(ThemeId::Dracula);
        let expected_roles = [
            [0.04, 0.05, 0.07, 1.0], [0.10, 0.11, 0.14, 0.98], [0.12, 0.13, 0.17, 1.0],
            [0.12, 0.13, 0.17, 1.0], [0.11, 0.12, 0.16, 0.98], [0.0, 0.0, 0.0, 0.42],
            [0.28, 0.30, 0.33, 1.0], [0.28, 0.24, 0.34, 1.0], [0.3, 0.27, 0.38, 1.0],
            [0.07, 0.073, 0.092, 1.0], [0.11, 0.12, 0.15, 0.96], [0.11, 0.12, 0.13, 1.0],
            [0.224, 0.231, 0.251, 1.0], [1.0, 1.0, 1.0, 1.0], [0.78, 0.80, 0.88, 1.0],
            [0.55, 0.57, 0.64, 1.0], [0.9, 0.9, 0.9, 1.0], [0.35, 0.26, 0.48, 1.0],
            [0.60, 0.35, 0.85, 1.0], [0.60, 0.35, 0.85, 0.24], [0.60, 0.35, 0.85, 0.24],
            [0.55, 0.36, 0.90, 0.36], [0.7, 0.33, 0.54, 0.8], [0.7, 0.33, 0.54, 1.0],
            [1.0, 1.0, 1.0, 1.0], [1.0, 0.38, 0.38, 1.0], [1.0, 0.7, 0.42, 1.0],
            [0.46, 0.82, 0.58, 1.0], [0.35, 0.75, 1.0, 1.0], [1.0, 0.58, 0.62, 1.0],
            [0.72, 0.52, 1.0, 1.0], [0.78, 0.68, 1.0, 1.0], [0.86, 0.90, 1.0, 1.0],
            [0.52, 0.82, 0.58, 1.0], [0.95, 0.42, 0.46, 1.0], [0.28, 0.24, 0.40, 1.0],
            [0.38, 0.62, 1.0, 1.0], [0.48, 0.82, 0.52, 1.0], [0.97, 0.76, 0.38, 1.0],
            [0.95, 0.42, 0.46, 1.0], [0.48, 0.74, 1.0, 1.0], [0.35, 0.75, 1.0, 1.0],
            [0.48, 0.86, 0.52, 1.0], [1.0, 0.76, 0.32, 1.0], [0.78, 0.58, 1.0, 1.0],
            [1.0, 0.42, 0.42, 1.0], [0.48, 0.86, 0.52, 1.0], [0.35, 0.75, 1.0, 1.0],
            [1.0, 0.42, 0.42, 1.0], [0.35, 0.85, 0.48, 1.0], [0.45, 0.47, 0.53, 1.0],
            [0.28, 0.85, 0.45, 1.0], [0.85, 0.75, 0.25, 1.0], [0.45, 0.45, 0.45, 1.0],
            [0.95, 0.45, 0.30, 1.0], [0.90, 0.30, 0.30, 1.0], [0.18, 0.20, 0.22, 1.0],
            [1.0, 0.85, 0.2, 0.35], [1.0, 0.55, 0.1, 0.55], [0.50, 0.90, 0.55, 1.0],
            [1.0, 0.67, 0.16, 1.0], [1.0, 0.68, 0.26, 1.0], [0.86, 0.87, 0.91, 1.0],
            [0.78, 0.75, 0.87, 1.0], [0.95, 0.93, 0.98, 1.0], [0.972, 0.972, 0.949, 1.0],
        ];
        assert_eq!(
            ui.roles.map(|color| color.map(f32::to_bits)),
            expected_roles.map(|color| color.map(f32::to_bits)),
        );
        for (index, role) in UiRole::ALL.into_iter().enumerate() {
            let color = [
                f32::from_bits(0x3e12_3456 + index as u32),
                f32::from_bits(0x3f23_4567 + index as u32),
                f32::from_bits(0x3d34_5678 + index as u32),
                f32::from_bits(0x3f45_6789 + index as u32),
            ];
            assert_eq!(ui.pick(role, color).map(f32::to_bits), color.map(f32::to_bits));
        }
        assert!(ui.is_dark);
        assert_eq!(ui.syntax, SyntaxPalette::for_id(ThemeId::Dracula));
        assert_eq!(ui.text.map(f32::to_bits), [1.0, 1.0, 1.0, 1.0].map(f32::to_bits));
        assert_eq!(ui.text_dim.map(f32::to_bits), [0.7, 0.7, 0.7, 1.0].map(f32::to_bits));
        assert_eq!(ui.bg_raised.map(f32::to_bits), [0.15, 0.16, 0.20, 1.0].map(f32::to_bits));
        assert_eq!(ui.border.map(f32::to_bits), [0.224, 0.231, 0.251, 1.0].map(f32::to_bits));
        assert_eq!(ui.accent.map(f32::to_bits), [0.35, 0.26, 0.48, 1.0].map(f32::to_bits));
        assert_eq!(ui.ink(0.37).map(f32::to_bits), [1.0, 1.0, 1.0, 0.37].map(f32::to_bits));
    }

    #[test]
    fn ui_palettes_are_theme_specific_and_meet_surface_contrast() {
        let surface_roles = [
            UiRole::BgPanel, UiRole::BgPanelAlt, UiRole::BgDialog, UiRole::BgInput,
            UiRole::BgTooltip, UiRole::RowHover, UiRole::RowActive, UiRole::BgChip,
            UiRole::BgCode, UiRole::BgMedia,
        ];
        let mut previous = UiPalette::for_id(ThemeId::Dracula);
        for id in ThemeId::ALL {
            let ui = UiPalette::for_id(id);
            assert_eq!(ui.syntax, SyntaxPalette::for_id(id));
            assert_eq!(ui.is_dark, id.is_dark());
            assert_eq!(ui.dracula, id == ThemeId::Dracula);
            if id != ThemeId::Dracula {
                assert_ne!(ui.roles, previous.roles, "{id:?} UI palette must be distinct");
            }
            for role in surface_roles {
                let bg = ui.roles[role as usize];
                assert!(contrast(ui.text, bg) >= 4.5, "{id:?} text/{role:?}: {:.2}", contrast(ui.text, bg));
                assert!(contrast(ui.text_dim, bg) >= 3.0, "{id:?} text_dim/{role:?}: {:.2}", contrast(ui.text_dim, bg));
            }
            previous = ui;
        }
    }

    #[test]
    fn editor_theme_values_match_spec_hex_literals() {
        // bg, surface, minimap, fg, line_num, selection, modified, saved, warn, error,
        // comment, keyword, keyword_control, function, string, constant, parameter, class, md_code.
        let expected = [
            [0x282c34, 0x2c313a, 0x21252b, 0xabb2bf, 0x636d83, 0x3e4451, 0xc678dd, 0x98c379, 0xe5c07b, 0xe06c75, 0x7f848e, 0xe5c07b, 0xc678dd, 0x61afef, 0x98c379, 0xd19a66, 0xe06c75, 0xe5c07b, 0xd19a66],
            [0x2d353b, 0x343f44, 0x272e33, 0xd3c6aa, 0x7a8478, 0x475258, 0xd699b6, 0xa7c080, 0xdbbc7f, 0xe67e80, 0x859289, 0x7fbbb3, 0xe67e80, 0xa7c080, 0xdbbc7f, 0xd699b6, 0xe69875, 0x83c092, 0xdbbc7f],
            [0xf4ecd8, 0xede3cb, 0xebe1c8, 0x5b4636, 0x9c8c74, 0xe3d3b0, 0xa8467a, 0x56701a, 0x9a6a00, 0xb23a2a, 0x857560, 0x2f6f7e, 0xa0442c, 0x56701a, 0x86600c, 0x7a4e8c, 0xa85a14, 0x2e7562, 0x86600c],
            [0xfafafa, 0xf0f0f1, 0xeeeeef, 0x383a42, 0x9d9d9f, 0xdcdde3, 0xa626a4, 0x478f46, 0xa96f00, 0xd84a3d, 0x8a8b92, 0xa96f00, 0xa626a4, 0x4078f2, 0x478f46, 0x986801, 0xd84a3d, 0xa96f00, 0x986801],
        ];
        for (id, row) in ThemeId::ALL[1..].iter().copied().zip(expected) {
            let theme = Theme::for_id(id, [0.0; 4]);
            let theme_fields = [
                theme.bg, theme.surface_bg, theme.minimap_bg, theme.fg, theme.line_num,
                theme.sel, theme.modified_unsaved, theme.modified_saved, theme.diag_warn, theme.diag_error,
            ];
            for (actual, hex) in theme_fields.into_iter().zip(row[..10].iter().copied()) {
                assert_eq!(actual, rgb(hex), "{:?} theme field #{hex:06x}", id);
            }

            let expected_roles = [
                rgb(row[3]), rgb(row[10]), rgb(row[11]), rgb(row[12]), rgb(row[13]), rgb(row[14]),
                rgb(row[15]), rgb(row[16]), rgb(row[17]), rgb(row[18]), rgb(row[10]), rgb(row[4]),
                rgb(row[3]), rgb(row[3]),
            ];
            assert_eq!(theme.syntax.colors, expected_roles, "{:?} syntax roles", id);
        }
    }

    #[test]
    fn terminal_panel_background_and_cursor_follow_editor_theme() {
        for (id, bg) in [
            (ThemeId::OneDark, rgb(0x282c34)),
            (ThemeId::Forest, rgb(0x2d353b)),
            (ThemeId::Sepia, rgb(0xf4ecd8)),
            (ThemeId::OneLight, rgb(0xfafafa)),
        ] {
            let theme = Theme::for_id(id, [0.0; 4]);
            assert_eq!(theme.terminal_bg, [bg[0], bg[1], bg[2], 0.80]);
            let cursor = if id.is_dark() {
                [1.0, 1.0, 1.0, 0.5]
            } else {
                [theme.fg[0], theme.fg[1], theme.fg[2], 0.5]
            };
            assert_eq!(theme.terminal_cursor, cursor);
            let close_hover = if id.is_dark() { [1.0, 1.0, 1.0, 1.0] } else { theme.fg };
            assert_eq!(theme.terminal_close_hover, close_hover);
        }
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
            let expected_ink = if id.is_dark() { [1.0, 1.0, 1.0, 0.4] } else { [ui.text[0], ui.text[1], ui.text[2], 0.4] };
            assert_eq!(ui.ink(0.4), expected_ink);
        }
        let light_ui = UiPalette { is_dark: false, text: [0.2, 0.3, 0.4, 0.9], ..UiPalette::for_id(ThemeId::OneLight) };
        assert_eq!(light_ui.ink(0.4), [0.2, 0.3, 0.4, 0.4]);
    }

}
