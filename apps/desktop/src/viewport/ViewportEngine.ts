import * as THREE from 'three';
import type { Model, Vec3 } from '../types';
import { luxColor } from '../presentation.mjs';
import { candidateForHandle, candidateForPlanarMove, convexHull, handleDefinitions, localToWorld, mergeSelection, nearestSnap, previewWireSegments, rectangleSelect, renderToWorld, snapCandidates, snapScalar, worldToLocal, worldToRender } from './interaction.mjs';
import { objectLabel, objectVisibility, semanticObjects, surfaceEntityId, surfaceLayer } from './entities';
import type { CameraState, SceneViewProps, SemanticObject, ViewId } from './types';

export const VIEW_IDS: ViewId[] = ['top', 'perspective', 'front', 'right'];
type Camera = THREE.PerspectiveCamera | THREE.OrthographicCamera;
type Handle = { id: string; axis: number; kind: 'move' | 'resize' | 'rotate'; label: string; path: string[]; multiplier: number; planar?: boolean };
interface View { id: ViewId; camera: Camera; target: THREE.Vector3; element: HTMLDivElement }
interface SurfaceNode { mesh: THREE.Mesh<THREE.BufferGeometry, THREE.MeshStandardMaterial>; edge: THREE.LineSegments<THREE.EdgesGeometry, THREE.LineBasicMaterial>; signature: string; entityId: string; material: string }
interface Drag { view: ViewId; start: THREE.Vector2; last: THREE.Vector2; button: number; shift: boolean; ctrl: boolean; handle?: Handle; original?: Model; candidate?: Model; origin?: THREE.Vector3; axis?: THREE.Vector3; screenAxis?: THREE.Vector2; unitsPerPixel?: number; plane?: THREE.Plane; startPoint?: THREE.Vector3; moved: boolean }
export interface ViewportOverlay { rectangle: null | { view: ViewId; left: number; top: number; width: number; height: number; crossing: boolean }; chooser: null | { view: ViewId; x: number; y: number; ids: string[]; shift: boolean; ctrl: boolean }; hover: string | null; snap: null | { view: ViewId; x: number; y: number; label: string }; preview: string | null }
const vector = (p: Vec3) => new THREE.Vector3(...worldToRender(p));
const world = (p: THREE.Vector3) => renderToWorld(p.toArray()) as Vec3;
const axisColors = [0xb6554c, 0x4b946c, 0x477bb4];
function fillPolygon(geometry: THREE.BufferGeometry, points: Vec3[]) {
  const coords: number[] = [];
  for (let i = 1; i < points.length - 1; i++) for (const p of [points[0], points[i], points[i + 1]]) coords.push(...worldToRender(p));
  const existing = geometry.getAttribute('position');
  if (existing instanceof THREE.BufferAttribute && existing.count === coords.length / 3) { existing.copyArray(coords); existing.needsUpdate = true; }
  else {
    // Release uploaded attributes before replacing buffers of a different size.
    if (existing) geometry.dispose();
    geometry.deleteAttribute('normal'); geometry.setAttribute('position', new THREE.Float32BufferAttribute(coords, 3));
  }
  geometry.computeVertexNormals(); geometry.computeBoundingSphere();
}
function release(object: THREE.Object3D) {
  object.traverse(child => { const item = child as THREE.Mesh; item.geometry?.dispose(); if (Array.isArray(item.material)) item.material.forEach(m => m.dispose()); else item.material?.dispose(); });
  object.removeFromParent();
}

// One renderer, four permanent cameras. Ordinary model/display updates never
// reconstruct the renderer or camera objects. Coordinates persisted by the app
// remain world ENU; Three.js alone uses the right-handed [x,z,-y] rotation.
export class ViewportEngine {
  private renderer: THREE.WebGLRenderer;
  private scene = new THREE.Scene();
  private surfaces = new Map<string, SurfaceNode>();
  private objects = new Map<string, SemanticObject>();
  private views = new Map<ViewId, View>();
  private grid = new THREE.Group();
  private handles = new THREE.Group();
  private ghost = new THREE.Group();
  private sensors: THREE.InstancedMesh | null = null;
  private cells: THREE.InstancedMesh | null = null;
  private plane: THREE.Mesh<THREE.BufferGeometry, THREE.MeshBasicMaterial>;
  private raycaster = new THREE.Raycaster();
  private observer: ResizeObserver;
  private props: SceneViewProps;
  private signature = '';
  private documentId: string | null = null;
  private awaitingFrame = true;
  private drag: Drag | null = null;
  private overlay: ViewportOverlay = { rectangle: null, chooser: null, hover: null, snap: null, preview: null };
  private handleKey = '';
  private lastCommand = -1;
  private pending = false;
  private disposers: (() => void)[] = [];
  private snapPoints: { position: number[]; kind: string; objectId: string }[] = [];

  constructor(private root: HTMLDivElement, elements: Record<ViewId, HTMLDivElement>, props: SceneViewProps, private onOverlay: (overlay: ViewportOverlay) => void) {
    this.props = props;
    this.renderer = new THREE.WebGLRenderer({ antialias: true, alpha: false });
    this.renderer.setPixelRatio(Math.min(devicePixelRatio, 2)); this.renderer.setScissorTest(true);
    this.renderer.domElement.setAttribute('aria-hidden', 'true');
    Object.assign(this.renderer.domElement.style, { position: 'absolute', inset: '0', width: '100%', height: '100%', pointerEvents: 'none' });
    root.prepend(this.renderer.domElement);
    this.renderer.domElement.dataset.rendererIdentity = crypto.randomUUID();
    this.scene.add(new THREE.HemisphereLight(0xffffff, 0x808a91, 2.4));
    const light = new THREE.DirectionalLight(0xffffff, 2); light.position.set(8, 15, 12); this.scene.add(light);
    this.plane = new THREE.Mesh(new THREE.BufferGeometry(), new THREE.MeshBasicMaterial({ color: 0x528e7a, transparent: true, opacity: 0.09, depthWrite: false, side: THREE.DoubleSide }));
    this.plane.userData.entityId = 'sensor-plane'; this.scene.add(this.plane, this.grid, this.handles, this.ghost);
    this.makeGrid();
    for (const id of VIEW_IDS) {
      const camera: Camera = id === 'perspective' ? new THREE.PerspectiveCamera(40, 1, 0.01, 10000) : new THREE.OrthographicCamera(-8, 8, 8, -8, -5000, 5000);
      if (id === 'top') camera.up.set(0, 0, -1); else camera.up.set(0, 1, 0);
      const view = { id, camera, target: new THREE.Vector3(), element: elements[id] }; this.views.set(id, view);
      const down = (e: PointerEvent) => this.pointerDown(view, e), move = (e: PointerEvent) => this.pointerMove(view, e), up = (e: PointerEvent) => this.pointerUp(view, e);
      const wheel = (e: WheelEvent) => { e.preventDefault(); this.activate(id); const factor = Math.exp(Math.max(-100, Math.min(100, e.deltaY)) * 0.0015); if (camera instanceof THREE.OrthographicCamera) camera.zoom = THREE.MathUtils.clamp(camera.zoom / factor, 0.02, 10000); else camera.position.sub(view.target).multiplyScalar(factor).clampLength(0.1, 10000).add(view.target); camera.updateProjectionMatrix(); this.saveCameras(); };
      const keyboard = (e: KeyboardEvent) => {
        if (e.target !== elements[id] || e.ctrlKey || e.altKey || e.metaKey) return;
        const step: Record<string, [number, number]> = { ArrowLeft: [-24, 0], ArrowRight: [24, 0], ArrowUp: [0, -24], ArrowDown: [0, 24] };
        if (step[e.key]) { e.preventDefault(); e.stopPropagation(); this.activate(id); this.navigate(view, new THREE.Vector2(...step[e.key]), e.shiftKey); this.saveCameras(); }
        else if (['+', '=', '-', '_'].includes(e.key)) { e.preventDefault(); e.stopPropagation(); this.activate(id); const factor = ['-', '_'].includes(e.key) ? 1.15 : 1 / 1.15; if (camera instanceof THREE.OrthographicCamera) camera.zoom = THREE.MathUtils.clamp(camera.zoom / factor, 0.02, 10000); else camera.position.sub(view.target).multiplyScalar(factor).clampLength(0.1, 10000).add(view.target); camera.updateProjectionMatrix(); this.saveCameras(); }
      };
      const cancelled = () => this.cancel();
      const context = (e: Event) => e.preventDefault();
      elements[id].addEventListener('pointerdown', down); elements[id].addEventListener('pointermove', move); elements[id].addEventListener('pointerup', up); elements[id].addEventListener('pointercancel', cancelled); elements[id].addEventListener('keydown', keyboard); elements[id].addEventListener('wheel', wheel, { passive: false }); elements[id].addEventListener('contextmenu', context);
      this.disposers.push(() => { elements[id].removeEventListener('pointerdown', down); elements[id].removeEventListener('pointermove', move); elements[id].removeEventListener('pointerup', up); elements[id].removeEventListener('pointercancel', cancelled); elements[id].removeEventListener('keydown', keyboard); elements[id].removeEventListener('wheel', wheel); elements[id].removeEventListener('contextmenu', context); });
    }
    const key = (e: KeyboardEvent) => { if (e.key === 'Escape' && !/INPUT|TEXTAREA|SELECT/.test((e.target as HTMLElement)?.tagName ?? '')) { if (this.drag || this.overlay.chooser) { e.preventDefault(); e.stopImmediatePropagation(); this.cancel(); } else this.props.onSelectionChange([]); } };
    window.addEventListener('keydown', key, true); this.disposers.push(() => window.removeEventListener('keydown', key, true));
    this.observer = new ResizeObserver(() => this.resize()); this.observer.observe(root); this.resize();
    this.update(props); this.renderer.setAnimationLoop(() => this.render());
  }
  private emit(patch: Partial<ViewportOverlay>) { this.overlay = { ...this.overlay, ...patch }; this.onOverlay(this.overlay); }
  private activate(id: ViewId) { if (this.props.display.active_view !== id) { this.props = { ...this.props, display: { ...this.props.display, active_view: id } }; this.props.onDisplayChange(this.props.display); } }
  private resize() { this.renderer.setSize(Math.max(1, this.root.clientWidth), Math.max(1, this.root.clientHeight), false); }
  private makeGrid() {
    const points: number[] = [];
    for (let n = -50; n <= 50; n++) { points.push(n, 0, -50, n, 0, 50, -50, 0, n, 50, 0, n); }
    const geometry = new THREE.BufferGeometry(); geometry.setAttribute('position', new THREE.Float32BufferAttribute(points, 3));
    this.grid.add(new THREE.LineSegments(geometry, new THREE.LineBasicMaterial({ color: 0xcdd2d7, transparent: true, opacity: 0.65, depthWrite: false })));
    const axes = new THREE.BufferGeometry().setFromPoints([new THREE.Vector3(-50, 0, 0), new THREE.Vector3(50, 0, 0), new THREE.Vector3(0, 0, -50), new THREE.Vector3(0, 0, 50)]);
    this.grid.add(new THREE.LineSegments(axes, new THREE.LineBasicMaterial({ color: 0x9ca8af, transparent: true, opacity: 0.8, depthWrite: false })));
    this.grid.position.y = -0.002;
  }
  update(props: SceneViewProps) {
    const changedDocument = this.documentId !== props.documentId;
    const changedEditContext = this.props.validated?.scientific_input_hash !== props.validated?.scientific_input_hash || this.props.selection.join('|') !== props.selection.join('|') || this.props.tool !== props.tool;
    this.props = props;
    if (changedDocument) { this.cancel(); this.documentId = props.documentId; this.awaitingFrame = true; for (const view of this.views.values()) if (props.display.cameras[view.id]) this.restoreCamera(view, props.display.cameras[view.id]!); }
    else if (this.drag?.handle && (changedEditContext || props.disabled || !this.state(props.selection[0]).visible || this.state(props.selection[0]).locked)) this.cancel();
    if (props.validated && this.signature !== props.validated.scientific_input_hash) { this.syncGeometry(); this.signature = props.validated.scientific_input_hash; }
    if (props.validated && this.awaitingFrame) { for (const id of VIEW_IDS) if (!props.display.cameras[id]) this.frame(id); this.awaitingFrame = false; }
    this.syncAppearance(); this.syncHandles();
    const command = props.viewCommand;
    if (command && command.sequence !== this.lastCommand) {
      this.lastCommand = command.sequence;
      if (command.kind === 'extents' || command.kind === 'selected') { this.frame(command.view ?? props.display.active_view, command.kind === 'selected'); this.saveCameras(); }
      else if (command.kind === 'four') props.onDisplayChange({ ...props.display, maximized_view: null });
      else props.onDisplayChange({ ...props.display, active_view: command.view ?? props.display.active_view, maximized_view: command.view ?? props.display.active_view });
    }
  }
  private restoreCamera(view: View, state: CameraState) {
    view.camera.position.copy(vector(state.position)); view.target.copy(vector(state.target));
    view.camera.zoom = state.zoom; if (view.id === 'perspective') view.camera.up.copy(vector(state.up));
    else this.fixOrthoDirection(view); view.camera.lookAt(view.target); view.camera.updateProjectionMatrix();
  }
  private fixOrthoDirection(view: View) {
    const distance = Math.max(20, view.camera.position.distanceTo(view.target));
    const direction = view.id === 'top' ? new THREE.Vector3(0, 1, 0) : view.id === 'front' ? new THREE.Vector3(0, 0, 1) : new THREE.Vector3(1, 0, 0);
    view.camera.position.copy(view.target).addScaledVector(direction, distance); view.camera.up.set(0, view.id === 'top' ? 0 : 1, view.id === 'top' ? -1 : 0);
  }
  private saveCameras() {
    const cameras: Partial<Record<ViewId, CameraState>> = {};
    for (const view of this.views.values()) cameras[view.id] = { position: world(view.camera.position), target: world(view.target), up: world(view.camera.up), zoom: view.camera.zoom };
    this.props.onDisplayChange({ ...this.props.display, cameras });
  }
  private syncGeometry() {
    const v = this.props.validated!;
    this.objects = new Map(semanticObjects(v).map(object => [object.id, object]));
    const live = new Set(v.surfaces.map(surface => surface.id));
    for (const [id, node] of this.surfaces) if (!live.has(id)) { release(node.mesh); release(node.edge); this.surfaces.delete(id); }
    for (const surface of v.surfaces) {
      const signature = JSON.stringify(surface.vertices);
      let node = this.surfaces.get(surface.id);
      if (!node) {
        const geometry = new THREE.BufferGeometry(); fillPolygon(geometry, surface.vertices);
        const material = new THREE.MeshStandardMaterial({ side: THREE.DoubleSide, roughness: 1 });
        node = { mesh: new THREE.Mesh(geometry, material), edge: new THREE.LineSegments(new THREE.EdgesGeometry(geometry), new THREE.LineBasicMaterial()), signature, entityId: surfaceEntityId(surface), material: surface.material };
        node.mesh.userData.entityId = node.entityId; this.scene.add(node.mesh, node.edge); this.surfaces.set(surface.id, node);
      } else if (node.signature !== signature) { fillPolygon(node.mesh.geometry, surface.vertices); node.edge.geometry.dispose(); node.edge.geometry = new THREE.EdgesGeometry(node.mesh.geometry); node.signature = signature; }
    }
    fillPolygon(this.plane.geometry, this.objects.get('sensor-plane')!.vertices);
    const grid = v.sensor_grid, count = grid.sensors.length;
    if (!this.sensors || this.sensors.count !== count) {
      if (this.sensors) release(this.sensors);
      this.sensors = new THREE.InstancedMesh(new THREE.SphereGeometry(1, 6, 4), new THREE.MeshBasicMaterial({ color: 0x428473 }), count); this.sensors.userData.sensorPoints = true; this.scene.add(this.sensors);
      if (this.cells) release(this.cells);
      this.cells = new THREE.InstancedMesh(new THREE.PlaneGeometry(1, 1), new THREE.MeshBasicMaterial({ side: THREE.DoubleSide, transparent: true, opacity: 0.95, depthWrite: false }), count); this.scene.add(this.cells);
    }
    const dummy = new THREE.Object3D(), radius = Math.min(grid.actual_spacing_x_m, grid.actual_spacing_y_m) * 0.075;
    grid.sensors.forEach((sensor, i) => {
      dummy.position.copy(vector(sensor.position)); dummy.scale.setScalar(radius); dummy.rotation.set(0, 0, 0); dummy.updateMatrix(); this.sensors!.setMatrixAt(i, dummy.matrix);
      dummy.scale.set(grid.actual_spacing_x_m * 0.96, grid.actual_spacing_y_m * 0.96, 1); dummy.rotation.set(-Math.PI / 2, 0, -v.model.room.rotation_deg * Math.PI / 180); dummy.updateMatrix(); this.cells!.setMatrixAt(i, dummy.matrix);
    });
    this.sensors!.instanceMatrix.needsUpdate = true; this.sensors!.computeBoundingSphere(); this.cells!.instanceMatrix.needsUpdate = true; this.cells!.computeBoundingSphere();
    // Face boundaries remain separate: concatenated context faces would invent
    // diagonal edges and midpoint snaps between unrelated polygons.
    this.handleKey = ''; this.snapPoints = snapCandidates([
      ...v.surfaces.map(surface => ({ id: surfaceEntityId(surface), vertices: surface.vertices })),
      this.objects.get('sensor-plane')!,
    ]);
  }
  private state(id: string) { const object = this.objects.get(id); return object ? objectVisibility(id, object.layer, this.props.display) : { visible: false, locked: true, color: '#aaa' }; }
  private selected(id: string) { return this.props.selection.includes(id) || (id.startsWith('surface:') && this.props.selection.includes('room')); }
  private syncAppearance() {
    const { display, lux, scale } = this.props;
    for (const node of this.surfaces.values()) {
      const state = this.state(node.entityId), selected = this.selected(node.entityId), hovered = this.overlay.hover === node.entityId;
      const ghosted = display.cutaway && (node.material === 'ceiling' || node.material === 'wall');
      node.mesh.visible = state.visible && display.mode !== 'wireframe'; node.edge.visible = state.visible;
      node.mesh.material.color.set(selected ? '#e6bf65' : hovered ? '#b3c7d1' : state.color);
      node.mesh.material.transparent = ghosted || node.material === 'glass'; node.mesh.material.opacity = node.material === 'glass' ? 0.22 : ghosted ? (display.mode === 'analysis' ? 0.08 : 0.22) : 1;
      node.mesh.material.depthWrite = !ghosted && node.material !== 'glass';
      node.edge.material.color.set(selected ? '#d59b26' : hovered ? '#377daf' : state.locked ? '#969da3' : '#4c5b66'); node.edge.material.transparent = true; node.edge.material.opacity = selected || hovered ? 1 : 0.7;
    }
    const plane = this.state('sensor-plane'); this.plane.visible = plane.visible && display.mode !== 'wireframe'; this.plane.material.opacity = this.selected('sensor-plane') ? 0.22 : 0.035;
    if (this.sensors && this.props.validated) {
      this.sensors.visible = display.layers.sensors.visible && display.objects['sensor-plane']?.visible !== false;
      const dummy = new THREE.Object3D(), grid = this.props.validated.sensor_grid, radius = Math.min(grid.actual_spacing_x_m, grid.actual_spacing_y_m) * 0.075;
      grid.sensors.forEach((sensor, i) => {
        const id = `sensor:${sensor.id}`, state = this.state(id), selected = this.selected(id), hover = this.overlay.hover === id;
        this.sensors!.setColorAt(i, new THREE.Color(selected ? '#e4a726' : hover ? '#469cd2' : state.color));
        dummy.position.copy(vector(sensor.position)); dummy.scale.setScalar(state.visible ? radius * (selected ? 1.8 : 1) : 0); dummy.updateMatrix(); this.sensors!.setMatrixAt(i, dummy.matrix);
        this.cells!.setColorAt(i, new THREE.Color(lux && lux.length === grid.sensors.length ? luxColor(lux[i], scale) : '#76968b'));
      });
      this.sensors.instanceMatrix.needsUpdate = true; if (this.sensors.instanceColor) this.sensors.instanceColor.needsUpdate = true;
      this.cells!.visible = display.mode === 'analysis' && display.layers.analysis.visible && lux !== null && lux.length === grid.sensors.length && !this.drag?.handle;
      if (this.cells!.instanceColor) this.cells!.instanceColor.needsUpdate = true;
    }
  }
  private frame(id: ViewId, selected = false) {
    const view = this.views.get(id)!;
    const candidates = [...this.objects.values()].filter(o => this.state(o.id).visible && (!selected || this.selected(o.id)));
    if (!candidates.length) { if (selected) this.props.onInteractionError?.('Select a visible object before Zoom Selected.'); return; }
    const bounds = new THREE.Box3().setFromPoints(candidates.flatMap(o => o.vertices.map(vector))), center = bounds.getCenter(new THREE.Vector3());
    const span = Math.max(...bounds.getSize(new THREE.Vector3()).toArray(), 0.6);
    const priorDirection = view.camera.position.clone().sub(view.target); view.target.copy(center);
    if (view.camera instanceof THREE.PerspectiveCamera) { const direction = priorDirection; if (direction.length() < 0.1 || this.awaitingFrame) direction.set(1, 0.8, 1.2); view.camera.position.copy(center).addScaledVector(direction.normalize(), span * 1.8); }
    else { const size = bounds.getSize(new THREE.Vector3()), aspect = Math.max(0.1, view.element.clientWidth / Math.max(1, view.element.clientHeight)); const projectedHeight = view.id === 'top' ? size.z : size.y, projectedWidth = view.id === 'right' ? size.z : size.x; const viewSpan = Math.max(projectedHeight, projectedWidth / aspect, 0.4); view.camera.position.copy(center).add(new THREE.Vector3(0, 0, span * 2)); view.camera.zoom = 12 / (viewSpan * 1.35); this.fixOrthoDirection(view); }
    view.camera.lookAt(center); view.camera.updateProjectionMatrix();
  }
  private project(point: number[], view: View): [number, number] | null {
    const p = vector(point as Vec3).project(view.camera); if (p.z < -1 || p.z > 1) return null;
    return [(p.x + 1) * view.element.clientWidth / 2, (1 - p.y) * view.element.clientHeight / 2];
  }
  private ray(view: View, p: THREE.Vector2) { this.raycaster.setFromCamera(new THREE.Vector2(p.x / view.element.clientWidth * 2 - 1, -p.y / view.element.clientHeight * 2 + 1), view.camera); }
  private point(e: PointerEvent, view: View) { const r = view.element.getBoundingClientRect(); return new THREE.Vector2(e.clientX - r.left, e.clientY - r.top); }
  private hitIds(view: View, p: THREE.Vector2) {
    this.ray(view, p);
    const meshes: THREE.Object3D[] = [...this.surfaces.values()].filter(n => this.state(n.entityId).visible).map(n => n.mesh);
    if (this.state('sensor-plane').visible) meshes.push(this.plane); if (this.sensors?.visible) meshes.push(this.sensors);
    const ids: string[] = [];
    for (const hit of this.raycaster.intersectObjects(meshes)) {
      const id = hit.object.userData.sensorPoints && hit.instanceId !== undefined ? `sensor:${this.props.validated!.sensor_grid.sensors[hit.instanceId].id}` : hit.object.userData.entityId as string;
      if (id && this.state(id).visible && !this.state(id).locked && !ids.includes(id)) ids.push(id);
    }
    // Envelope and generated surface are distinct inspectable semantic entities.
    if (ids.some(id => id.startsWith('surface:')) && this.state('room').visible && !this.state('room').locked) ids.push('room');
    return ids;
  }
  private handleHit(view: View, point: THREE.Vector2): Handle | undefined { this.ray(view, point); return this.raycaster.intersectObjects(this.handles.children, true).find(hit => hit.object.userData.handle)?.object.userData.handle; }
  private pointerDown(view: View, e: PointerEvent) {
    // Native canvas listeners run before React's delegated overlay handlers.
    // Let chooser controls receive their click without capturing it as geometry.
    if ((e.target as HTMLElement).closest('button,[role="menu"]')) return;
    if (e.button !== 0 && e.button !== 2 && e.button !== 1) return;
    e.preventDefault(); view.element.focus(); view.element.setPointerCapture(e.pointerId); this.activate(view.id);
    const p = this.point(e, view); this.emit({ chooser: null, rectangle: null, snap: null });
    const handle = e.button === 0 && !this.props.disabled && !this.pending ? this.handleHit(view, p) : undefined;
    this.drag = { view: view.id, start: p.clone(), last: p.clone(), button: e.button, shift: e.shiftKey, ctrl: e.ctrlKey || e.metaKey, moved: false };
    if (handle && this.props.validated) {
      const origin = this.handles.position.clone(), model = this.props.validated.model;
      if (handle.kind === 'rotate') {
        const plane = new THREE.Plane().setFromNormalAndCoplanarPoint(new THREE.Vector3(0, 1, 0), origin); this.ray(view, p);
        const startPoint = this.raycaster.ray.intersectPlane(plane, new THREE.Vector3());
        if (!startPoint) { this.drag = null; this.props.onInteractionError?.('The rotation plane is edge-on. Use Top or Perspective.'); return; }
        this.drag = { ...this.drag, handle, original: structuredClone(model), origin, plane, startPoint }; this.emit({ preview: `${handle.label} · angular preview; Escape cancels` }); return;
      }
      if (handle.planar) {
        const normal = this.props.selection[0] === 'window' ? vector(localToWorld([0, 1, 0], model.room.rotation_deg) as Vec3) : new THREE.Vector3(0, 1, 0);
        const plane = new THREE.Plane().setFromNormalAndCoplanarPoint(normal, origin); this.ray(view, p);
        const startPoint = this.raycaster.ray.intersectPlane(plane, new THREE.Vector3());
        if (!startPoint) { this.drag = null; this.props.onInteractionError?.('The edit plane is edge-on. Use Front or Perspective for a window, Top for context.'); return; }
        this.drag = { ...this.drag, handle, original: structuredClone(model), origin, plane, startPoint }; this.emit({ preview: `${handle.label} · planar preview; Ortho constrains the dominant axis` }); return;
      }
      const axis = vector(localToWorld([handle.axis === 0 ? 1 : 0, handle.axis === 1 ? 1 : 0, handle.axis === 2 ? 1 : 0], model.room.rotation_deg) as Vec3).normalize();
      const startScreen = this.project(world(origin), view), endScreen = this.project(world(origin.clone().add(axis)), view);
      if (!startScreen || !endScreen) { this.drag = null; this.props.onInteractionError?.('The handle is outside this camera clipping range. Use Zoom Selected.'); return; }
      const screenAxis = new THREE.Vector2(endScreen[0] - startScreen[0], endScreen[1] - startScreen[1]), pixels = screenAxis.length();
      if (pixels < 3) { this.drag = null; this.props.onInteractionError?.('This axis points toward the camera. Use another viewport or numeric handle entry.'); return; }
      this.drag = { ...this.drag, handle, original: structuredClone(model), origin, axis, screenAxis: screenAxis.normalize(), unitsPerPixel: 1 / pixels };
      this.emit({ preview: `${handle.label} · drag preview; Escape cancels${handle.kind === 'resize' ? ' · Resize uses grid/numeric; object snaps apply to Move' : ''}` });
    }
  }
  private pointerMove(view: View, e: PointerEvent) {
    const p = this.point(e, view), drag = this.drag;
    if (drag && drag.view === view.id) {
      const delta = p.clone().sub(drag.last); drag.last.copy(p); drag.moved ||= p.distanceTo(drag.start) > 4;
      if (drag.button === 2 || drag.button === 1) { this.navigate(view, delta, e.shiftKey || drag.button === 1); return; }
      if (drag.handle) { this.dragHandle(view, p); return; }
      if (drag.moved) this.emit({ rectangle: { view: view.id, left: Math.min(p.x, drag.start.x), top: Math.min(p.y, drag.start.y), width: Math.abs(p.x - drag.start.x), height: Math.abs(p.y - drag.start.y), crossing: p.x < drag.start.x } });
      this.coordinate(view, p); return;
    }
    const ids = this.hitIds(view, p), hover = ids[0] ?? null;
    if (hover !== this.overlay.hover) { this.emit({ hover }); this.syncAppearance(); }
    const handle = this.handleHit(view, p); view.element.style.cursor = handle ? 'grab' : ids.length ? 'pointer' : 'crosshair';
    this.coordinate(view, p);
  }
  private coordinate(view: View, p: THREE.Vector2, allowObjectSnap = true) {
    this.ray(view, p);
    const normal = view.id === 'front' ? new THREE.Vector3(0, 0, 1) : view.id === 'right' ? new THREE.Vector3(1, 0, 0) : new THREE.Vector3(0, 1, 0);
    const position = this.raycaster.ray.intersectPlane(new THREE.Plane(normal, 0), new THREE.Vector3());
    let found: ReturnType<typeof nearestSnap> = null;
    if (allowObjectSnap && this.props.display.object_snap) found = nearestSnap(this.snapPoints.filter(s => this.state(s.objectId).visible), (v: number[]) => this.project(v, view), p.toArray());
    const point = found ? found.position as Vec3 : position ? world(position).map(v => snapScalar(v, 0.1, this.props.display.grid_snap)) as Vec3 : null;
    const screen = found ? this.project(found.position, view) : null;
    this.emit({ snap: screen && found ? { view: view.id, x: screen[0], y: screen[1], label: found.kind } : null });
    this.props.onPointerStatus?.({ world: point, snap: found?.kind ?? (this.props.display.grid_snap ? 'Grid 0.1 m' : null), view: view.id });
    return found;
  }
  private navigate(view: View, delta: THREE.Vector2, pan: boolean) {
    const camera = view.camera;
    if (view.id !== 'perspective' || pan) {
      const units = camera instanceof THREE.OrthographicCamera ? (camera.top - camera.bottom) / camera.zoom / Math.max(view.element.clientHeight, 1) : camera.position.distanceTo(view.target) * 2 * Math.tan(20 * Math.PI / 180) / Math.max(view.element.clientHeight, 1);
      const right = new THREE.Vector3().setFromMatrixColumn(camera.matrixWorld, 0), up = new THREE.Vector3().setFromMatrixColumn(camera.matrixWorld, 1);
      const shift = right.multiplyScalar(-delta.x * units).add(up.multiplyScalar(delta.y * units)); camera.position.add(shift); view.target.add(shift);
    } else {
      const spherical = new THREE.Spherical().setFromVector3(camera.position.clone().sub(view.target)); spherical.theta -= delta.x * 0.007; spherical.phi = THREE.MathUtils.clamp(spherical.phi - delta.y * 0.007, 0.02, Math.PI - 0.02);
      camera.position.copy(view.target).add(new THREE.Vector3().setFromSpherical(spherical));
    }
    camera.lookAt(view.target); camera.updateMatrixWorld();
  }
  private dragHandle(view: View, p: THREE.Vector2) {
    const d = this.drag!, handle = d.handle!; let delta: number;
    if (handle.planar) {
      this.ray(view, p); const point = this.raycaster.ray.intersectPlane(d.plane!, new THREE.Vector3()); if (!point) return;
      const snap = this.coordinate(view, p), displacement = point.sub(d.startPoint!);
      const appliedObjectSnap = Boolean(snap && !this.props.selection.includes(snap.objectId));
      if (snap && appliedObjectSnap) { displacement.copy(vector(snap.position as Vec3).sub(d.origin!)); displacement.addScaledVector(d.plane!.normal, -displacement.dot(d.plane!.normal)); }
      const local = worldToLocal(world(displacement), d.original!.room.rotation_deg);
      d.candidate = candidateForPlanarMove(d.original!, this.props.selection[0], local, this.props.display.ortho, this.props.display.grid_snap, appliedObjectSnap) as Model;
      if (!d.candidate) return;
      this.props.onPreviewCandidate?.(d.candidate, handle.label);
      this.previewGeometry(d);
      const id = this.props.selection[0];
      const deltaLocal: Vec3 = id === 'window' ? [d.candidate.window.center_x_m - d.original!.window.center_x_m, 0, d.candidate.window.sill_m - d.original!.window.sill_m] : [d.candidate.obstruction!.center_x_m - d.original!.obstruction!.center_x_m, d.original!.obstruction!.distance_m - d.candidate.obstruction!.distance_m, 0];
      if (snap && appliedObjectSnap) {
        const reference = world(d.origin!.clone().add(vector(localToWorld(deltaLocal, d.original!.room.rotation_deg) as Vec3)));
        const screen = this.project(reference, view), label = `${snap.kind} · ${this.props.display.ortho ? 'Ortho projection' : 'plane projection'}`;
        this.emit({ snap: screen ? { view: view.id, x: screen[0], y: screen[1], label } : null });
        this.props.onPointerStatus?.({ world: reference, snap: label, view: view.id });
      }
      this.emit({ preview: `${handle.label} · Δ ${deltaLocal.map(x => x.toFixed(3)).join(', ')} m${this.props.display.ortho ? ' · Ortho' : ''}` }); if (this.cells) this.cells.visible = false; return;
    }
    if (handle.kind === 'rotate') {
      this.ray(view, p); const current = this.raycaster.ray.intersectPlane(d.plane!, new THREE.Vector3()); if (!current) return;
      const a = world(d.startPoint!.clone().sub(d.origin!)), b = world(current.sub(d.origin!));
      delta = -Math.atan2(a[0] * b[1] - a[1] * b[0], a[0] * b[0] + a[1] * b[1]) * 180 / Math.PI;
      delta = snapScalar(delta, this.props.display.ortho ? 90 : 5, this.props.display.ortho || this.props.display.grid_snap);
    } else {
      delta = p.clone().sub(d.start).dot(d.screenAxis!) * d.unitsPerPixel!;
      const found = this.coordinate(view, p, handle.kind === 'move');
      if (found && !this.props.selection.includes(found.objectId)) delta = vector(found.position as Vec3).sub(d.origin!).dot(d.axis!);
      else if (this.props.display.grid_snap) { const base = (d.original as unknown as Record<string, Record<string, number>>)[handle.path[0]][handle.path[1]]; delta = (snapScalar(base + delta * handle.multiplier, 0.1, true) - base) / handle.multiplier; }
    }
    d.candidate = candidateForHandle(d.original!, handle, delta) as Model;
    const value = (d.candidate as unknown as Record<string, Record<string, number>>)[handle.path[0]][handle.path[1]];
    this.props.onPreviewCandidate?.(d.candidate, handle.label); this.previewGeometry(d);
    this.emit({ preview: `${handle.label}: ${value.toFixed(handle.kind === 'rotate' ? 1 : 3)} ${handle.kind === 'rotate' ? '°' : 'm'} · provisional${handle.kind === 'resize' ? ' · Resize uses grid/numeric; object snaps apply to Move' : ''}` });
    if (this.cells) this.cells.visible = false;
  }
  private previewGeometry(d: Drag) {
    for (const child of [...this.ghost.children]) release(child);
    if (!d.candidate) return;
    const segments = previewWireSegments(d.candidate, this.props.selection[0]);
    const geometry = new THREE.BufferGeometry().setFromPoints(segments.flat().map((p: number[]) => vector(p as Vec3)));
    const helper = new THREE.LineSegments(geometry, new THREE.LineBasicMaterial({ color: 0x387cc0, depthTest: false, transparent: true, opacity: 0.9 }));
    helper.renderOrder = 30; this.ghost.add(helper);
  }
  private async pointerUp(view: View, e: PointerEvent) {
    const drag = this.drag; if (!drag || drag.view !== view.id) return;
    if (view.element.hasPointerCapture(e.pointerId)) view.element.releasePointerCapture(e.pointerId);
    const p = this.point(e, view); this.drag = null; this.emit({ rectangle: null, preview: null });
    if (drag.button !== 0) { this.saveCameras(); return; }
    if (drag.handle) {
      if (!drag.moved && this.props.selection.length === 1 && !drag.handle.planar) {
        const [group, field] = drag.handle.path; this.props.onNumericHandle?.({ entity: this.props.selection[0], parameter: drag.handle.path.join('.'), operation: drag.handle.kind, label: drag.handle.label, value: (drag.original as unknown as Record<string, Record<string, number>>)[group][field], unit: drag.handle.kind === 'rotate' ? 'deg' : 'm' });
      } else if (drag.candidate) {
        this.pending = true;
        try { await this.props.onCommitCandidate(drag.candidate, drag.handle.label); } catch (error) { this.props.onInteractionError?.(String(error)); }
        finally { this.pending = false; }
      }
      this.clearPreview(); this.syncAppearance(); return;
    }
    if (drag.moved) {
      const ids = [...this.objects.values()].filter(object => this.state(object.id).visible && !this.state(object.id).locked && object.id !== 'room').filter(object => {
        const projected = object.vertices.map(v => this.project(v, view)); return projected.every(Boolean) && rectangleSelect(convexHull(projected), drag.start.toArray(), p.toArray());
      }).map(object => object.id);
      this.props.onSelectionChange(mergeSelection(this.props.selection, ids, drag.shift, drag.ctrl));
    } else {
      const ids = this.hitIds(view, p);
      if (ids.length > 1) this.emit({ chooser: { view: view.id, x: p.x, y: p.y, ids, shift: drag.shift, ctrl: drag.ctrl } });
      else this.props.onSelectionChange(mergeSelection(this.props.selection, ids, drag.shift, drag.ctrl));
    }
  }
  choose(id: string, shift: boolean, ctrl: boolean) { if (this.state(id).visible && !this.state(id).locked) this.props.onSelectionChange(mergeSelection(this.props.selection, [id], shift || this.overlay.chooser?.shift, ctrl || this.overlay.chooser?.ctrl)); this.emit({ chooser: null, hover: null }); this.syncAppearance(); }
  hover(id: string | null) { this.emit({ hover: id }); this.syncAppearance(); }
  cancel() { this.drag = null; this.clearPreview(); this.emit({ rectangle: null, chooser: null, preview: null, snap: null }); this.syncAppearance(); }
  private clearPreview() { for (const child of [...this.ghost.children]) release(child); this.ghost.rotation.set(0, 0, 0); this.ghost.position.set(0, 0, 0); this.props.onPreviewCandidate?.(null); }
  private syncHandles() {
    const { validated, selection, display, tool, disabled } = this.props;
    const key = `${this.signature}|${selection.join(',')}|${display.gumball}|${tool}|${disabled}|${JSON.stringify(display.layers)}|${JSON.stringify(display.objects)}`;
    if (key === this.handleKey) return; this.handleKey = key;
    for (const child of [...this.handles.children]) release(child);
    if (!validated || selection.length !== 1 || !display.gumball || disabled || !this.state(selection[0]).visible || this.state(selection[0]).locked) return;
    const object = this.objects.get(selection[0]); if (!object) return;
    const handles = handleDefinitions(validated.model, selection[0], tool) as Handle[];
    if (!handles.length) return;
    const box = new THREE.Box3().setFromPoints(object.vertices.map(vector)); this.handles.position.copy(box.getCenter(new THREE.Vector3()));
    if (selection[0] === 'room') this.handles.position.set(0, 0, 0);
    const length = Math.max(0.55, Math.min(1.3, box.getSize(new THREE.Vector3()).length() * 0.2));
    if ((tool === 'select' || tool === 'move') && ['window', 'obstruction'].includes(selection[0])) {
      const grip = new THREE.Mesh(new THREE.SphereGeometry(length * 0.095, 12, 8), new THREE.MeshBasicMaterial({ color: 0xc89632, depthTest: false, depthWrite: false }));
      grip.userData.handle = { id: 'move-plane', axis: -1, kind: 'move', label: `Move ${objectLabel(selection[0])}`, path: [], multiplier: 1, planar: true }; grip.renderOrder = 21; this.handles.add(grip);
    }
    for (const handle of handles) {
      const axis = vector(localToWorld([handle.axis === 0 ? 1 : 0, handle.axis === 1 ? 1 : 0, handle.axis === 2 ? 1 : 0], validated.model.room.rotation_deg) as Vec3).normalize();
      const material = new THREE.MeshBasicMaterial({ color: axisColors[handle.axis], depthTest: false, depthWrite: false });
      if (handle.kind === 'rotate') {
        const ring = new THREE.Mesh(new THREE.TorusGeometry(length * 1.15, length * 0.025, 6, 64), material); ring.rotation.x = -Math.PI / 2; ring.userData.handle = handle; ring.renderOrder = 20; this.handles.add(ring); continue;
      }
      const end = axis.clone().multiplyScalar(length * (handle.kind === 'resize' ? 1.45 : 1));
      const tip = new THREE.Mesh(handle.kind === 'resize' ? new THREE.BoxGeometry(length * 0.13, length * 0.13, length * 0.13) : new THREE.ConeGeometry(length * 0.08, length * 0.2, 10), material);
      tip.position.copy(end); tip.quaternion.setFromUnitVectors(new THREE.Vector3(0, 1, 0), axis); tip.userData.handle = handle; tip.renderOrder = 20; this.handles.add(tip);
      if (handle.kind === 'move') { const line = new THREE.Mesh(new THREE.CylinderGeometry(length * 0.018, length * 0.018, length, 6), material.clone()); line.position.copy(end).multiplyScalar(0.5); line.quaternion.copy(tip.quaternion); line.userData.handle = handle; line.renderOrder = 20; this.handles.add(line); }
    }
  }
  private render() {
    const bounds = this.root.getBoundingClientRect();
    for (const view of this.views.values()) {
      if (this.props.display.maximized_view && this.props.display.maximized_view !== view.id) continue;
      const rect = view.element.getBoundingClientRect(); if (rect.width < 1 || rect.height < 1) continue;
      const camera = view.camera;
      if (camera instanceof THREE.PerspectiveCamera) camera.aspect = rect.width / rect.height;
      else { camera.left = -6 * rect.width / rect.height; camera.right = 6 * rect.width / rect.height; camera.top = 6; camera.bottom = -6; }
      camera.updateProjectionMatrix(); camera.updateMatrixWorld();
      view.element.dataset.cameraIdentity = camera.uuid;
      view.element.dataset.cameraProjection = view.id === 'perspective' ? 'perspective' : 'orthographic';
      view.element.dataset.cameraState = JSON.stringify({ position: world(camera.position), target: world(view.target), zoom: camera.zoom });
      this.grid.visible = this.props.display.grid_visible; this.grid.rotation.set(view.id === 'front' ? Math.PI / 2 : 0, 0, view.id === 'right' ? Math.PI / 2 : 0);
      this.renderer.setViewport(rect.left - bounds.left, bounds.bottom - rect.bottom, rect.width, rect.height); this.renderer.setScissor(rect.left - bounds.left, bounds.bottom - rect.bottom, rect.width, rect.height);
      this.renderer.setClearColor(view.id === 'perspective' ? 0xe6eaed : 0xf2f4f5); this.renderer.render(this.scene, camera);
    }
  }
  dispose() { this.cancel(); this.renderer.setAnimationLoop(null); this.observer.disconnect(); this.disposers.forEach(f => f()); for (const child of [...this.scene.children]) release(child); this.renderer.dispose(); this.renderer.domElement.remove(); }
}
