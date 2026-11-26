use super::{Kind, Mark};
use serde::Serialize;

#[derive(Clone, Debug)]
struct Step {
    index: usize,
    before: Option<Mark>,
    after: Option<Mark>,
    group: Option<u64>,
    sequence: Option<(u32, u32)>,
}

/// A gesture records one operation. A grouped property run retains its first
/// before value and final after value. Marks share immutable point buffers.
#[derive(Clone, Debug)]
pub struct History {
    marks: Vec<Mark>,
    undo: Vec<Step>,
    redo: Vec<Step>,
    selected: Option<usize>,
    sequence_index: u32,
}
impl Default for History {
    fn default() -> Self {
        Self {
            marks: vec![],
            undo: vec![],
            redo: vec![],
            selected: None,
            sequence_index: 1,
        }
    }
}
#[derive(Clone, Copy, Debug, Serialize)]
pub struct HistoryState {
    pub can_undo: bool,
    pub can_redo: bool,
    pub selected: Option<usize>,
}
impl History {
    pub fn sequence_index(&self) -> u32 {
        self.sequence_index
    }
    pub fn set_sequence_index(&mut self, index: u32) {
        self.sequence_index = index.clamp(1, super::sequence::MAX_INDEX);
    }
    pub fn marks(&self) -> &[Mark] {
        &self.marks
    }
    pub fn selected(&self) -> Option<usize> {
        self.selected
    }
    pub fn state(&self) -> HistoryState {
        HistoryState {
            can_undo: !self.undo.is_empty(),
            can_redo: !self.redo.is_empty(),
            selected: self.selected,
        }
    }
    pub fn add(&mut self, mark: Mark) {
        self.record(self.marks.len(), None, Some(mark), None);
    }
    pub fn replace(&mut self, index: usize, mark: Mark, group: Option<u64>) {
        if let Some(before) = self.marks.get(index) {
            self.record(index, Some(before.clone()), Some(mark), group);
        }
    }
    pub fn remove(&mut self, index: usize) {
        if let Some(before) = self.marks.get(index) {
            self.record(index, Some(before.clone()), None, None);
        }
    }
    fn record(
        &mut self,
        index: usize,
        before: Option<Mark>,
        after: Option<Mark>,
        group: Option<u64>,
    ) {
        if before == after {
            return;
        }
        let number = |m: &Option<Mark>| {
            m.as_ref()
                .filter(|m| m.kind == Kind::Sequence)
                .map(|m| m.style.sequence.index)
        };
        let old = number(&before);
        let new = number(&after);
        let sequence = if old == new || before.is_some() && after.is_some() {
            None
        } else {
            let next = new.map_or_else(
                || {
                    let removed = old.unwrap();
                    if self.sequence_index == removed.saturating_add(1)
                        && self.marks.iter().rposition(|m| m.kind == Kind::Sequence) == Some(index)
                    {
                        removed
                    } else {
                        self.sequence_index
                    }
                },
                |n| n.saturating_add(1).min(super::sequence::MAX_INDEX),
            );
            Some((self.sequence_index, next))
        };
        let step = Step {
            index,
            before,
            after,
            group,
            sequence,
        };
        self.apply(&step, true);
        if let Some(previous) = self.undo.last_mut().filter(|p| {
            group.is_some()
                && self.redo.is_empty()
                && p.group == group
                && p.index == index
                && p.before.is_some()
                && step.before.is_some()
                && step.after.is_some()
        }) {
            previous.after = step.after;
            if let Some((before, after)) = step.sequence {
                previous.sequence = Some((previous.sequence.map_or(before, |s| s.0), after));
            }
            if previous.before == previous.after {
                if let Some((before, _)) = previous.sequence {
                    self.sequence_index = before;
                }
                self.undo.pop();
            }
        } else {
            self.undo.push(step);
        }
        self.redo.clear();
    }
    fn apply(&mut self, step: &Step, forward: bool) {
        if let Some((before, after)) = step.sequence {
            self.sequence_index = if forward { after } else { before };
        }
        let value = if forward { &step.after } else { &step.before };
        match value {
            None => {
                self.marks.remove(step.index);
                self.selected = None;
            }
            Some(mark) => {
                if (forward && step.before.is_none()) || (!forward && step.after.is_none()) {
                    self.marks.insert(step.index, mark.clone());
                } else {
                    self.marks[step.index] = mark.clone();
                }
                self.selected = Some(step.index);
            }
        }
    }
    pub fn undo(&mut self) {
        if let Some(step) = self.undo.pop() {
            self.apply(&step, false);
            self.redo.push(step);
        }
    }
    pub fn redo(&mut self) {
        if let Some(step) = self.redo.pop() {
            self.apply(&step, true);
            self.undo.push(step);
        }
    }
    pub fn clear(&mut self) {
        *self = Self::default();
    }
}
