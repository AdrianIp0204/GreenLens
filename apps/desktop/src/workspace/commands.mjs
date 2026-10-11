export const COMMANDS = [
  ['New','New synthetic document'], ['Open','Open project folder'], ['Save','Save project'], ['SaveAs','Save a complete project copy'], ['Close','Close with document protection'],
  ['Undo','Undo accepted edit'], ['Redo','Redo accepted edit'], ['4View','Restore four views'],
  ['Perspective','Maximize perspective'], ['Top','Maximize top'], ['Front','Maximize front'], ['Right','Maximize right'],
  ['ZoomExtents','Frame all visible geometry'], ['ZoomSelected','Frame selection'], ['SelectAll','Select visible unlocked entities'], ['SelectNone','Clear selection'],
  ['Hide','Hide selected entities'], ['Show','Show all entities and layers'], ['Lock','Lock selected entities'], ['Unlock','Unlock all entities and layers'],
  ['Move','Move window or obstruction by local offsets'], ['Resize','Set supported entity dimensions'], ['Rotate','Set room clockwise rotation'],
  ['Room','Configure width depth height [rotation]'], ['Window','Configure width height sill center-x'],
  ['Overhang','Create/configure projection [side-extension gap]'], ['Obstruction','Create/configure width depth height clearance center-x'],
  ['Remove','Remove optional overhang or obstruction'], ['Run','Run current design'], ['Cancel','Cancel calculation or command'], ['Export','Export portable evidence'],
  ['Wireframe','Wireframe display'], ['Shaded','Shaded display'], ['Analysis','Analysis display'], ['Cutaway','Toggle translucent envelope'], ['Grid','Toggle construction grid'], ['Snap','Toggle grid snap'], ['Ortho','Toggle Ortho'], ['Osnap','Toggle geometric snaps'], ['Gumball','Toggle supported handles'],
];
const names = new Map(COMMANDS.map(([name]) => [name.toLowerCase(), name]));
const aliases = { 'selall':'SelectAll','selnone':'SelectNone','ze':'ZoomExtents','zs':'ZoomSelected','saveas':'SaveAs','delete':'Remove' };
export function parseCommand(text) {
  const tokens = text.trim().split(/\s+/).filter(Boolean);
  if (!tokens.length) return null;
  let raw = tokens.shift().toLowerCase();
  if (['zoom','select','save'].includes(raw) && tokens.length) {
    const combined = raw + tokens[0].toLowerCase();
    if (names.has(combined)) { raw = combined; tokens.shift(); }
  }
  const name = aliases[raw] ?? names.get(raw);
  if (!name) throw new Error(`Unknown command “${raw}”. Type a registered command; shell commands are not supported.`);
  return { name, args: tokens };
}
export function suggestions(prefix) { return COMMANDS.filter(([name]) => name.toLowerCase().startsWith(prefix.trim().toLowerCase())); }
export function repeatable(name) { return ['4View','Perspective','Top','Front','Right','ZoomExtents','ZoomSelected','SelectAll','SelectNone'].includes(name); }
export function parseQuantity(text, kind = 'length') {
  const m = String(text).trim().match(/^([+-]?(?:\d+(?:\.\d*)?|\.\d+)(?:e[+-]?\d+)?)\s*(m|cm|mm|ft|in|deg|°|lx)?$/i);
  if (!m) throw new Error('Enter a finite number with compatible units (for example 1.8m or 1800mm).');
  const value = Number(m[1]); const unit = (m[2] ?? '').toLowerCase();
  if (!Number.isFinite(value)) throw new Error('The numeric value must be finite.');
  if (kind === 'length') {
    const factor = { '':1, m:1, cm:.01, mm:.001, ft:.3048, in:.0254 }[unit];
    if (factor === undefined) throw new Error('This parameter requires a length in m, cm, mm, ft or in.');
    return value * factor;
  }
  if (kind === 'angle' && !['','deg','°'].includes(unit)) throw new Error('Rotation requires degrees.');
  if (kind === 'lux' && !['','lx'].includes(unit)) throw new Error('Illuminance requires lux.');
  if (kind === 'number' && unit) throw new Error('This parameter is dimensionless.');
  return value;
}
const optionalDefaults = {
  overhang: { projection_m:.6, side_extension_m:0, gap_above_window_m:0, reflectance:.5 },
  obstruction: { center_x_m:2, width_m:6, depth_m:2, height_m:4, distance_m:5, reflectance:.2 },
};
export function setParameter(model, path, value) {
  const [part,key] = path.split('.');
  const next = structuredClone(model);
  if (!next[part] || typeof next[part][key] !== 'number') throw new Error('That parameter is unavailable on the selected entity.');
  next[part][key] = value; return next;
}
export function modelCommand(model, selection, name, args) {
  const next = structuredClone(model);
  let target = args[0]?.toLowerCase();
  const entities = ['room','window','overhang','obstruction'];
  let values = args;
  if (entities.includes(target)) values = args.slice(1);
  else { target = selection.length === 1 ? selection[0] : null; }
  if (['Room','Window','Overhang','Obstruction'].includes(name)) target = name.toLowerCase();
  const required = (count, prompt) => { if(values.length < count) return {prompt}; if(values.length > count && !['Room','Overhang'].includes(name)) throw new Error(`Expected ${count} numeric values. ${prompt}`); return null; };
  if (name === 'Room') {
    const needed=required(3,'Room width depth height [rotation-deg]'); if(needed)return needed;
    if(values.length>4)throw new Error('Room expects width depth height and optional rotation.');
    [next.room.width_m,next.room.depth_m,next.room.height_m]=values.slice(0,3).map(v=>parseQuantity(v));
    if(values[3])next.room.rotation_deg=parseQuantity(values[3],'angle');
  } else if (name === 'Window') {
    const needed=required(4,'Window width height sill center-x');if(needed)return needed;
    [next.window.width_m,next.window.height_m,next.window.sill_m,next.window.center_x_m]=values.map(v=>parseQuantity(v));
  } else if (name === 'Overhang') {
    if (!values.length) { next.overhang ??= structuredClone(optionalDefaults.overhang); }
    else { if(values.length>3)throw new Error('Overhang expects projection [side-extension gap].'); next.overhang ??= structuredClone(optionalDefaults.overhang); ['projection_m','side_extension_m','gap_above_window_m'].forEach((key,i)=>{if(values[i])next.overhang[key]=parseQuantity(values[i]);}); }
  } else if (name === 'Obstruction') {
    if (!values.length) next.obstruction ??= {...optionalDefaults.obstruction,center_x_m: model.room.width_m/2};
    else { const needed=required(5,'Obstruction width depth height clearance center-x');if(needed)return needed;next.obstruction??=structuredClone(optionalDefaults.obstruction);['width_m','depth_m','height_m','distance_m','center_x_m'].forEach((key,i)=>next.obstruction[key]=parseQuantity(values[i])); }
  } else if (name === 'Remove') {
    if (!['overhang','obstruction'].includes(target)) throw new Error('Only the optional overhang or obstruction can be removed. The room and window are required.');
    next[target]=null;
  } else if (name === 'Rotate') {
    if(target!=='room')throw new Error('Only the room has a supported vertical-axis rotation. Select Room or use Rotate room <degrees>.');
    const needed=required(1,'Rotate room clockwise-degrees');if(needed)return needed;
    next.room.rotation_deg=parseQuantity(values[0],'angle');
  } else if(name==='Move') {
    if(!['window','obstruction'].includes(target))throw new Error('Move supports one window (local X/Z) or obstruction (local X/clearance). Select one unlocked entity.');
    if(!next[target])throw new Error('Create the optional entity before moving it.');
    const needed=required(2,`Move ${target} delta-x ${target==='window'?'delta-z':'delta-clearance'}`);if(needed)return needed;
    next[target].center_x_m+=parseQuantity(values[0]);next[target][target==='window'?'sill_m':'distance_m']+=parseQuantity(values[1]);
  } else if(name==='Resize') {
    if(!entities.includes(target)||!next[target])throw new Error('Resize requires one existing room, window, overhang or obstruction.');
    const keys=target==='window'?['width_m','height_m']:target==='overhang'?['projection_m','side_extension_m','gap_above_window_m']:['width_m','depth_m','height_m'];
    const needed=required(keys.length,`Resize ${target} ${keys.map(k=>k.replace('_m','')).join(' ')}`);if(needed)return needed;
    keys.forEach((key,i)=>next[target][key]=parseQuantity(values[i]));
  } else throw new Error('This is not a scientific model command.');
  return { model:next,target,label:`${name} ${target ?? ''}`.trim() };
}
