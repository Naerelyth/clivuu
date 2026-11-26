// Each completed gesture or property change is one reversible operation.
export const cloneMark = value => JSON.parse(JSON.stringify(value));
export class AnnotationHistory {
    constructor() { this.marks = []; this.undoSteps = []; this.redo = []; this.selected = -1; }
    get canUndo() { return this.undoSteps.length > 0; }
    get canRedo() { return this.redo.length > 0; }
    record(index, before, after, group = null) {
        if (JSON.stringify(before) === JSON.stringify(after)) return;
        const step = {index, before: before && cloneMark(before), after: after && cloneMark(after), group};
        const previous = this.undoSteps.at(-1);
        this._apply(step, after);
        if (group && !this.redo.length && previous?.group === group && previous.index === index && previous.before && before && after) {
            previous.after = step.after;
            // A wheel run that returns to its initial value is no operation.
            if (JSON.stringify(previous.before) === JSON.stringify(previous.after)) this.undoSteps.pop();
        } else this.undoSteps.push(step);
        this.redo.length = 0;
    }
    _apply(step, value) {
        if (value === null) this.marks.splice(step.index, 1);
        else if (step.before === null && value === step.after) this.marks.splice(step.index, 0, cloneMark(value));
        else if (step.after === null && value === step.before) this.marks.splice(step.index, 0, cloneMark(value));
        else this.marks[step.index] = cloneMark(value);
        this.selected = value === null ? -1 : step.index;
    }
    add(mark) { this.record(this.marks.length, null, mark); }
    replace(index, mark, group = null) { if (this.marks[index]) this.record(index, this.marks[index], mark, group); }
    remove(index) { if (this.marks[index]) this.record(index, this.marks[index], null); }
    undo() { const step = this.undoSteps.pop(); if (step) { this._apply(step, step.before); this.redo.push(step); } }
    restore() { const step = this.redo.pop(); if (step) { this._apply(step, step.after); this.undoSteps.push(step); } }
    clear() { this.marks.length = 0; this.redo.length = 0; this.undoSteps.length = 0; this.selected = -1; }
}
export function historyShortcut(key, ctrl, shift) {
    if (!ctrl) return null;
    if (key === 122 || key === 90) return shift ? 'redo' : 'undo';
    if (key === 121 || key === 89) return 'redo';
    return null;
}
