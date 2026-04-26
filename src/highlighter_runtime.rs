use super::*;

impl Highlighter {
    pub fn reset(&self, version: u64, text: String, ext: String) {
        let _ = self
            .tx
            .send(HighlighterMessage::Reset { version, text, ext });
    }

    pub fn apply_edits(
        &self,
        version: u64,
        edits: Vec<SyncEdit>,
        edit_start_byte: Option<usize>,
        edit_end_byte: Option<usize>,
    ) {
        if !edits.is_empty() {
            let _ = self.tx.send(HighlighterMessage::Edits {
                version,
                edits,
                edit_start_byte,
                edit_end_byte,
            });
        }
    }

    pub fn poll(&mut self, current_editor_version: u64) -> bool {
        let mut updated = false;
        while let Ok((ver, spans, completions, foldable_ranges, syntax_errors)) = self.rx.try_recv()
        {
            if ver >= self.current_version {
                self.current_version = ver;
                if ver == current_editor_version {
                    self.spans = spans;
                    self.completions = completions;
                    self.foldable_ranges = foldable_ranges;
                    self.syntax_errors = syntax_errors;
                    updated = true;
                }
            }
        }
        updated
    }

    /// Блокирует текущий поток (до `timeout`) ожидая первый результат для `version`.
    /// Возвращает `true` если результат получен и применён до таймаута.
    /// Используется при открытии файла, чтобы первый кадр уже содержал подсветку.
    pub fn wait_for_first_result(&mut self, version: u64, timeout: std::time::Duration) -> bool {
        let deadline = std::time::Instant::now() + timeout;
        loop {
            let now = std::time::Instant::now();
            if now >= deadline {
                return false;
            }
            let remaining = deadline - now;
            match self.rx.recv_timeout(remaining) {
                Ok((ver, spans, completions, foldable_ranges, syntax_errors)) => {
                    if ver >= self.current_version {
                        self.current_version = ver;
                    }
                    if ver == version {
                        self.spans = spans;
                        self.completions = completions;
                        self.foldable_ranges = foldable_ranges;
                        self.syntax_errors = syntax_errors;
                        // Дренируем оставшиеся ожидающие результаты
                        self.poll(version);
                        return true;
                    }
                    // Устаревший результат — ждём дальше
                }
                Err(_) => return false,
            }
        }
    }

    pub fn shift_insert(&mut self, offset: usize, len: usize, text_opt: Option<&str>) {
        let prev_offset = offset.saturating_sub(1);
        let mut predicted_color = DRACULA_FG;

        for span in &self.spans {
            if span.start <= prev_offset && span.end > prev_offset {
                predicted_color = span.color;
                break;
            }
        }

        if let Some(t) = text_opt {
            match t.trim() {
                "+" | "-" | "*" | "/" | "%" | "=" | "==" | "!=" | "<" | ">" | "<=" | ">=" | "&"
                | "|" | "^" | "~" | ":" => predicted_color = DRACULA_PINK,
                "0" | "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" => {
                    predicted_color = DRACULA_PURPLE
                }
                "." | "," | "(" | ")" | "[" | "]" | "{" | "}" => predicted_color = DRACULA_FG,
                "import" | "from" | "if" | "else" | "elif" | "for" | "while" | "return" | "def"
                | "class" | "let" | "const" | "fn" | "mut" | "pub" | "struct" | "impl"
                | "match" | "break" | "continue" | "in" | "as" | "await" | "async" | "yield"
                | "try" | "except" | "finally" | "raise" | "with" => predicted_color = DRACULA_PINK,
                "True" | "False" | "None" | "true" | "false" | "null" => {
                    predicted_color = DRACULA_PINK
                }
                "int" | "float" | "str" | "bool" | "String" => predicted_color = DRACULA_CYAN,
                "self" | "cls" => predicted_color = DRACULA_PURPLE,
                _ => {}
            }
        }

        let mut new_spans = Vec::new();
        for span in &mut self.spans {
            if span.start >= offset {
                span.start += len;
                span.end += len;
            } else if span.end > offset {
                let old_end = span.end;
                span.end = offset;

                new_spans.push(ColorSpan {
                    start: offset,
                    end: offset + len,
                    color: predicted_color,
                });

                new_spans.push(ColorSpan {
                    start: offset + len,
                    end: old_end + len,
                    color: span.color,
                });
            } else if span.end == offset {
                new_spans.push(ColorSpan {
                    start: offset,
                    end: offset + len,
                    color: predicted_color,
                });
            }
        }

        if !new_spans.is_empty() {
            self.spans.extend(new_spans);
            self.spans.sort_by_key(|s| s.start);
            let mut merged = Vec::new();
            if !self.spans.is_empty() {
                let mut current = self.spans[0].clone();
                for i in 1..self.spans.len() {
                    let next = &self.spans[i];
                    if next.start <= current.end {
                        if next.color == current.color {
                            current.end = current.end.max(next.end);
                        } else if next.end > current.end {
                            merged.push(current.clone());
                            current = next.clone();
                            current.start = current.start.max(merged.last().unwrap().end);
                        }
                    } else {
                        merged.push(current);
                        current = next.clone();
                    }
                }
                if current.start < current.end {
                    merged.push(current);
                }
            }
            self.spans = merged;
            self.spans.retain(|s| s.start < s.end);
        } else {
            self.spans.push(ColorSpan {
                start: offset,
                end: offset + len,
                color: predicted_color,
            });
        }
    }

    pub fn shift_delete(&mut self, offset: usize, len: usize) {
        let end_del = offset + len;
        for span in &mut self.spans {
            if span.start >= end_del {
                span.start -= len;
            } else if span.start > offset {
                span.start = offset;
            }
            if span.end >= end_del {
                span.end -= len;
            } else if span.end > offset {
                span.end = offset;
            }
        }
        self.spans.retain(|s| s.start < s.end);
    }
}
pub(super) fn get_bracket_color(depth: usize) -> [f32; 4] {
    if depth == 0 {
        return DRACULA_FG;
    }
    match 1 + (depth - 1) % 5 {
        1 => DRACULA_GREEN,
        2 => DRACULA_CYAN,
        3 => DRACULA_ORANGE,
        4 => DRACULA_YELLOW,
        5 => DRACULA_PURPLE,
        _ => DRACULA_FG,
    }
}

pub(super) fn flatten_spans(
    mut spans: Vec<ColorSpan>,
    len: usize,
    text: &str,
    byte_colors: &mut Vec<[f32; 4]>,
    error_ranges: &[(usize, usize)],
    apply_rainbow_brackets: bool,
    is_log_or_huge: bool,
) -> Vec<ColorSpan> {
    if spans.is_empty() && error_ranges.is_empty() && (is_log_or_huge || !apply_rainbow_brackets) {
        return vec![ColorSpan {
            start: 0,
            end: len,
            color: DRACULA_FG,
        }];
    }

    spans.sort_by_key(|s| std::cmp::Reverse(s.end - s.start));

    byte_colors.clear();
    byte_colors.resize(len, DRACULA_FG);

    for span in spans {
        for i in span.start..span.end.min(len) {
            byte_colors[i] = span.color;
        }
    }

    let text_bytes = text.as_bytes();

    for i in 0..len {
        let b = text_bytes[i];
        if byte_colors[i] == MARKER_INTERPOLATION {
            if b == b'{' || b == b'}' {
                byte_colors[i] = DRACULA_ORANGE;
            } else {
                byte_colors[i] = DRACULA_FG;
            }
        }
    }

    if apply_rainbow_brackets {
        let mut depth_round = 0usize;
        let mut depth_square = 0usize;
        let mut depth_curly = 0usize;

        for i in 0..len {
            if byte_colors[i] != DRACULA_COMMENT
                && (byte_colors[i] == DRACULA_FG
                    || byte_colors[i] == DRACULA_GREEN
                    || byte_colors[i] == DRACULA_CYAN
                    || byte_colors[i] == DRACULA_ORANGE
                    || byte_colors[i] == DRACULA_YELLOW
                    || byte_colors[i] == DRACULA_PURPLE)
            {
                match text_bytes[i] {
                    b'(' => {
                        byte_colors[i] = get_bracket_color(depth_round);
                        depth_round += 1;
                    }
                    b')' => {
                        if depth_round > 0 {
                            depth_round -= 1;
                        }
                        byte_colors[i] = get_bracket_color(depth_round);
                    }
                    b'[' => {
                        byte_colors[i] = get_bracket_color(depth_square);
                        depth_square += 1;
                    }
                    b']' => {
                        if depth_square > 0 {
                            depth_square -= 1;
                        }
                        byte_colors[i] = get_bracket_color(depth_square);
                    }
                    b'{' => {
                        byte_colors[i] = get_bracket_color(depth_curly);
                        depth_curly += 1;
                    }
                    b'}' => {
                        if depth_curly > 0 {
                            depth_curly -= 1;
                        }
                        byte_colors[i] = get_bracket_color(depth_curly);
                    }
                    _ => {}
                }
            }
        }
    }

    // The logic to restore colors for ranges with syntax errors was removed.
    // It was using stale byte offsets from before the edit, causing highlighting to shift.
    // Now, text with syntax errors will just use the default color until the syntax is valid again.

    let mut flat = Vec::new();
    if len == 0 {
        return flat;
    }

    let mut current_color = byte_colors[0];
    let mut start = 0;
    for i in 1..len {
        if byte_colors[i] != current_color {
            flat.push(ColorSpan {
                start,
                end: i,
                color: current_color,
            });
            start = i;
            current_color = byte_colors[i];
        }
    }
    flat.push(ColorSpan {
        start,
        end: len,
        color: current_color,
    });
    flat
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn highlighter_flatten_spans_overlays_colors_and_brackets_end_to_end() {
        let text = "fn call((x))";
        let mut byte_colors = Vec::new();
        let spans = vec![ColorSpan {
            start: 0,
            end: 2,
            color: DRACULA_PINK,
        }];

        let flat = flatten_spans(spans, text.len(), text, &mut byte_colors, &[], true, false);

        assert_eq!(flat.first().map(|span| span.color), Some(DRACULA_PINK));
        assert_eq!(byte_colors[0], DRACULA_PINK);
        let nested_open = text.find("((").unwrap() + 1;
        let nested_close = text.find("))").unwrap();
        assert_ne!(byte_colors[nested_open], DRACULA_FG);
        assert_ne!(byte_colors[nested_close], DRACULA_FG);
    }

    #[test]
    fn highlighter_flatten_spans_returns_plain_span_for_logs_without_input_spans() {
        let mut byte_colors = Vec::new();
        let flat = flatten_spans(Vec::new(), 4, "text", &mut byte_colors, &[], false, true);

        assert_eq!(flat.len(), 1);
        assert_eq!(flat[0].start, 0);
        assert_eq!(flat[0].end, 4);
        assert_eq!(flat[0].color, DRACULA_FG);
    }
}
