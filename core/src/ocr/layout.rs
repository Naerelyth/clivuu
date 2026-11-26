//! Reading-order layout, proportional character boundary solver, BiDi, and homoglyph cleanup.
use super::model::{OcrBlock, OcrLine, RectF};

const BLOCK_GAP: f32 = 0.4;
const BLOCK_GAP_MAX: f32 = 0.7;
const BLOCK_OVERLAP: f32 = 0.5;
const BLOCK_ALIGN: f32 = 0.35;
const ROW_TOLERANCE: f32 = 0.6;
const ROW_SPLIT_GAP: f32 = 4.0;
const COLUMN_GUTTER: f32 = 6.0;

const WIDTH_WEIGHT: f32 = 1.0;
const MARK_WEIGHT: f32 = 50.0;

/// Group detection boxes sharing a visual baseline into left-to-right line crops.
pub fn row_groups(rects: &[RectF], divided: impl Fn(&RectF, &RectF) -> bool) -> Vec<Vec<usize>> {
    let n = rects.len();
    if n == 0 {
        return Vec::new();
    }
    let unit = median_of(rects.iter().map(|r| r.height));
    let mut by_y: Vec<usize> = (0..n).collect();
    by_y.sort_by(|&a, &b| rects[a].center_y().total_cmp(&rects[b].center_y()));

    let mut rows: Vec<Vec<usize>> = Vec::new();
    let mut row_center: Vec<f32> = Vec::new();
    for &i in &by_y {
        let cy = rects[i].center_y();
        if let Some(&c) = row_center.last()
            && (cy - c).abs() < ROW_TOLERANCE * unit
        {
            rows.last_mut().unwrap().push(i);
        } else {
            rows.push(vec![i]);
            row_center.push(cy);
        }
    }

    let mut groups: Vec<Vec<usize>> = Vec::with_capacity(rows.len());
    for mut row in rows {
        row.sort_by(|&a, &b| rects[a].left().total_cmp(&rects[b].left()));
        let mut current: Vec<usize> = Vec::new();
        let mut reach_box = 0;
        for i in row {
            if !current.is_empty()
                && (rects[i].left() - rects[reach_box].right() >= ROW_SPLIT_GAP * unit
                    || divided(&rects[reach_box], &rects[i]))
            {
                groups.push(std::mem::take(&mut current));
            }
            if current.is_empty() || rects[i].right() > rects[reach_box].right() {
                reach_box = i;
            }
            current.push(i);
        }
        if !current.is_empty() {
            groups.push(current);
        }
    }
    groups
}

/// Group recognized lines into reading-order blocks and multi-column reading sequence.
pub fn group_blocks(lines: &[OcrLine]) -> Vec<OcrBlock> {
    let n = lines.len();
    if n == 0 {
        return Vec::new();
    }
    let unit = median_height(lines);
    let mut sorted: Vec<usize> = (0..n).collect();
    sorted.sort_by(|&a, &b| cmp_reading(&lines[a].bounds, &lines[b].bounds));

    let mut uf = UnionFind::new(n);
    for (si, &i) in sorted.iter().enumerate() {
        let a = &lines[i].bounds;
        for &j in &sorted[si + 1..] {
            let b = &lines[j].bounds;
            let vgap = b.top() - a.bottom();
            if vgap > BLOCK_GAP_MAX * unit {
                break;
            }
            if vgap > BLOCK_GAP * unit {
                continue;
            }
            let overlap = a.right().min(b.right()) - a.left().max(b.left());
            let min_w = a.width.min(b.width).max(1.0);
            let left_aligned = (a.left() - b.left()).abs() < BLOCK_ALIGN * unit;
            if overlap > 0.0 && (overlap / min_w > BLOCK_OVERLAP || left_aligned) {
                uf.union(i, j);
            }
        }
    }

    let mut group_of: Vec<Option<usize>> = vec![None; n];
    let mut groups: Vec<Vec<usize>> = Vec::new();
    for i in 0..n {
        let root = uf.find(i);
        let g = *group_of[root].get_or_insert_with(|| {
            groups.push(Vec::new());
            groups.len() - 1
        });
        groups[g].push(i);
    }

    let blocks: Vec<OcrBlock> = groups
        .into_iter()
        .map(|mut idxs| {
            idxs.sort_by(|&a, &b| cmp_reading(&lines[a].bounds, &lines[b].bounds));
            let bounds = union_bounds(&idxs, lines);
            OcrBlock {
                lines: idxs,
                bounds,
            }
        })
        .collect();

    order_blocks(merge_touching(blocks, lines), unit)
}

fn merge_touching(blocks: Vec<OcrBlock>, lines: &[OcrLine]) -> Vec<OcrBlock> {
    let n = blocks.len();
    if n < 2 {
        return blocks;
    }
    let mut uf = UnionFind::new(n);
    let mut merged = false;
    for (i, first) in blocks.iter().enumerate() {
        let a = &first.bounds;
        for (j, second) in blocks.iter().enumerate().skip(i + 1) {
            let b = &second.bounds;
            let apart = a.right() <= b.left()
                || b.right() <= a.left()
                || a.bottom() <= b.top()
                || b.bottom() <= a.top();
            if !apart {
                uf.union(i, j);
                merged = true;
            }
        }
    }
    if !merged {
        return blocks;
    }
    let mut slot_of: Vec<Option<usize>> = vec![None; n];
    let mut out: Vec<OcrBlock> = Vec::new();
    for (i, block) in blocks.into_iter().enumerate() {
        let root = uf.find(i);
        match slot_of[root] {
            Some(s) => {
                let target = &mut out[s];
                target.lines.extend(block.lines);
                target.bounds = target.bounds.union(block.bounds);
            }
            None => {
                slot_of[root] = Some(out.len());
                out.push(block);
            }
        }
    }
    for block in &mut out {
        block
            .lines
            .sort_by(|&a, &b| cmp_reading(&lines[a].bounds, &lines[b].bounds));
    }
    merge_touching(out, lines)
}

fn order_blocks(blocks: Vec<OcrBlock>, unit: f32) -> Vec<OcrBlock> {
    let n = blocks.len();
    if n < 2 {
        return blocks;
    }
    let mut by_left: Vec<usize> = (0..n).collect();
    by_left.sort_by(|&a, &b| blocks[a].bounds.left().total_cmp(&blocks[b].bounds.left()));

    let gutter = COLUMN_GUTTER * unit;
    let mut column = vec![0usize; n];
    for w in 1..n {
        let prev = &blocks[by_left[w - 1]].bounds;
        let cur = &blocks[by_left[w]].bounds;
        let split = cur.left() - prev.left() > gutter && cur.left() > prev.right();
        column[by_left[w]] = column[by_left[w - 1]] + usize::from(split);
    }

    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by(|&a, &b| {
        column[a]
            .cmp(&column[b])
            .then(cmp_reading(&blocks[a].bounds, &blocks[b].bounds))
    });

    let mut slots: Vec<Option<OcrBlock>> = blocks.into_iter().map(Some).collect();
    order
        .into_iter()
        .map(|src| slots[src].take().expect("each block moved once"))
        .collect()
}

pub fn median_height(lines: &[OcrLine]) -> f32 {
    median_of(lines.iter().map(|l| l.bounds.height))
}

pub fn median_of(values: impl Iterator<Item = f32>) -> f32 {
    let mut v: Vec<f32> = values.collect();
    if v.is_empty() {
        return 1.0;
    }
    v.sort_by(f32::total_cmp);
    v[v.len() / 2].max(1.0)
}

pub fn cmp_reading(a: &RectF, b: &RectF) -> std::cmp::Ordering {
    a.top()
        .total_cmp(&b.top())
        .then(a.left().total_cmp(&b.left()))
}

/// Sort recognized lines top-to-bottom by visual row and left-to-right within each row,
/// avoiding raw `top()` jitter across horizontally aligned columns.
pub fn sort_lines_by_rows(lines: &mut Vec<OcrLine>) {
    if lines.len() < 2 {
        return;
    }
    let unit = median_height(lines);
    let mut by_cy: Vec<usize> = (0..lines.len()).collect();
    by_cy.sort_by(|&a, &b| {
        lines[a]
            .bounds
            .center_y()
            .total_cmp(&lines[b].bounds.center_y())
    });

    let mut rows: Vec<(f32, Vec<usize>)> = Vec::new();
    for idx in by_cy {
        let cy = lines[idx].bounds.center_y();
        if let Some((row_cy, members)) = rows.last_mut()
            && (cy - *row_cy).abs() < ROW_TOLERANCE * unit
        {
            let count = members.len() as f32;
            *row_cy = (*row_cy * count + cy) / (count + 1.0);
            members.push(idx);
        } else {
            rows.push((cy, vec![idx]));
        }
    }
    rows.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut slots: Vec<Option<OcrLine>> = std::mem::take(lines).into_iter().map(Some).collect();
    let mut ordered = Vec::with_capacity(slots.len());
    for (_, mut members) in rows {
        members.sort_by(|&a, &b| {
            let la = slots[a].as_ref().unwrap().bounds.left();
            let lb = slots[b].as_ref().unwrap().bounds.left();
            la.total_cmp(&lb)
        });
        for idx in members {
            if let Some(line) = slots[idx].take() {
                ordered.push(line);
            }
        }
    }
    *lines = ordered;
}

pub fn union_rect(a: Option<RectF>, b: RectF) -> Option<RectF> {
    Some(match a {
        Some(a) => a.union(b),
        None => b,
    })
}

fn union_bounds(idxs: &[usize], lines: &[OcrLine]) -> RectF {
    let (mut l, mut t, mut r, mut b) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for &i in idxs {
        let bb = &lines[i].bounds;
        l = l.min(bb.left());
        t = t.min(bb.top());
        r = r.max(bb.right());
        b = b.max(bb.bottom());
    }
    RectF::from_ltrb(l, t, r, b).unwrap_or(lines[idxs[0]].bounds)
}

struct UnionFind {
    parent: Vec<usize>,
}

impl UnionFind {
    fn new(n: usize) -> Self {
        Self {
            parent: (0..n).collect(),
        }
    }

    fn find(&mut self, x: usize) -> usize {
        let mut root = x;
        while self.parent[root] != root {
            root = self.parent[root];
        }
        let mut node = x;
        while self.parent[node] != root {
            let next = self.parent[node];
            self.parent[node] = root;
            node = next;
        }
        root
    }

    fn union(&mut self, a: usize, b: usize) {
        let (ra, rb) = (self.find(a), self.find(b));
        if ra != rb {
            self.parent[ra] = rb;
        }
    }
}

/// Solve monotonic character boundary X positions (`char_count + 1` values) combining
/// CTC step centers `norm` (`0..=1`) and proportional glyph advance widths.
pub fn char_boundaries(bounds: &RectF, text: &str, norm: &[f32]) -> Vec<f32> {
    let widths: Vec<f32> = text.chars().map(advance).collect();
    let n = widths.len();
    let (left, right) = (bounds.left(), bounds.right());
    if n == 0 {
        return vec![left, right];
    }

    let at = |k: usize| left + bounds.width * norm[k].clamp(0.0, 1.0);
    let total: f32 = widths.iter().sum();
    let span = if norm.len() == n && n > 1 {
        at(n - 1) - at(0)
    } else {
        0.0
    };
    if span <= 0.0 || total <= 0.0 {
        return modelled_boundaries(left, right, &widths);
    }
    let unit = span / (total - 0.5 * (widths[0] + widths[n - 1])).max(f32::EPSILON);

    let mut system = Tridiagonal::new(n + 1);
    for (k, &w) in widths.iter().enumerate() {
        if w == 0.0 {
            system.add(k, (-1.0, 1.0), 0.0, MARK_WEIGHT);
            continue;
        }
        system.add(k, (0.5, 0.5), at(k), 1.0);
        system.add(k, (-1.0, 1.0), w * unit, WIDTH_WEIGHT);
    }

    let mut xs = system.solve();
    for x in &mut xs {
        *x = x.clamp(left, right);
    }
    xs[0] = left;
    xs[n] = right;
    for k in 0..n {
        xs[k + 1] = xs[k + 1].max(xs[k]);
    }
    xs
}

pub fn modelled_boundaries(left: f32, right: f32, widths: &[f32]) -> Vec<f32> {
    if widths.is_empty() {
        return vec![left, right];
    }
    let unit = (right - left) / widths.iter().sum::<f32>().max(f32::EPSILON);
    let mut xs = Vec::with_capacity(widths.len() + 1);
    let mut x = left;
    xs.push(left);
    for w in widths {
        x += w * unit;
        xs.push(x);
    }
    if let Some(last) = xs.last_mut() {
        *last = right;
    }
    xs
}

pub fn advance(c: char) -> f32 {
    match c {
        'i' | 'j' | 'l' | 'I' | '|' | '.' | ',' | ':' | ';' | '!' | '\'' | '`' => 0.4,
        ' ' | 'f' | 'r' | 't' | '(' | ')' | '[' | ']' | '{' | '}' | '-' | '"' | '/' | '\\' => 0.6,
        'm' | 'w' | 'M' | 'W' | 'ш' | 'щ' | 'ж' | 'ы' | 'ю' | 'ф' => 1.5,
        'Ш' | 'Щ' | 'Ж' | 'Ы' | 'Ю' | 'Ф' | '—' | '№' => 1.6,
        _ if is_combining_mark(c) => 0.0,
        _ if c as u32 >= 0x2E80 => 2.0,
        _ if c.is_uppercase() => 1.2,
        _ => 1.0,
    }
}

pub fn is_combining_mark(c: char) -> bool {
    matches!(
        c as u32,
        0x0300..=0x036F
            | 0x0483..=0x0489
            | 0x0591..=0x05C7
            | 0x0610..=0x061A
            | 0x064B..=0x065F
            | 0x0670
            | 0x06D6..=0x06ED
            | 0x0E31
            | 0x0E34..=0x0E3A
            | 0x0E47..=0x0E4E
            | 0xFE20..=0xFE2F
    )
}

/// Whether `c` is a CJK ideograph, Kana, Hangul, or fullwidth CJK punctuation where
/// Tesseract's inter-token ASCII spaces are spurious.
pub fn is_cjk_no_space(c: char) -> bool {
    matches!(
        c as u32,
        0x2E80..=0x9FFF
            | 0xAC00..=0xD7AF
            | 0xF900..=0xFAFF
            | 0xFE30..=0xFE4F
            | 0xFF01..=0xFF60
            | 0x20000..=0x2FA1F
    )
}

struct Tridiagonal {
    diag: Vec<f32>,
    off: Vec<f32>,
    rhs: Vec<f32>,
}

impl Tridiagonal {
    fn new(len: usize) -> Self {
        Self {
            diag: vec![0.0; len],
            off: vec![0.0; len.saturating_sub(1)],
            rhs: vec![0.0; len],
        }
    }

    fn add(&mut self, k: usize, (u, v): (f32, f32), target: f32, weight: f32) {
        self.diag[k] += weight * u * u;
        self.diag[k + 1] += weight * v * v;
        self.off[k] += weight * u * v;
        self.rhs[k] += weight * u * target;
        self.rhs[k + 1] += weight * v * target;
    }

    fn solve(self) -> Vec<f32> {
        let len = self.diag.len();
        let mut c = vec![0.0f32; len];
        let mut d = vec![0.0f32; len];
        for i in 0..len {
            let (sub, cp, dp) = if i > 0 {
                (self.off[i - 1], c[i - 1], d[i - 1])
            } else {
                (0.0, 0.0, 0.0)
            };
            let denom = (self.diag[i] - sub * cp).max(1e-9);
            c[i] = self.off.get(i).map_or(0.0, |off| off / denom);
            d[i] = (self.rhs[i] - sub * dp) / denom;
        }
        let mut xs = vec![0.0f32; len];
        for i in (0..len).rev() {
            let next = if i + 1 < len { c[i] * xs[i + 1] } else { 0.0 };
            xs[i] = d[i] - next;
        }
        xs
    }
}

pub mod bidi {
    #[derive(Clone, Copy, PartialEq, Eq)]
    enum Class {
        Ltr,
        Rtl,
        Neutral,
        Mark,
    }

    fn class(c: char) -> Class {
        match c as u32 {
            0x0591..=0x05C7 | 0x0610..=0x061A | 0x064B..=0x065F | 0x0670 | 0x06D6..=0x06ED => {
                Class::Mark
            }
            0x0660..=0x0669 | 0x06F0..=0x06F9 => Class::Ltr,
            0x0590..=0x08FF | 0xFB1D..=0xFDFF | 0xFE70..=0xFEFF => Class::Rtl,
            _ if c.is_alphanumeric() => Class::Ltr,
            _ => Class::Neutral,
        }
    }

    pub fn is_rtl(text: &str) -> bool {
        text.chars().any(|c| class(c) == Class::Rtl)
    }

    pub fn to_logical(visual: &str) -> String {
        if !is_rtl(visual) {
            return visual.to_owned();
        }
        let chars: Vec<char> = visual.chars().collect();
        let mut units: Vec<(usize, usize, Class)> = Vec::new();
        for (i, &c) in chars.iter().enumerate() {
            match (class(c), units.last_mut()) {
                (Class::Mark, Some(unit)) => unit.1 = i + 1,
                (Class::Mark, None) => units.push((i, i + 1, Class::Neutral)),
                (cls, _) => units.push((i, i + 1, cls)),
            }
        }

        let mut runs: Vec<(usize, usize, bool)> = Vec::new();
        let mut i = 0;
        while i < units.len() {
            let (start, mut end, cls) = units[i];
            i += 1;
            if cls != Class::Ltr {
                runs.push((start, end, false));
                continue;
            }
            loop {
                let mut k = i;
                while k < units.len() && units[k].2 == Class::Neutral {
                    k += 1;
                }
                if k < units.len() && units[k].2 == Class::Ltr {
                    end = units[k].1;
                    i = k + 1;
                } else {
                    break;
                }
            }
            runs.push((start, end, true));
        }

        let mut out = String::with_capacity(visual.len());
        for &(start, end, ltr) in runs.iter().rev() {
            for &c in &chars[start..end] {
                out.push(if ltr { c } else { mirror(c) });
            }
        }
        out
    }

    fn mirror(c: char) -> char {
        match c {
            '(' => ')',
            ')' => '(',
            '[' => ']',
            ']' => '[',
            '{' => '}',
            '}' => '{',
            '<' => '>',
            '>' => '<',
            '«' => '»',
            '»' => '«',
            _ => c,
        }
    }
}

pub mod lookalikes {
    #[derive(Clone, Copy, PartialEq, Eq, Debug)]
    enum Script {
        Latin,
        Cyrillic,
        Greek,
    }

    const SCRIPTS: [Script; 3] = [Script::Latin, Script::Cyrillic, Script::Greek];

    const CYRILLIC: &[(char, char)] = &[
        ('А', 'A'),
        ('В', 'B'),
        ('Е', 'E'),
        ('К', 'K'),
        ('М', 'M'),
        ('Н', 'H'),
        ('О', 'O'),
        ('Р', 'P'),
        ('С', 'C'),
        ('Т', 'T'),
        ('Х', 'X'),
        ('а', 'a'),
        ('е', 'e'),
        ('о', 'o'),
        ('р', 'p'),
        ('с', 'c'),
        ('у', 'y'),
        ('х', 'x'),
    ];

    const CYRILLIC_TO_LATIN: &[(char, char)] = &[
        ('І', 'I'),
        ('Ї', 'Ï'),
        ('Ј', 'J'),
        ('Ѕ', 'S'),
        ('Ү', 'Y'),
        ('Ѵ', 'V'),
        ('Ԛ', 'Q'),
        ('Ԝ', 'W'),
        ('Ӏ', 'l'),
        ('ԁ', 'd'),
        ('һ', 'h'),
        ('і', 'i'),
        ('ї', 'ï'),
        ('ј', 'j'),
        ('ӏ', 'l'),
        ('ԛ', 'q'),
        ('ѕ', 's'),
        ('ү', 'y'),
        ('ѵ', 'v'),
        ('ԝ', 'w'),
    ];

    const FOREIGN: &[char] = &['Ӏ', 'ӏ', 'ԁ', 'Ԛ', 'ԛ', 'Ԝ', 'ԝ', 'Ѵ', 'ѵ'];

    const GREEK: &[(char, char)] = &[
        ('Α', 'A'),
        ('Β', 'B'),
        ('Ε', 'E'),
        ('Ζ', 'Z'),
        ('Η', 'H'),
        ('Ι', 'I'),
        ('Κ', 'K'),
        ('Μ', 'M'),
        ('Ν', 'N'),
        ('Ο', 'O'),
        ('Ρ', 'P'),
        ('Τ', 'T'),
        ('Υ', 'Y'),
        ('Χ', 'X'),
        ('ν', 'v'),
        ('ο', 'o'),
        ('ρ', 'p'),
    ];

    fn script(c: char) -> Option<Script> {
        match c {
            'A'..='Z' | 'a'..='z' | '\u{C0}'..='\u{2AF}' | '\u{1E00}'..='\u{1EFF}' => {
                Some(Script::Latin)
            }
            '\u{400}'..='\u{52F}' | '\u{1C80}'..='\u{1C8F}' | '\u{A640}'..='\u{A69F}' => {
                Some(Script::Cyrillic)
            }
            '\u{370}'..='\u{3FF}' | '\u{1F00}'..='\u{1FFF}' => Some(Script::Greek),
            _ => None,
        }
    }

    fn find(table: &[(char, char)], c: char) -> Option<char> {
        table.iter().find(|&&(own, _)| own == c).map(|&(_, l)| l)
    }

    fn find_back(table: &[(char, char)], latin: char) -> Option<char> {
        table
            .iter()
            .find(|&&(_, l)| l == latin)
            .map(|&(own, _)| own)
    }

    fn latin(c: char) -> Option<char> {
        match script(c)? {
            Script::Latin => Some(c),
            Script::Cyrillic => find(CYRILLIC, c).or_else(|| find(CYRILLIC_TO_LATIN, c)),
            Script::Greek => find(GREEK, c),
        }
    }

    fn write_in(c: char, to: Script) -> Option<char> {
        if c.is_numeric() {
            return Some(c);
        }
        if script(c) == Some(to) {
            return (!FOREIGN.contains(&c)).then_some(c);
        }
        let l = latin(c)?;
        match to {
            Script::Latin => Some(l),
            Script::Cyrillic => find_back(CYRILLIC, l),
            Script::Greek => find_back(GREEK, l),
        }
    }

    fn candidates(word: &[char]) -> Vec<Script> {
        SCRIPTS
            .into_iter()
            .filter(|&s| word.iter().all(|&c| write_in(c, s).is_some()))
            .collect()
    }

    fn decided(word: &[char]) -> Option<Script> {
        match candidates(word)[..] {
            [only] => Some(only),
            _ => None,
        }
    }

    pub fn fix(text: &str) -> String {
        let mut out: Vec<char> = text.chars().collect();
        let mut words: Vec<(usize, usize)> = Vec::new();
        let mut start = None;
        for (i, c) in out.iter().enumerate() {
            match (c.is_alphanumeric(), start) {
                (true, None) => start = Some(i),
                (false, Some(s)) => {
                    words.push((s, i));
                    start = None;
                }
                _ => {}
            }
        }
        if let Some(s) = start {
            words.push((s, out.len()));
        }

        let verdicts: Vec<Option<Script>> =
            words.iter().map(|&(s, e)| decided(&out[s..e])).collect();
        let neighbour = |i: usize| {
            (1..words.len()).find_map(|d| {
                let left = i.checked_sub(d).and_then(|j| verdicts[j]);
                let right = verdicts.get(i + d).copied().flatten();
                left.or(right)
            })
        };

        for (i, &(s, e)) in words.iter().enumerate() {
            let word = &out[s..e];
            let cands = candidates(word);
            let target = match cands[..] {
                [] => None,
                [only] => Some(only),
                _ if word.iter().any(|c| c.is_numeric()) => Some(Script::Latin),
                [..] if e - s == 1 => None,
                _ => neighbour(i).filter(|s| cands.contains(s)),
            };
            if let Some(to) = target {
                for c in &mut out[s..e] {
                    *c = write_in(*c, to).unwrap_or(*c);
                }
            }
        }
        out.into_iter().collect()
    }
}
