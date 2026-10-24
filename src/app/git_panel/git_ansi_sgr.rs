use ronsole_term::vte::Params;

/// Applies one CSI SGR sequence to the git commit panel's u8 colour state.
///
/// Colours are palette indices (`u8`), not theme RGB: the git panel keeps its own
/// colour mapping, so this parser stays independent of `ronsole_term::Term`.
pub(crate) fn apply_ansi_sgr(
    params: &Params,
    fg: &mut Option<u8>,
    bold: &mut bool,
    default_fg: Option<u8>,
    mut bg: Option<&mut u8>,
    default_bg: u8,
) {
    if params.is_empty() {
        *fg = default_fg;
        *bold = false;
        if let Some(bg) = bg.as_deref_mut() {
            *bg = default_bg;
        }
        return;
    }
    let values = params.iter().collect::<Vec<_>>();
    let mut index = 0usize;
    while index < values.len() {
        let Some(value) = values[index].first().copied() else {
            index += 1;
            continue;
        };
        match value {
            0 => {
                *fg = default_fg;
                *bold = false;
                if let Some(bg) = bg.as_deref_mut() {
                    *bg = default_bg;
                }
            }
            1 => {
                *bold = true;
                if let Some(color) = fg.as_mut()
                    && *color < 8
                {
                    *color += 8;
                }
            }
            22 => {
                *bold = false;
                if let Some(color) = fg.as_mut()
                    && (8..16).contains(color)
                {
                    *color -= 8;
                }
            }
            30..=37 => *fg = Some((value - 30) as u8 + if *bold { 8 } else { 0 }),
            40..=47 => {
                if let Some(bg) = bg.as_deref_mut() {
                    *bg = (value - 40) as u8;
                }
            }
            90..=97 => *fg = Some((value - 90 + 8) as u8),
            100..=107 => {
                if let Some(bg) = bg.as_deref_mut() {
                    *bg = (value - 100 + 8) as u8;
                }
            }
            39 => *fg = default_fg,
            49 => {
                if let Some(bg) = bg.as_deref_mut() {
                    *bg = default_bg;
                }
            }
            38 | 48 => {
                if index + 1 < values.len() && !values[index + 1].is_empty() {
                    let mode = values[index + 1][0];
                    if mode == 5 && index + 2 < values.len() && !values[index + 2].is_empty() {
                        let color = values[index + 2][0] as u8;
                        if value == 38 {
                            *fg = Some(color);
                        } else if let Some(bg) = bg.as_deref_mut() {
                            *bg = color;
                        }
                        index += 2;
                    } else if mode == 2 && index + 4 < values.len() {
                        index += 4;
                    } else {
                        index += 1;
                    }
                }
            }
            _ => {}
        }
        index += 1;
    }
}
