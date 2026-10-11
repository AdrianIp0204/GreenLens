import type { SelectionMode } from '../viewport/types';

export default function SelectionBar({ mode, names, disabled, onMode, onObjects }: {
  mode: SelectionMode;
  names: string[];
  disabled: boolean;
  onMode: (mode: SelectionMode) => void;
  onObjects: () => void;
}) {
  const hints = {
    replace: 'Click an object. Hold Shift and click to add; hold Ctrl and click to remove.',
    add: 'Click an object to add it. If objects overlap, choose its name. Escape exits Add.',
    remove: 'Click a selected object to remove it from selection. Geometry is kept. Escape exits Remove.',
  };
  return <div className="selection-bar" aria-label="Object selection">
    <div className="selection-controls" role="group" aria-label="Selection mode">
      <span>Selection</span>
      {(['replace', 'add', 'remove'] as const).map(value => <button key={value}
        aria-pressed={mode === value} disabled={disabled} onClick={() => onMode(value)}
        title={value === 'remove' ? 'Remove from selection; does not delete geometry' : `${value === 'add' ? 'Add to' : 'Replace'} selection with the next object you click`}>
        {value[0].toUpperCase() + value.slice(1)}
      </button>)}
      <button className="selection-objects" onClick={onObjects} title="Choose named objects in the Layers panel">Objects</button>
    </div>
    <output className="selection-summary" aria-live="polite" title={names.join(', ')}>
      <strong>{names.length} selected</strong><span>{names.length ? names.join(', ') : 'No objects'}</span>
    </output>
    <p className="selection-hint">{hints[mode]}</p>
  </div>;
}
