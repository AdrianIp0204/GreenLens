import { useEffect, useRef, useState } from 'react';
import { ViewportEngine, VIEW_IDS, type ViewportOverlay } from './viewport/ViewportEngine';
import { objectLabel } from './viewport/entities';
import type { SceneViewProps, ViewId } from './viewport/types';
import './viewport/viewport.css';

const titles = { perspective: 'Perspective', top: 'Top', front: 'Front', right: 'Right' };
const axes = { perspective: 'World XY · Z up', top: 'X → · Y ↑', front: 'X → · Z ↑', right: 'Y → · Z ↑' };
const initialOverlay: ViewportOverlay = { rectangle: null, chooser: null, hover: null, snap: null, preview: null };

export default function SceneView(props: SceneViewProps) {
  const root = useRef<HTMLDivElement>(null), panes = useRef<Partial<Record<ViewId, HTMLDivElement>>>({}), engine = useRef<ViewportEngine | null>(null);
  const latest = useRef(props); latest.current = props;
  const [error, setError] = useState(''), [overlay, setOverlay] = useState(initialOverlay);
  useEffect(() => {
    if (!root.current) return;
    try { engine.current = new ViewportEngine(root.current, panes.current as Record<ViewId, HTMLDivElement>, latest.current, setOverlay); }
    catch (failure) { setError(`WebGL2 is unavailable. Use the object list and sensor table. ${String(failure)}`); }
    return () => { engine.current?.dispose(); engine.current = null; };
  }, []);
  useEffect(() => { engine.current?.update(props); }, [props]);
  const activate = (id: ViewId) => props.onDisplayChange({ ...props.display, active_view: id });
  const maximize = (id: ViewId) => props.onDisplayChange({ ...props.display, active_view: id, maximized_view: props.display.maximized_view === id ? null : id });
  return <div ref={root} className={`gl-viewports ${props.display.maximized_view ? 'is-maximized' : ''}`} data-testid="persistent-viewports">
    <div className="gl-viewport-grid">
      {VIEW_IDS.map(id => <section key={id} className={`gl-viewport ${props.display.active_view === id ? 'is-active' : ''}`} style={{ display: props.display.maximized_view && props.display.maximized_view !== id ? 'none' : undefined }} aria-label={`${titles[id]} viewport`}>
        <header className="gl-viewport-title" onDoubleClick={() => maximize(id)} onClick={() => activate(id)}>
          <button onDoubleClick={e => e.preventDefault()} title="Double-click the title to maximize or restore">{titles[id]}<span aria-hidden="true">⌄</span></button>
          <span>{id === 'perspective' ? 'Perspective' : 'Parallel'} · {props.display.mode}</span>
          <button className="gl-maximize" aria-label={`${props.display.maximized_view === id ? 'Restore four views' : `Maximize ${titles[id]}`}`} onClick={e => { e.stopPropagation(); maximize(id); }}>{props.display.maximized_view === id ? '▦' : '□'}</button>
        </header>
        <div ref={node => { if (node) panes.current[id] = node; }} className="gl-viewport-pane" tabIndex={0} role="application" aria-label={`${titles[id]} modeling canvas. Selection mode: ${props.selectionMode ?? 'replace'}. Click geometry to select; choose an object if an overlap menu opens. Hold Shift while clicking to add, Ctrl while clicking to remove. Right drag ${id === 'perspective' ? 'orbits; Shift right drag pans' : 'pans'}. Wheel zooms. Arrow keys navigate, Shift arrows pan, plus or minus zoom.`} data-viewport={id}>
          <span className="gl-view-axes">{axes[id]} · m</span>
          {overlay.rectangle?.view === id && <div className={`gl-selection-rectangle ${overlay.rectangle.crossing ? 'is-crossing' : ''}`} style={{ left: overlay.rectangle.left, top: overlay.rectangle.top, width: overlay.rectangle.width, height: overlay.rectangle.height }} />}
          {overlay.snap?.view === id && <div className="gl-snap-marker" style={{ left: overlay.snap.x, top: overlay.snap.y }}><span>□</span><b>{overlay.snap.label}</b></div>}
          {overlay.chooser?.view === id && <div className="gl-pick-menu" role="menu" aria-label="Choose overlapping object" style={{ left: Math.min(overlay.chooser.x, (panes.current[id]?.clientWidth ?? 400) - 205), top: Math.min(overlay.chooser.y, (panes.current[id]?.clientHeight ?? 300) - 160) }} onPointerDown={e => e.stopPropagation()}>
            <strong>{overlay.chooser.ctrl ? 'Remove from selection' : overlay.chooser.shift ? 'Add to selection' : 'Choose object'}</strong>{overlay.chooser.ids.map(object => <button key={object} role="menuitem" onPointerEnter={() => engine.current?.hover(object)} onClick={e => { e.stopPropagation(); engine.current?.choose(object, e.shiftKey, e.ctrlKey || e.metaKey); }}>{objectLabel(object)}</button>)}
          </div>}
        </div>
      </section>)}
    </div>
    {overlay.preview && <div className="gl-drag-preview" role="status">{overlay.preview}</div>}
    {error && <div className="gl-viewport-error" role="alert">{error}</div>}
    {!props.validated && !error && <div className="gl-viewport-error">Open or create a validated document.</div>}
    {props.display.mode === 'analysis' && !props.lux && <div className="gl-analysis-empty" role={props.resultLoading?'status':undefined}>{props.resultLoading?'Reading verified result · geometry retained':'No matching completed result · geometry only'}</div>}
  </div>;
}
