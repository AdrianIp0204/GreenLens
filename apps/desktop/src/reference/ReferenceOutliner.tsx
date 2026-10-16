import { useEffect, useMemo, useRef, useState } from 'react';
import type { ReferenceModel } from './types';
import { indexOccurrences, occurrenceAncestors, outlinerRows, OUTLINER_PAGE_SIZE } from './reference-outliner.mjs';
import './reference-outliner.css';

export type ReferenceOutlinerProps = {
  documentKey: string; reference: ReferenceModel; selectedOccurrenceId: string | null;
  onSelectOccurrence: (id: string | null) => void; onFrameSelected?: () => void; disabled?: boolean;
};

/** Active-source navigation only; historical mapping inspection is independently controlled. */
export default function ReferenceOutliner(props: ReferenceOutlinerProps) {
  const { reference, selectedOccurrenceId, disabled } = props;
  const identity = `${props.documentKey}:${reference.revision_sha256}`;
  const [navigation, setNavigation] = useState({ identity, query: '', page: 0, collapsed: new Set<string>() });
  const state = navigation.identity === identity ? navigation : { identity, query: '', page: 0, collapsed: new Set<string>() };
  const [reveal, setReveal] = useState<{ identity: string; id: string } | null>(null);
  const host = useRef<HTMLDivElement>(null);
  const index = useMemo(() => indexOccurrences(reference.occurrences), [reference]);
  const rows = useMemo(() => outlinerRows(index, state.query, state.collapsed), [index, state.query, state.collapsed]);
  const lastPage = Math.max(0, Math.ceil(rows.length / OUTLINER_PAGE_SIZE) - 1), page = Math.min(state.page, lastPage);
  const start = page * OUTLINER_PAGE_SIZE, shown = rows.slice(start, start + OUTLINER_PAGE_SIZE);
  const selected = selectedOccurrenceId ? index.byId.get(selectedOccurrenceId) : undefined;
  const ancestors = selected ? occurrenceAncestors(index, selected.id) : [];
  useEffect(() => {
    if (!reveal || reveal.identity !== identity || selected?.id !== reveal.id) return;
    const button = [...(host.current?.querySelectorAll<HTMLButtonElement>('button[data-occurrence-id]') ?? [])].find(item => item.dataset.occurrenceId === reveal.id);
    button?.focus({ preventScroll: true }); button?.scrollIntoView?.({ block: 'nearest' }); setReveal(null);
  }, [identity, reveal, selected?.id, rows, page]);
  const revealSelected = () => {
    if (!selected) return;
    const collapsed = new Set(state.collapsed); for (const row of ancestors) collapsed.delete(row.id);
    const visible = outlinerRows(index, '', collapsed), position = visible.findIndex(row => row.occurrence.id === selected.id);
    setNavigation({ identity, query: '', collapsed, page: Math.max(0, Math.floor(position / OUTLINER_PAGE_SIZE)) });
    setReveal({ identity, id: selected.id });
  };
  return <section className="reference-outliner" aria-label="Active reference hierarchy" onKeyDown={event => { if (event.key === 'Escape' && !(event.target as HTMLElement).closest('input, textarea, select')) { event.stopPropagation(); if (!disabled) props.onSelectOccurrence(null); } }}>
    <label>Search source hierarchy<input value={state.query} maxLength={256} onChange={event => setNavigation({ ...state, query: event.target.value, page: 0 })} placeholder="Name, ancestor, node or occurrence ID" /></label>
    <div className="reference-outliner-actions"><button disabled={!selected} onClick={revealSelected}>Reveal selected</button><button disabled={disabled || !selected?.bounds || !props.onFrameSelected} title={!selected?.bounds ? 'Select an occurrence with admitted geometric bounds.' : !props.onFrameSelected ? 'Framing is available in the viewport.' : 'Fit the active viewport to the accepted selection.'} onClick={() => { if (selected?.bounds && !disabled) props.onFrameSelected?.(); }}>Frame selected</button><button disabled={!selected || disabled} onClick={() => props.onSelectOccurrence(null)}>Clear active selection</button></div>
    {selected && <><nav aria-label="Selected occurrence breadcrumb"><ol>{ancestors.map(row => <li key={row.id}><button disabled={disabled} aria-current={row.id === selected.id ? 'location' : undefined} title={`${row.id} · node ${row.node_index}`} onClick={() => props.onSelectOccurrence(row.id)}>{row.label || '(unnamed)'}<small>node {row.node_index}</small></button></li>)}</ol></nav><p className="reference-outliner-facts">{selected.source_mesh_index === null ? 'Transform-only glTF node' : `Mesh occurrence · source mesh ${selected.source_mesh_index} · ${index.meshUses.get(selected.source_mesh_index)} occurrence(s) use this mesh`}{selected.mirrored ? ' · mirrored transform' : ''}. Source tags and authoring group/component types are not recorded by this importer.</p></>}
    {selectedOccurrenceId && !selected && <p role="status">The selected identity is absent from this active reference revision. Historical mapping inspection remains separate.</p>}
    <p aria-live="polite">Showing {rows.length ? start + 1 : 0}–{start + shown.length} of {rows.length.toLocaleString()} {state.query.trim() ? 'matching' : 'visible'} occurrences; {reference.occurrences.length.toLocaleString()} total. Search includes ancestors; collapse only changes this list.</p>
    <div ref={host} className="reference-outliner-list">
      <ul aria-label="Source hierarchy occurrences">{shown.map(({ occurrence: row, depth, hasChildren }) => <li key={row.id} style={{ paddingLeft: `${Math.min(depth, 8) * .6}rem` }}><div className="reference-outliner-row">{hasChildren ? <button className="reference-outliner-toggle" aria-expanded={Boolean(state.query.trim()) || !state.collapsed.has(row.id)} aria-label={`${state.collapsed.has(row.id) && !state.query.trim() ? 'Expand' : 'Collapse'} node ${row.node_index}`} disabled={Boolean(state.query.trim()) || !state.collapsed.has(row.id) && state.collapsed.size >= 1024} title={state.query.trim() ? 'Clear search to collapse branches.' : 'Collapse this display branch; geometry stays included.'} onClick={() => { const collapsed = new Set(state.collapsed); if (!collapsed.delete(row.id)) collapsed.add(row.id); setNavigation({ ...state, collapsed, page: 0 }); }}>{state.collapsed.has(row.id) && !state.query.trim() ? '+' : '−'}</button> : <span className="reference-outliner-leaf" aria-hidden="true">·</span>}<button className="reference-outliner-occurrence" data-occurrence-id={row.id} aria-pressed={selected?.id === row.id} disabled={disabled} title={`${row.id} · source path ${row.node_path.join('/')} · depth ${depth}`} onClick={() => props.onSelectOccurrence(row.id)}><span>{row.label || '(unnamed)'}</span><small>node {row.node_index} · {row.source_mesh_index === null ? 'transform' : `mesh ${row.source_mesh_index}`} · {row.id.slice(0, 12)}</small></button></div></li>)}</ul>
    </div>
    <div className="reference-outliner-actions"><button disabled={!page} onClick={() => setNavigation({ ...state, page: page - 1 })}>Previous occurrences</button><button disabled={page === lastPage} onClick={() => setNavigation({ ...state, page: page + 1 })}>Next occurrences</button><button disabled={!state.collapsed.size} onClick={() => setNavigation({ ...state, collapsed: new Set(), page: 0 })}>Expand all branches</button></div>
    {state.collapsed.size >= 1024 && <p role="status">The display cache holds at most 1,024 collapsed branches. Expand some branches before collapsing more.</p>}
    <p className="reference-outliner-facts">Browsing and framing do not change analytical inclusion, scope mappings or report views.</p>
  </section>;
}
