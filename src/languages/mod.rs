pub mod dart;
pub mod python;
pub mod rust;
pub mod sql;
pub mod sql_analysis;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ImportBlock {
    pub start: usize,
    pub end: usize,
    pub keyword_start: usize,
    pub keyword_end: usize,
    pub line_count: usize,
}

pub(crate) fn finish_import_block(
    current: &mut Option<ImportBlock>,
    blocks: &mut Vec<ImportBlock>,
) {
    if let Some(block) = current.take()
        && block.line_count >= 2
        && block.end > block.start
    {
        blocks.push(block);
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct PythonDelimiterState {
    paren_depth: i32,
    bracket_depth: i32,
    brace_depth: i32,
    quote: Option<u8>,
    triple_quoted: bool,
    escaped: bool,
}

impl PythonDelimiterState {
    pub(crate) fn scan_line(&mut self, line: &str) {
        let bytes = line.as_bytes();
        let mut index = 0usize;
        while index < bytes.len() {
            if let Some(quote) = self.quote {
                if self.escaped {
                    self.escaped = false;
                    index += 1;
                    continue;
                }
                if bytes[index] == b'\\' {
                    self.escaped = true;
                    index += 1;
                    continue;
                }
                if self.triple_quoted {
                    if bytes[index..].starts_with(&[quote, quote, quote]) {
                        self.quote = None;
                        self.triple_quoted = false;
                        index += 3;
                    } else {
                        index += 1;
                    }
                } else if bytes[index] == quote {
                    self.quote = None;
                    index += 1;
                } else {
                    index += 1;
                }
                continue;
            }

            match bytes[index] {
                b'#' => break,
                quote @ (b'\'' | b'"') => {
                    self.quote = Some(quote);
                    self.triple_quoted = bytes[index..].starts_with(&[quote, quote, quote]);
                    index += if self.triple_quoted { 3 } else { 1 };
                }
                b'(' => {
                    self.paren_depth += 1;
                    index += 1;
                }
                b')' => {
                    self.paren_depth -= 1;
                    index += 1;
                }
                b'[' => {
                    self.bracket_depth += 1;
                    index += 1;
                }
                b']' => {
                    self.bracket_depth -= 1;
                    index += 1;
                }
                b'{' => {
                    self.brace_depth += 1;
                    index += 1;
                }
                b'}' => {
                    self.brace_depth -= 1;
                    index += 1;
                }
                _ => index += 1,
            }
        }
    }

    pub(crate) fn has_open_delimiter(self) -> bool {
        self.paren_depth > 0 || self.bracket_depth > 0 || self.brace_depth > 0
    }

    #[cfg(test)]
    pub(crate) fn depths(self) -> (i32, i32, i32) {
        (self.paren_depth, self.bracket_depth, self.brace_depth)
    }
}

pub(crate) fn python_call_argument<'a>(text: &'a str, marker: &str) -> Option<&'a str> {
    let start = text.find(marker)? + marker.len();
    let bytes = text.as_bytes();
    let mut open = start;
    while bytes.get(open).is_some_and(u8::is_ascii_whitespace) {
        open += 1;
    }
    if bytes.get(open) != Some(&b'(') {
        return None;
    }
    let arg_start = open + 1;
    let mut depth = 1usize;
    let mut quote = None;
    let mut escaped = false;
    for i in arg_start..bytes.len() {
        let byte = bytes[i];
        if let Some(active) = quote {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == active {
                quote = None;
            }
            continue;
        }
        match byte {
            b'\'' | b'"' => quote = Some(byte),
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return text.get(arg_start..i).map(str::trim);
                }
            }
            b',' if depth == 1 => return text.get(arg_start..i).map(str::trim),
            _ => {}
        }
    }
    None
}

pub(crate) fn decode_python_string_literal(text: &str) -> Option<String> {
    let text = text.trim();
    let quote_at = text.find(['\'', '"'])?;
    if !text[..quote_at]
        .chars()
        .all(|ch| matches!(ch.to_ascii_lowercase(), 'r' | 'u' | 'b' | 'f'))
    {
        return None;
    }
    let raw = text[..quote_at].to_ascii_lowercase().contains('r');
    let quote = text.as_bytes()[quote_at];
    let triple = text.as_bytes().get(quote_at + 1) == Some(&quote)
        && text.as_bytes().get(quote_at + 2) == Some(&quote);
    let quote_len = if triple { 3 } else { 1 };
    let end = text.len().checked_sub(quote_len)?;
    if end < quote_at + quote_len || !text.as_bytes()[end..].iter().all(|byte| *byte == quote) {
        return None;
    }
    let content = &text[quote_at + quote_len..end];
    if raw {
        return Some(content.to_string());
    }
    let chars: Vec<char> = content.chars().collect();
    let mut out = String::with_capacity(content.len());
    let mut i = 0usize;
    while i < chars.len() {
        if chars[i] != '\\' {
            out.push(chars[i]);
            i += 1;
            continue;
        }
        i += 1;
        let Some(&escaped) = chars.get(i) else {
            out.push('\\');
            break;
        };
        let (digits, width) = match escaped {
            'x' => (16, 2),
            'u' => (16, 4),
            'U' => (16, 8),
            _ => (0, 0),
        };
        if width > 0 && i + width < chars.len() {
            let value = chars[i + 1..=i + width].iter().collect::<String>();
            if let Ok(value) = u32::from_str_radix(&value, digits)
                && let Some(ch) = char::from_u32(value)
            {
                out.push(ch);
                i += width + 1;
                continue;
            }
        }
        match escaped {
            'n' => out.push('\n'),
            'r' => out.push('\r'),
            't' => out.push('\t'),
            '\\' => out.push('\\'),
            '\'' => out.push('\''),
            '"' => out.push('"'),
            '0' => out.push('\0'),
            other => {
                out.push('\\');
                out.push(other);
            }
        }
        i += 1;
    }
    Some(out)
}

#[cfg(test)]
mod shared_language_regression_tests {
    use super::*;

    #[test]
    fn python_delimiter_state_ignores_brackets_in_strings_and_comments() {
        let mut state = PythonDelimiterState::default();
        state.scan_line(r#"value = ("escaped quote: \" )", next  # ] })"#);
        assert_eq!(state.depths(), (1, 0, 0));
        assert!(state.has_open_delimiter());
        state.scan_line(")");
        assert_eq!(state.depths(), (0, 0, 0));
        assert!(!state.has_open_delimiter());
    }

    #[test]
    fn python_delimiter_state_tracks_triple_quoted_multiline_strings() {
        let mut state = PythonDelimiterState::default();
        state.scan_line("value = (\"\"\"text ) # not syntax");
        assert_eq!(state.depths(), (1, 0, 0));
        state.scan_line("still ] } text\"\"\", next)");
        assert_eq!(state.depths(), (0, 0, 0));
    }

    #[test]
    fn python_literal_decoder_preserves_raw_regex_and_decodes_escapes() {
        assert_eq!(
            decode_python_string_literal(r#"r"\d+\s""#).as_deref(),
            Some(r"\d+\s")
        );
        assert_eq!(
            decode_python_string_literal(r#""a\n\x42\u263a""#).as_deref(),
            Some("a\nB☺")
        );
        assert!(decode_python_string_literal("not_a_string").is_none());
    }

    #[test]
    fn python_call_argument_ignores_closing_tokens_and_commas_inside_strings() {
        assert_eq!(
            python_call_argument(r#"Pattern("a)b,c")"#, "Pattern"),
            Some(r#""a)b,c""#)
        );
        assert_eq!(
            python_call_argument(
                "Field(default_factory=lambda: fn(1, 2), title='x')",
                "Field"
            ),
            Some("default_factory=lambda: fn(1, 2)")
        );
    }
}
