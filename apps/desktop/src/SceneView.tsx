import { useEffect, useRef, useState } from 'react';
import * as THREE from 'three';
import { OrbitControls } from 'three/addons/controls/OrbitControls.js';
import type { ValidatedModel, Vec3 } from './types';
import { luxColor } from './presentation.mjs';

// Explicit right-handed Z-up world -> right-handed Y-up Three.js mapping.
const renderPoint = ([x, y, z]: Vec3) => new THREE.Vector3(x, z, -y);

export default function SceneView({ validated, lux, scale, selectedSensor, onSelect }: {
  validated: ValidatedModel | null; lux: number[] | null; scale: number;
  selectedSensor: number | null; onSelect: (index: number) => void;
}) {
  const host = useRef<HTMLDivElement>(null);
  const [error, setError] = useState('');
  const [cutaway, setCutaway] = useState(true);
  const selectRef = useRef(onSelect);
  useEffect(() => { selectRef.current = onSelect; }, [onSelect]);
  useEffect(() => {
    if (!host.current || !validated) return;
    const root = host.current;
    let renderer: THREE.WebGLRenderer;
    try { renderer = new THREE.WebGLRenderer({ antialias: true, alpha: false }); setError(''); }
    catch { setError('WebGL2 is unavailable. The validated sensor table remains available below.'); return; }
    renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));
    renderer.setClearColor(0xeef2ed);
    root.appendChild(renderer.domElement);
    renderer.domElement.setAttribute('aria-label', 'Validated room geometry and workplane sensors. Drag to orbit, scroll to zoom.');
    const scene = new THREE.Scene();
    const camera = new THREE.PerspectiveCamera(38, 1, 0.01, 2000);
    const allPoints = validated.surfaces.flatMap(surface => surface.vertices.map(renderPoint));
    const box = new THREE.Box3().setFromPoints(allPoints), center = box.getCenter(new THREE.Vector3());
    const span = Math.max(...box.getSize(new THREE.Vector3()).toArray(), 3);
    camera.position.copy(center).add(new THREE.Vector3(span * 1.08, span * 0.95, span * 1.25));
    const controls = new OrbitControls(camera, renderer.domElement);
    controls.target.copy(center); controls.enableDamping = true; controls.dampingFactor = 0.12;
    controls.minDistance = 0.5; controls.maxDistance = span * 8;
    scene.add(new THREE.HemisphereLight(0xffffff, 0x657f74, 2.6));
    const light = new THREE.DirectionalLight(0xffffff, 2); light.position.set(10, 15, 12); scene.add(light);
    const disposable: (THREE.BufferGeometry | THREE.Material)[] = [];
    for (const surface of validated.surfaces) {
      const vertices = surface.vertices.map(renderPoint);
      const coords: number[] = [];
      for (let i = 1; i < vertices.length - 1; i++) for (const p of [vertices[0], vertices[i], vertices[i + 1]]) coords.push(...p.toArray());
      const geometry = new THREE.BufferGeometry(); geometry.setAttribute('position', new THREE.Float32BufferAttribute(coords, 3)); geometry.computeVertexNormals();
      const glass = surface.material === 'glass', ceiling = surface.material === 'ceiling', wall = surface.material === 'wall';
      const color = glass ? 0x9bd7d4 : surface.material === 'overhang' ? 0x61887a : surface.material === 'obstruction' ? 0x8e9892 : surface.material === 'floor' ? 0xd4d9cd : 0xf6f5ed;
      const transparent = glass || (cutaway && (ceiling || wall));
      const material = new THREE.MeshStandardMaterial({ color, side: THREE.DoubleSide, roughness: 1, transparent, opacity: glass ? 0.28 : ceiling && cutaway ? 0.05 : wall && cutaway ? 0.2 : 1, depthWrite: !transparent });
      const mesh = new THREE.Mesh(geometry, material); scene.add(mesh); disposable.push(geometry, material);
      const edgeGeometry = new THREE.EdgesGeometry(geometry), edgeMaterial = new THREE.LineBasicMaterial({ color: glass ? 0x397d75 : 0x53655b, transparent: true, opacity: ceiling && cutaway ? 0.2 : 0.55 });
      scene.add(new THREE.LineSegments(edgeGeometry, edgeMaterial)); disposable.push(edgeGeometry, edgeMaterial);
    }
    const grid = validated.sensor_grid;
    const cellGeometry = new THREE.PlaneGeometry(grid.actual_spacing_x_m * 0.94, grid.actual_spacing_y_m * 0.94);
    const cellMaterial = new THREE.MeshBasicMaterial({ side: THREE.DoubleSide });
    const cells = new THREE.InstancedMesh(cellGeometry, cellMaterial, grid.sensors.length);
    const dummy = new THREE.Object3D();
    grid.sensors.forEach((sensor, index) => {
      dummy.position.copy(renderPoint(sensor.position));
      dummy.rotation.set(-Math.PI / 2, 0, -validated.model.room.rotation_deg * Math.PI / 180);
      dummy.updateMatrix(); cells.setMatrixAt(index, dummy.matrix);
      cells.setColorAt(index, new THREE.Color(lux ? luxColor(lux[index], scale) : '#76968b'));
    });
    scene.add(cells); disposable.push(cellGeometry, cellMaterial);
    if (selectedSensor !== null && grid.sensors[selectedSensor]) {
      const geo = new THREE.RingGeometry(Math.min(grid.actual_spacing_x_m, grid.actual_spacing_y_m) * 0.22, Math.min(grid.actual_spacing_x_m, grid.actual_spacing_y_m) * 0.31, 32);
      const mat = new THREE.MeshBasicMaterial({ color: 0xffffff, side: THREE.DoubleSide });
      const marker = new THREE.Mesh(geo, mat); marker.position.copy(renderPoint(grid.sensors[selectedSensor].position)); marker.position.y += 0.005; marker.rotation.x = -Math.PI / 2; scene.add(marker); disposable.push(geo, mat);
    }
    const axes = new THREE.AxesHelper(span * 0.18); axes.position.set(-0.15, 0.015, 0.15); scene.add(axes);
    const resize = new ResizeObserver(() => { const w = root.clientWidth, h = root.clientHeight; renderer.setSize(w, h); camera.aspect = w / Math.max(h, 1); camera.updateProjectionMatrix(); }); resize.observe(root);
    const raycaster = new THREE.Raycaster();
    let downX = 0, downY = 0;
    const onDown = (event: PointerEvent) => { downX = event.clientX; downY = event.clientY; };
    const onClick = (event: PointerEvent) => {
      if (Math.hypot(event.clientX - downX, event.clientY - downY) > 4) return;
      const bounds = renderer.domElement.getBoundingClientRect();
      raycaster.setFromCamera(new THREE.Vector2((event.clientX - bounds.left) / bounds.width * 2 - 1, -(event.clientY - bounds.top) / bounds.height * 2 + 1), camera);
      const picked = raycaster.intersectObject(cells)[0]?.instanceId;
      if (picked !== undefined) selectRef.current(picked);
    };
    renderer.domElement.addEventListener('pointerdown', onDown); renderer.domElement.addEventListener('pointerup', onClick);
    renderer.setAnimationLoop(() => { controls.update(); renderer.render(scene, camera); });
    return () => { resize.disconnect(); renderer.setAnimationLoop(null); controls.dispose(); disposable.forEach(item => item.dispose()); axes.geometry.dispose(); (axes.material as THREE.Material).dispose(); renderer.dispose(); renderer.domElement.remove(); };
  }, [validated, lux, scale, cutaway, selectedSensor]);
  return <div className="scene-shell">
    <div className="scene-tools"><span>WORLD METRES · Z UP</span><button onClick={() => setCutaway(!cutaway)} aria-pressed={cutaway}>{cutaway ? 'Cutaway view' : 'Solid envelope'}</button></div>
    <div ref={host} className="scene-canvas" />
    {!validated && <div className="scene-empty">Validate a model to inspect its geometry.</div>}
    {error && <div className="scene-empty" role="alert">{error}</div>}
    <div className="scene-caption">Drag to orbit · scroll to zoom · select a sensor. {lux ? 'Cell colors show calculated illuminance.' : 'Green cells locate sensors; no results are shown.'}</div>
  </div>;
}
