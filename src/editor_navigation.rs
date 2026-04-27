use super::*;

impl Editor {
    pub fn select_expand(&mut self) {
        let (start, end) = if let Some(anchor) = self.selection_anchor {
            (anchor.min(self.cursor), anchor.max(self.cursor))
        } else {
            (self.cursor, self.cursor)
        };

        let mut candidates = Vec::new();

        let mut word_l = start;
        while word_l > 0 && char_class(self.byte_at(word_l - 1)) == 1 {
            word_l -= 1;
        }
        let mut word_r = end;
        while word_r < self.len() && char_class(self.byte_at(word_r)) == 1 {
            word_r += 1;
        }
        if word_l < word_r {
            candidates.push((word_l, word_r));
        }

        let mut line_l = start;
        while line_l > 0 && self.byte_at(line_l - 1) != b'\n' {
            line_l -= 1;
        }
        let mut line_r = end;
        while line_r < self.len() && self.byte_at(line_r) != b'\n' {
            line_r += 1;
        }
        candidates.push((line_l, line_r));

        let mut t_line_l = line_l;
        while t_line_l < line_r
            && (self.byte_at(t_line_l) == b' ' || self.byte_at(t_line_l) == b'\t')
        {
            t_line_l += 1;
        }
        let mut t_line_r = line_r;
        while t_line_r > t_line_l
            && (self.byte_at(t_line_r - 1) == b' ' || self.byte_at(t_line_r - 1) == b'\t')
        {
            t_line_r -= 1;
        }
        if t_line_l < t_line_r {
            candidates.push((t_line_l, t_line_r));
        }

        let scan_start = start.saturating_sub(50000);
        let scan_end = (end + 50000).min(self.len());
        let mut round_stack = Vec::new();
        let mut curly_stack = Vec::new();
        let mut square_stack = Vec::new();

        for i in scan_start..scan_end {
            let b = self.byte_at(i);
            match b {
                b'(' => round_stack.push(i),
                b'{' => curly_stack.push(i),
                b'[' => square_stack.push(i),
                b')' => {
                    if let Some(open) = round_stack.pop() {
                        candidates.push((open + 1, i));
                        candidates.push((open, i + 1));
                    }
                }
                b'}' => {
                    if let Some(open) = curly_stack.pop() {
                        candidates.push((open + 1, i));
                        candidates.push((open, i + 1));
                    }
                }
                b']' => {
                    if let Some(open) = square_stack.pop() {
                        candidates.push((open + 1, i));
                        candidates.push((open, i + 1));
                    }
                }
                _ => {}
            }
        }

        let mut q_l = line_l;
        let mut last_double = None;
        let mut last_single = None;
        while q_l < line_r {
            if self.byte_at(q_l) == b'"' {
                if let Some(open) = last_double {
                    candidates.push((open + 1, q_l));
                    candidates.push((open, q_l + 1));
                    last_double = None;
                } else {
                    last_double = Some(q_l);
                }
            } else if self.byte_at(q_l) == b'\'' {
                if let Some(open) = last_single {
                    candidates.push((open + 1, q_l));
                    candidates.push((open, q_l + 1));
                    last_single = None;
                } else {
                    last_single = Some(q_l);
                }
            }
            q_l += 1;
        }

        candidates.push((0, self.len()));

        let mut best = None;
        let mut min_len = usize::MAX;

        for (l, r) in candidates {
            if l <= start && r >= end && (l < start || r > end) {
                let len = r - l;
                if len < min_len {
                    min_len = len;
                    best = Some((l, r));
                }
            }
        }

        if let Some((l, r)) = best {
            self.selection_anchor = Some(l);
            self.cursor = r;
        }
    }

    pub fn select_word(&mut self) {
        let mut start = self.cursor;
        let mut end = self.cursor;

        while start > 0 && !is_delimiter(self.byte_at(start - 1)) {
            start -= 1;
        }
        while end < self.len() && !is_delimiter(self.byte_at(end)) {
            end += 1;
        }

        if start != end {
            self.selection_anchor = Some(start);
            self.cursor = end;
        }
    }

    pub fn select_line(&mut self) {
        let mut start = self.cursor;
        let mut end = self.cursor;

        while start > 0 && self.byte_at(start - 1) != b'\n' {
            start -= 1;
        }
        while end < self.len() && self.byte_at(end) != b'\n' {
            end += 1;
        }

        self.selection_anchor = Some(start);
        self.cursor = end;
    }

    pub(super) fn is_char_boundary(&self, index: usize) -> bool {
        if index == 0 || index == self.len() {
            return true;
        }
        let b = self.byte_at(index);
        b < 128 || b >= 192
    }

    pub fn byte_at(&self, idx: usize) -> u8 {
        if idx < self.gap_start {
            self.data[idx]
        } else {
            self.data[self.gap_end + (idx - self.gap_start)]
        }
    }

    pub fn len(&self) -> usize {
        self.data.len() - (self.gap_end - self.gap_start)
    }

    /// Итерирует по символам логической строки `line_idx` без аллокации.
    /// `f(ch, utf16_units_before, pixel_x_accum)` — вызывается для каждого символа.
    /// Возвращает pixel_x у символа с данным utf16_col, или total_x если col за концом строки.
    #[inline]
    pub fn utf16_col_to_byte_advance<F>(&self, line_idx: usize, mut f: F)
    where
        F: FnMut(char, u32, usize), // (char, utf16_before, byte_offset_in_logical_text)
    {
        let start = self.line_offsets.get(line_idx).copied().unwrap_or(0);
        let end = self
            .line_offsets
            .get(line_idx + 1)
            .map(|&o| o.saturating_sub(1))
            .unwrap_or(self.len());
        let mut utf16: u32 = 0;
        let mut pos = start;
        while pos < end {
            let b = self.byte_at(pos);
            let char_len = if b < 0x80 {
                1
            } else if b < 0xE0 {
                2
            } else if b < 0xF0 {
                3
            } else {
                4
            };
            let mut buf = [0u8; 4];
            for k in 0..char_len {
                buf[k] = self.byte_at(pos + k);
            }
            if let Ok(s) = std::str::from_utf8(&buf[..char_len]) {
                if let Some(ch) = s.chars().next() {
                    f(ch, utf16, pos);
                    utf16 += ch.len_utf16() as u32;
                }
            }
            pos += char_len;
        }
    }

    pub fn get_selection(&self) -> Option<String> {
        let anchor = self.selection_anchor?;
        if anchor == self.cursor {
            return None;
        }
        let start = anchor.min(self.cursor);
        let end = anchor.max(self.cursor);
        let mut res = Vec::with_capacity(end - start);
        for i in start..end {
            res.push(self.byte_at(i));
        }
        Some(String::from_utf8_lossy(&res).into_owned())
    }

    pub fn get_visible_lines_count(&self) -> usize {
        let mut phys_line = 0;
        let mut lines_count = 0;
        if self.line_offsets.is_empty() {
            return 1;
        }
        while phys_line < self.line_offsets.len() {
            lines_count += 1;
            if self.folded_lines.contains(&phys_line)
                && self.foldable_lines.contains_key(&phys_line)
            {
                if let Some(&end_l) = self.foldable_lines.get(&phys_line) {
                    phys_line = end_l;
                }
            }
            phys_line += 1;
        }
        lines_count
    }

    pub fn select_all(&mut self) {
        self.selection_anchor = Some(0);
        self.cursor = self.len();
    }

    fn handle_selection(&mut self, shift: bool) {
        if !shift {
            self.selection_anchor = None;
        } else if self.selection_anchor.is_none() {
            self.selection_anchor = Some(self.cursor);
        }
    }

    /// ИСПРАВЛЕНИЕ: Новая универсальная логика обработки курсора, попадающего внутрь фолда.
    pub fn snap_cursor_out_of_fold(&mut self, old_cursor: usize) {
        let mut current_line = 0;
        while current_line < self.line_offsets.len() {
            if self.folded_lines.contains(&current_line)
                && self.foldable_lines.contains_key(&current_line)
            {
                let fold_end = self.foldable_lines[&current_line];
                let first_line_end = if current_line + 1 < self.line_offsets.len() {
                    self.line_offsets[current_line + 1]
                } else {
                    self.len()
                };
                let block_end = if fold_end + 1 < self.line_offsets.len() {
                    self.line_offsets[fold_end + 1]
                } else {
                    self.len()
                };

                // Если курсор оказался внутри свернутого кода
                if self.cursor > self.line_offsets[current_line] && self.cursor < block_end {
                    let cursor_on_first_line = self.cursor < first_line_end;

                    if !cursor_on_first_line {
                        if old_cursor >= block_end {
                            // Идем влево (Left) - перепрыгиваем до видимой части первой строки
                            self.cursor = self.line_offsets[current_line];
                            self.move_end(false); // до конца видимой строки
                        } else {
                            // Идем вправо (Right) или откуда-то еще - перепрыгиваем в конец блока
                            self.cursor = block_end;
                        }
                        return;
                    }
                }
                current_line = fold_end;
            }
            current_line += 1;
        }
    }

    pub fn move_left(&mut self, shift: bool) {
        self.handle_selection(shift);
        if self.cursor > 0 {
            let old_cursor = self.cursor;
            let mut prev = self.cursor - 1;
            while prev > 0 && !self.is_char_boundary(prev) {
                prev -= 1;
            }
            self.cursor = prev;
            self.snap_cursor_out_of_fold(old_cursor);
        }
    }

    pub fn move_right(&mut self, shift: bool) {
        self.handle_selection(shift);
        if self.cursor < self.len() {
            let old_cursor = self.cursor;
            let mut next = self.cursor + 1;
            while next < self.len() && !self.is_char_boundary(next) {
                next += 1;
            }
            self.cursor = next;
            self.snap_cursor_out_of_fold(old_cursor);
        }
    }

    pub fn move_word_left(&mut self, shift: bool) {
        self.handle_selection(shift);
        if self.cursor == 0 {
            return;
        }

        let old_cursor = self.cursor;
        let mut p = self.cursor;

        while p > 0 && char_class(self.byte_at(p - 1)) == 0 {
            p -= 1;
        }

        if p > 0 {
            let cls = char_class(self.byte_at(p - 1));
            while p > 0 && char_class(self.byte_at(p - 1)) == cls {
                p -= 1;
            }
        }
        self.cursor = p;
        self.snap_cursor_out_of_fold(old_cursor);
    }

    pub fn move_word_right(&mut self, shift: bool) {
        self.handle_selection(shift);
        if self.cursor == self.len() {
            return;
        }

        let old_cursor = self.cursor;
        let mut p = self.cursor;

        while p < self.len() && char_class(self.byte_at(p)) == 0 {
            p += 1;
        }

        if p < self.len() {
            let cls = char_class(self.byte_at(p));
            while p < self.len() && char_class(self.byte_at(p)) == cls {
                p += 1;
            }
        }
        self.cursor = p;
        self.snap_cursor_out_of_fold(old_cursor);
    }

    pub fn move_home(&mut self, shift: bool) {
        self.handle_selection(shift);

        let mut current_line = 0;
        let mut snapped = false;
        while current_line < self.line_offsets.len() {
            if self.folded_lines.contains(&current_line)
                && self.foldable_lines.contains_key(&current_line)
            {
                let fold_end = self.foldable_lines[&current_line];
                let start_byte = self.line_offsets[current_line];
                let block_end = if fold_end + 1 < self.line_offsets.len() {
                    self.line_offsets[fold_end + 1].saturating_sub(1)
                } else {
                    self.len()
                };

                if self.cursor > start_byte && self.cursor <= block_end {
                    self.cursor = start_byte;
                    snapped = true;
                    break;
                }
                current_line = fold_end;
            }
            current_line += 1;
        }

        if !snapped {
            let mut curr = self.cursor;
            while curr > 0 {
                let b = self.byte_at(curr - 1);
                if b == b'\n' {
                    break;
                }
                curr -= 1;
            }
            self.cursor = curr;
        }
    }

    pub fn move_end(&mut self, shift: bool) {
        self.handle_selection(shift);
        let mut curr = self.cursor;
        while curr < self.len() {
            let b = self.byte_at(curr);
            if b == b'\n' {
                break;
            }
            curr += 1;
        }
        self.cursor = curr;

        let mut current_line = 0;
        while current_line < self.line_offsets.len() {
            if self.folded_lines.contains(&current_line)
                && self.foldable_lines.contains_key(&current_line)
            {
                let fold_end = self.foldable_lines[&current_line];
                let first_line_end = if current_line + 1 < self.line_offsets.len() {
                    self.line_offsets[current_line + 1].saturating_sub(1)
                } else {
                    self.len()
                };
                let block_end = if fold_end + 1 < self.line_offsets.len() {
                    self.line_offsets[fold_end + 1].saturating_sub(1)
                } else {
                    self.len()
                };

                // При нажатии End на первой строке фолда — перепрыгиваем в самый конец свернутого блока
                if self.cursor == first_line_end {
                    self.cursor = block_end;
                    break;
                }
                current_line = fold_end;
            }
            current_line += 1;
        }
    }

    pub fn move_start_of_file(&mut self, shift: bool) {
        self.handle_selection(shift);
        self.cursor = 0;
    }

    pub fn move_end_of_file(&mut self, shift: bool) {
        self.handle_selection(shift);
        self.cursor = self.len();
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    pub fn move_up(&mut self, renderer: &mut Renderer, shift: bool) {
        self.handle_selection(shift);
        let (x, y) = renderer.get_cursor_xy(self);
        self.cursor = renderer.get_byte_at_xy(self, x, y - renderer.line_height * 1.5);
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    pub fn move_down(&mut self, renderer: &mut Renderer, shift: bool) {
        self.handle_selection(shift);
        let (x, y) = renderer.get_cursor_xy(self);
        self.cursor = renderer.get_byte_at_xy(self, x, y + renderer.line_height * 0.5);
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    pub fn set_cursor_at_pos(
        &mut self,
        target_x: f32,
        target_y: f32,
        renderer: &mut Renderer,
        is_click: bool,
    ) {
        let target_y = (target_y - renderer.line_height * 0.5).max(0.0);
        let idx = renderer.get_byte_at_xy(self, target_x, target_y);
        if is_click {
            self.selection_anchor = Some(idx);
        }
        self.cursor = idx;
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    pub fn move_page_up(&mut self, renderer: &mut Renderer, shift: bool, step: f32) {
        self.handle_selection(shift);
        let (x, y) = renderer.get_cursor_xy(self);
        self.cursor = renderer.get_byte_at_xy(self, x, y - step);
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    pub fn move_page_down(&mut self, renderer: &mut Renderer, shift: bool, step: f32) {
        self.handle_selection(shift);
        let (x, y) = renderer.get_cursor_xy(self);
        self.cursor = renderer.get_byte_at_xy(self, x, y + step);
    }
}
