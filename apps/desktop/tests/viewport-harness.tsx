// Test-only browser surface. Geometry was produced by native CLI validate from
// the original synthetic model. Colored test values and rejected callbacks are
// explicitly fixtures, never scientific/native acceptance evidence.
import { useRef, useState, type PointerEvent as ReactPointerEvent } from 'react';
import { createRoot } from 'react-dom/client';
import * as THREE from 'three';
import SceneView from '../src/SceneView';
import { defaultDisplay, type NumericHandleRequest } from '../src/viewport/types';
import type { Model, ValidatedModel } from '../src/types';
import fixture from './viewport-fixture.json';
import fixtureB from './viewport-fixture-b.json';

const cameraSnapshot = () => [...document.querySelectorAll<HTMLElement>('[data-viewport]')].map(pane => ({ view: pane.dataset.viewport, identity: pane.dataset.cameraIdentity, state: pane.dataset.cameraState, projection: pane.dataset.cameraProjection }));
const tick = () => new Promise<void>(resolve => requestAnimationFrame(() => resolve()));

function Harness() {
  const [display, setDisplay] = useState(defaultDisplay()), [selection, setSelection] = useState<string[]>([]), [result, setResult] = useState(false), [command, setCommand] = useState({ sequence: 0, kind: 'extents' as const });
  const [candidate, setCandidate] = useState<Model | null>(null), [commits, setCommits] = useState<Model[]>([]), [numeric, setNumeric] = useState<NumericHandleRequest | null>(null), [message, setMessage] = useState('');
  const [alternative, setAlternative] = useState(false), [synthetic, setSynthetic] = useState<unknown>(null), candidateRef = useRef<Model | null>(null);
  const validated = (alternative ? fixtureB : fixture) as ValidatedModel;
  // These explicitly synthetic DOM pointer sequences supplement genuine input
  // tests. They exercise production event handlers without native acceptance.
  const syntheticPointer = async (e: ReactPointerEvent, view: string, operation: 'orbit' | 'pan' | 'escape') => {
    e.preventDefault(); const pane = document.querySelector<HTMLElement>(`[data-viewport="${view}"]`)!;
    const bounds = pane.getBoundingClientRect(), before = cameraSnapshot(); let x = bounds.left + bounds.width * 0.8, y = bounds.top + bounds.height * 0.8;
    if (operation === 'escape') {
      const state = JSON.parse(pane.dataset.cameraState!), camera = new THREE.OrthographicCamera(-6 * bounds.width / bounds.height, 6 * bounds.width / bounds.height, 6, -6, -5000, 5000);
      camera.position.set(state.position[0], state.position[2], -state.position[1]); camera.lookAt(state.target[0], state.target[2], -state.target[1]); camera.zoom = state.zoom; camera.updateProjectionMatrix(); camera.updateMatrixWorld();
      const p = new THREE.Vector3(validated.model.window.center_x_m, validated.model.window.sill_m + validated.model.window.height_m / 2, 0).project(camera);
      x = bounds.left + (p.x + 1) * bounds.width / 2; y = bounds.top + (1 - p.y) * bounds.height / 2;
    }
    const init = { bubbles: true, cancelable: true, pointerId: e.pointerId, pointerType: 'mouse', button: operation === 'escape' ? 0 : 2, buttons: operation === 'escape' ? 1 : 2, shiftKey: operation === 'pan', clientX: x, clientY: y };
    pane.dispatchEvent(new PointerEvent('pointerdown', init));
    pane.dispatchEvent(new PointerEvent('pointermove', { ...init, clientX: x + 32, clientY: y - 18 }));
    const previewBeforeEscape = candidateRef.current;
    if (operation === 'escape') pane.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true }));
    pane.dispatchEvent(new PointerEvent('pointerup', { ...init, buttons: 0, clientX: x + 32, clientY: y - 18 }));
    await tick(); await tick(); setSynthetic({ operation, view, before, after: cameraSnapshot(), previewBeforeEscape, candidateAfter: candidateRef.current });
  };
  return <main style={{ height: '100vh', display: 'flex', flexDirection: 'column', font: '12px Segoe UI' }}>
    <header style={{ display: 'flex', alignItems: 'center', gap: 8, height: 40, padding: '0 12px' }}><strong>TEST FIXTURE · no calculation</strong><button onClick={() => setSelection(['window'])}>Select window</button><button onClick={() => setSelection(['room'])}>Select room</button><button onClick={() => setSelection([])}>Clear selection</button><button onClick={() => setResult(!result)}>Toggle fixture result</button><button onClick={() => setDisplay({ ...display, mode: display.mode === 'analysis' ? 'wireframe' : 'analysis' })}>Toggle mode</button><button onClick={() => setDisplay({ ...display, grid_snap: !display.grid_snap, ortho: !display.ortho, object_snap: false })}>Toggle GridSnap Ortho</button><button onClick={() => setCommand({ sequence: command.sequence + 1, kind: 'extents' })}>Extents</button></header>
    <nav style={{ display: 'flex', flexWrap: 'wrap', gap: 8, padding: 4 }}><button onClick={() => setAlternative(!alternative)}>Toggle validated A/B geometry</button><button onClick={() => setDisplay({ ...display, cutaway: !display.cutaway })}>Toggle cutaway</button><button onClick={() => setDisplay({ ...display, objects: { ...display.objects, window: { ...display.objects.window, locked: !display.objects.window?.locked } } })}>Toggle window lock</button><button onClick={() => setDisplay({ ...display, objects: { ...display.objects, window: { ...display.objects.window, visible: display.objects.window?.visible === false } } })}>Toggle window visibility</button><button onPointerDown={e => void syntheticPointer(e, 'perspective', 'orbit')}>Synthetic RMB orbit</button><button onPointerDown={e => void syntheticPointer(e, 'perspective', 'pan')}>Synthetic Shift RMB pan</button><button onPointerDown={e => void syntheticPointer(e, 'top', 'orbit')}>Synthetic Top RMB</button><button disabled={display.maximized_view !== 'front' || selection.join() !== 'window'} onPointerDown={e => void syntheticPointer(e, 'front', 'escape')}>Synthetic preview Escape</button></nav>
    <div style={{ flex: 1, minHeight: 0 }}><SceneView documentId="test-fixture" validated={validated} lux={result ? fixture.sensor_grid.sensors.map((_, i) => i + 1) : null} scale={77} selection={selection} display={display} tool="select" viewCommand={command} onDisplayChange={setDisplay} onSelectionChange={setSelection} onPreviewCandidate={model => { candidateRef.current = model; setCandidate(model); }} onCommitCandidate={async model => { setCommits(previous => [...previous, model]); return false; }} onNumericHandle={setNumeric} onInteractionError={setMessage} /></div>
    <output id="test-state" style={{ height: 100, overflow: 'auto', whiteSpace: 'pre-wrap', font: '10px Consolas' }}>{JSON.stringify({ selection, display, alternative, candidate, commits, numeric, message, synthetic })}</output>
  </main>;
}
const reactRoot = createRoot(document.getElementById('root')!); reactRoot.render(<Harness />);
if (import.meta.hot) import.meta.hot.dispose(() => reactRoot.unmount());
