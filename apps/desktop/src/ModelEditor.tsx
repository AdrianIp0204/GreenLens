import type { Model } from './types';

function NumberField({ label, unit = 'm', value, onChange, step = 0.1 }: { label: string; unit?: string; value: number; onChange: (n: number) => void; step?: number }) {
  return <label className="number-field"><span>{label}</span><div><input aria-label={`${label} (${unit})`} type="number" step={step} value={Number.isNaN(value) ? '' : value} onChange={event => onChange(event.target.value === '' ? NaN : Number(event.target.value))} /><span>{unit}</span></div></label>;
}
export default function ModelEditor({ model, onChange, disabled }: { model: Model; onChange: (model: Model) => void; disabled: boolean }) {
  const set = <K extends keyof Model>(part: K, key: string, value: number) => onChange({ ...model, [part]: { ...(model[part] as object), [key]: value } });
  return <fieldset className="editor" disabled={disabled}>
    <details open><summary>Room & orientation</summary><div className="fields">
      <NumberField label="Room width" value={model.room.width_m} onChange={n => set('room', 'width_m', n)} />
      <NumberField label="Room depth" value={model.room.depth_m} onChange={n => set('room', 'depth_m', n)} />
      <NumberField label="Room height" value={model.room.height_m} onChange={n => set('room', 'height_m', n)} />
      <NumberField label="Clockwise rotation" unit="°" value={model.room.rotation_deg} onChange={n => set('room', 'rotation_deg', n)} step={5} />
    </div><p className="hint">Window is on the local y = 0 facade, facing south at 0° rotation.</p></details>
    <details open><summary>Window & glazing</summary><div className="fields">
      <NumberField label="Window width" value={model.window.width_m} onChange={n => set('window', 'width_m', n)} />
      <NumberField label="Window height" value={model.window.height_m} onChange={n => set('window', 'height_m', n)} />
      <NumberField label="Window sill" value={model.window.sill_m} onChange={n => set('window', 'sill_m', n)} />
      <NumberField label="Window center X" value={model.window.center_x_m} onChange={n => set('window', 'center_x_m', n)} />
      <NumberField label="Visible transmittance" unit="0–1" value={model.materials.visible_transmittance} onChange={n => set('materials', 'visible_transmittance', n)} step={0.01} />
    </div><p className="hint">Neutral, zero-thickness equivalent glass. Supported T: 0.01–0.90.</p></details>
    <details><summary>Surface reflectances</summary><div className="fields">
      <NumberField label="Ceiling reflectance" unit="0–1" value={model.materials.ceiling_reflectance} onChange={n => set('materials', 'ceiling_reflectance', n)} step={0.05} />
      <NumberField label="Wall reflectance" unit="0–1" value={model.materials.wall_reflectance} onChange={n => set('materials', 'wall_reflectance', n)} step={0.05} />
      <NumberField label="Floor reflectance" unit="0–1" value={model.materials.floor_reflectance} onChange={n => set('materials', 'floor_reflectance', n)} step={0.05} />
    </div><p className="hint">Neutral diffuse surfaces. Supported reflectance: 0–0.95.</p></details>
    <details open><summary>Horizontal overhang</summary>
      <label className="toggle"><input type="checkbox" checked={Boolean(model.overhang)} onChange={event => onChange({ ...model, overhang: event.target.checked ? { projection_m: 0.6, side_extension_m: 0, gap_above_window_m: 0, reflectance: 0.5 } : null })} />Include overhang</label>
      {model.overhang && <div className="fields">
        <NumberField label="Overhang projection" value={model.overhang.projection_m} onChange={n => set('overhang', 'projection_m', n)} />
        <NumberField label="Side extension" value={model.overhang.side_extension_m} onChange={n => set('overhang', 'side_extension_m', n)} />
        <NumberField label="Gap above window" value={model.overhang.gap_above_window_m} onChange={n => set('overhang', 'gap_above_window_m', n)} />
        <NumberField label="Overhang reflectance" unit="0–1" value={model.overhang.reflectance} onChange={n => set('overhang', 'reflectance', n)} step={0.05} />
      </div>}
    </details>
    <details><summary>Detached obstruction</summary>
      <label className="toggle"><input type="checkbox" checked={Boolean(model.obstruction)} onChange={event => onChange({ ...model, obstruction: event.target.checked ? { center_x_m: 2, width_m: 6, depth_m: 2, height_m: 4, distance_m: 5, reflectance: 0.2 } : null })} />Include opaque obstruction</label>
      {model.obstruction && <div className="fields">
        <NumberField label="Obstruction center X" value={model.obstruction.center_x_m} onChange={n => set('obstruction', 'center_x_m', n)} />
        <NumberField label="Obstruction width" value={model.obstruction.width_m} onChange={n => set('obstruction', 'width_m', n)} />
        <NumberField label="Obstruction depth" value={model.obstruction.depth_m} onChange={n => set('obstruction', 'depth_m', n)} />
        <NumberField label="Obstruction height" value={model.obstruction.height_m} onChange={n => set('obstruction', 'height_m', n)} />
        <NumberField label="Facade clearance" value={model.obstruction.distance_m} onChange={n => set('obstruction', 'distance_m', n)} />
        <NumberField label="Obstruction reflectance" unit="0–1" value={model.obstruction.reflectance} onChange={n => set('obstruction', 'reflectance', n)} step={0.05} />
      </div>}
    </details>
    <details open><summary>Sensors & prescribed sky</summary><div className="fields">
      <NumberField label="Workplane height" value={model.sensors.workplane_height_m} onChange={n => set('sensors', 'workplane_height_m', n)} />
      <NumberField label="Maximum grid spacing" value={model.sensors.requested_spacing_m} onChange={n => set('sensors', 'requested_spacing_m', n)} step={0.05} />
      <NumberField label="Wall inset" value={model.sensors.wall_inset_m} onChange={n => set('sensors', 'wall_inset_m', n)} step={0.05} />
      <NumberField label="Exterior illuminance" unit="lx" value={model.sky.exterior_horizontal_illuminance_lux} onChange={n => set('sky', 'exterior_horizontal_illuminance_lux', n)} step={1000} />
      <NumberField label="Ground reflectance" unit="0–1" value={model.sky.ground_reflectance} onChange={n => set('sky', 'ground_reflectance', n)} step={0.05} />
    </div><p className="hint">Equal-area cells, upward normals, up to 10,000 sensors. No simulated date or weather file.</p></details>
  </fieldset>;
}
