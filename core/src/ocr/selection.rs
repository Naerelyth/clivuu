//! Character-level interactive text selection shared across capture overlays and pins.
use super::{
    layout::bidi,
    model::{OcrBlock, OcrDocument, OcrLine, RectF},
};

const HIT_SLACK: f32 = 4.0;
const VERTICAL_SLACK: f32 = 10.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Caret {
    line: usize,
    ch: usize,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LineSelection {
    pub x: (f32, f32),
    pub y: (f32, f32),
}

#[derive(Clone, Debug, Default)]
pub struct OcrSelection {
    doc: Option<OcrDocument>,
    block_of: Vec<usize>,
    order: Vec<usize>,
    rank: Vec<usize>,
    rtl: Vec<bool>,
    sel: Option<(Caret, Caret)>,
    anchor_block: Option<usize>,
}

impl OcrSelection {
    pub fn set_document(&mut self, doc: OcrDocument) {
        let n = doc.lines.len();
        let mut block_of = vec![0usize; n];
        let mut order = Vec::with_capacity(n);
        for (bi, block) in doc.blocks.iter().enumerate() {
            for &li in &block.lines {
                if li < n {
                    block_of[li] = bi;
                    order.push(li);
                }
            }
        }
        if order.len() < n {
            for li in 0..n {
                if !order.contains(&li) {
                    order.push(li);
                }
            }
        }
        let mut rank = vec![0usize; n];
        for (pos, &li) in order.iter().enumerate() {
            rank[li] = pos;
        }
        let rtl = doc
            .lines
            .iter()
            .map(|line| bidi::is_rtl(&line.text))
            .collect();

        self.doc = Some(doc);
        self.block_of = block_of;
        self.order = order;
        self.rank = rank;
        self.rtl = rtl;
        self.sel = None;
        self.anchor_block = None;
    }

    pub fn clear(&mut self) {
        *self = Self::default();
    }

    pub fn document(&self) -> Option<&OcrDocument> {
        self.doc.as_ref()
    }

    pub fn lines(&self) -> &[OcrLine] {
        self.doc.as_ref().map_or(&[], |d| &d.lines)
    }

    pub fn blocks(&self) -> &[OcrBlock] {
        self.doc.as_ref().map_or(&[], |d| &d.blocks)
    }

    pub fn is_active(&self) -> bool {
        !self.lines().is_empty()
    }

    pub fn has_selection(&self) -> bool {
        self.sel.is_some_and(|(a, b)| a != b)
    }

    /// Hit-test a point in document coordinates; returns `Some(line_idx)` when over text.
    pub fn line_at(&self, point: (f64, f64)) -> Option<usize> {
        let (x, y) = (point.0 as f32, point.1 as f32);
        let (line, distance) = self.best_line(x, y, false, false)?;
        (distance <= HIT_SLACK).then_some(line)
    }

    pub fn begin_drag(&mut self, point: (f64, f64)) -> bool {
        let Some(caret) = self.caret_at(point, false) else {
            return self.deselect();
        };
        let next = Some((caret, caret));
        let changed = self.sel != next;
        self.anchor_block = self.block_of.get(caret.line).copied();
        self.sel = next;
        changed
    }

    pub fn extend_drag(&mut self, point: (f64, f64)) -> bool {
        let Some((anchor, _)) = self.sel else {
            return false;
        };
        let Some(focus) = self.caret_at(point, true) else {
            return false;
        };
        let next = Some((anchor, focus));
        if self.sel == next {
            return false;
        }
        self.sel = next;
        true
    }

    pub fn select_line(&mut self, line: usize) -> bool {
        if line >= self.lines().len() {
            return false;
        }
        self.anchor_block = self.block_of.get(line).copied();
        self.set_span(line, line)
    }

    pub fn select_block(&mut self, line: usize) -> bool {
        let Some(&bi) = self.block_of.get(line) else {
            return false;
        };
        let Some(block) = self.blocks().get(bi) else {
            return false;
        };
        let (Some(&first), Some(&last)) = (block.lines.first(), block.lines.last()) else {
            return false;
        };
        self.anchor_block = Some(bi);
        self.set_span(first, last)
    }

    pub fn select_all(&mut self) -> bool {
        self.anchor_block = None;
        let (Some(&first), Some(&last)) = (self.order.first(), self.order.last()) else {
            return false;
        };
        self.set_span(first, last)
    }

    pub fn deselect(&mut self) -> bool {
        self.anchor_block = None;
        self.sel.take().is_some()
    }

    pub fn line_selection(&self, i: usize) -> Option<LineSelection> {
        let (lo, hi) = self.span(self.sel, i)?;
        let line = self.lines().get(i)?;
        let left = line.char_x.get(lo).copied().unwrap_or(line.bounds.left());
        let right = line.char_x.get(hi).copied().unwrap_or(line.bounds.right());
        Some(LineSelection {
            x: (left.min(right), left.max(right)),
            y: (line.bounds.top(), line.bounds.bottom()),
        })
    }

    pub fn selected_rects(&self) -> Vec<RectF> {
        (0..self.lines().len())
            .filter_map(|i| {
                let s = self.line_selection(i)?;
                RectF::from_ltrb(s.x.0, s.y.0, s.x.1, s.y.1)
            })
            .collect()
    }

    pub fn selected_text(&self, smart_paragraphs: bool) -> Option<String> {
        let sel = self.sel.filter(|&(a, b)| a != b)?;
        let text = self.extract_text(Some(sel), smart_paragraphs);
        (!text.is_empty()).then_some(text)
    }

    pub fn all_text(&self, smart_paragraphs: bool) -> String {
        self.extract_text(None, smart_paragraphs)
    }

    pub fn selected_or_all_text(&self, smart_paragraphs: bool) -> String {
        let sel = self.sel.filter(|&(a, b)| a != b);
        self.extract_text(sel, smart_paragraphs)
    }

    fn set_span(&mut self, first: usize, last: usize) -> bool {
        let next = Some((
            Caret {
                line: first,
                ch: self.line_start(first),
            },
            Caret {
                line: last,
                ch: self.line_end(last),
            },
        ));
        if self.sel == next {
            return false;
        }
        self.sel = next;
        true
    }

    fn line_start(&self, line: usize) -> usize {
        if self.rtl.get(line).copied().unwrap_or(false) {
            self.lines()[line].char_count()
        } else {
            0
        }
    }

    fn line_end(&self, line: usize) -> usize {
        if self.rtl.get(line).copied().unwrap_or(false) {
            0
        } else {
            self.lines()[line].char_count()
        }
    }

    fn caret_at(&self, point: (f64, f64), confine: bool) -> Option<Caret> {
        let (x, y) = (point.0 as f32, point.1 as f32);
        let (line, _) = self.best_line(x, y, confine, confine)?;
        Some(Caret {
            line,
            ch: nearest_boundary(&self.lines()[line].char_x, x),
        })
    }

    fn best_line(&self, x: f32, y: f32, by_row: bool, confine: bool) -> Option<(usize, f32)> {
        let lines = self.lines();
        let blocks = self.blocks();
        if lines.is_empty() {
            return None;
        }
        let anchored = confine && self.anchor_block.is_some();
        let level_with_block = anchored
            && self
                .anchor_block
                .and_then(|bi| blocks.get(bi))
                .is_some_and(|b| {
                    y >= b.bounds.top() - VERTICAL_SLACK && y <= b.bounds.bottom() + VERTICAL_SLACK
                });

        let anchor_rank = self.sel.map(|(a, _)| self.rank[a.line]).unwrap_or(0);
        let mut best = None;
        let mut best_key = (f32::MAX, f32::MAX, usize::MAX);
        for (i, line) in lines.iter().enumerate() {
            let block = self.block_of[i];
            if anchored && Some(block) != self.anchor_block {
                let over_block = !level_with_block
                    && blocks.get(block).is_some_and(|b| {
                        let (bx, by) = gaps(x, y, &b.bounds);
                        bx <= VERTICAL_SLACK && by <= VERTICAL_SLACK
                    });
                if !over_block {
                    continue;
                }
            }
            let (dx, dy) = gaps(x, y, &line.bounds);
            let rank = self.rank[i].abs_diff(anchor_rank);
            let key = if by_row {
                (dy, dx, rank)
            } else {
                (dx.hypot(dy), 0.0, rank)
            };
            if key < best_key {
                best_key = key;
                best = Some(i);
            }
        }
        best.map(|i| (i, best_key.0))
    }

    fn extract_text(&self, sel: Option<(Caret, Caret)>, smart_paragraphs: bool) -> String {
        let lines = self.lines();
        let paragraph_break = if smart_paragraphs { "\n\n" } else { "\n" };
        let mut out = String::new();
        let mut prev_block: Option<usize> = None;
        for &li in &self.order {
            let segment = if sel.is_none() {
                bidi::to_logical(&lines[li].text)
            } else {
                let Some((lo, hi)) = self.span(sel, li) else {
                    continue;
                };
                let visual: String = lines[li].text.chars().skip(lo).take(hi - lo).collect();
                bidi::to_logical(&visual)
            };
            let block = self.block_of[li];
            match prev_block {
                Some(pb) if pb == block => out.push('\n'),
                Some(_) => out.push_str(paragraph_break),
                None => {}
            }
            out.push_str(&segment);
            prev_block = Some(block);
        }
        out
    }

    fn span(&self, sel: Option<(Caret, Caret)>, i: usize) -> Option<(usize, usize)> {
        let (a, b) = self.ordered(sel)?;
        let r = *self.rank.get(i)?;
        if r < self.rank[a.line] || r > self.rank[b.line] {
            return None;
        }
        let n = self.lines()[i].char_count();
        let (lo, hi) = if self.rtl.get(i).copied().unwrap_or(false) {
            (
                if i == b.line { b.ch } else { 0 },
                if i == a.line { a.ch } else { n },
            )
        } else {
            (
                if i == a.line { a.ch } else { 0 },
                if i == b.line { b.ch } else { n },
            )
        };
        (lo < hi).then_some((lo, hi))
    }

    fn ordered(&self, sel: Option<(Caret, Caret)>) -> Option<(Caret, Caret)> {
        let (a, b) = sel?;
        if self.caret_key(a) <= self.caret_key(b) {
            Some((a, b))
        } else {
            Some((b, a))
        }
    }

    fn caret_key(&self, c: Caret) -> (usize, usize) {
        let rank = self.rank.get(c.line).copied().unwrap_or(0);
        if self.rtl.get(c.line).copied().unwrap_or(false) {
            (rank, self.lines()[c.line].char_count().saturating_sub(c.ch))
        } else {
            (rank, c.ch)
        }
    }
}

fn gaps(x: f32, y: f32, r: &RectF) -> (f32, f32) {
    (
        (r.left() - x).max(x - r.right()).max(0.0),
        (r.top() - y).max(y - r.bottom()).max(0.0),
    )
}

fn nearest_boundary(char_x: &[f32], x: f32) -> usize {
    let mut best = 0;
    let mut best_d = f32::MAX;
    for (k, &bx) in char_x.iter().enumerate() {
        let d = (x - bx).abs();
        if d < best_d {
            best_d = d;
            best = k;
        }
    }
    best
}
