/** Document actions only. Unmapped keys never fall through into daylight commands. */
export function energyShortcut(event) {
  if (event.altKey || !(event.ctrlKey || event.metaKey)) return null;
  return {n:'new',o:'open',s:event.shiftKey?'save_as':'save',z:event.shiftKey?'redo':'undo',y:'redo'}[event.key.toLowerCase()] ?? null;
}
export function selectedEnergyRun(runs, explicit, defaults, alternative) {
  const id=explicit || defaults?.[alternative] || '';
  return runs.some(r=>r.id===id && r.alternative_id===alternative)?id:'';
}
export function energyRowStart(text, intervalCount) {
  if (!/^\d+$/.test(text.trim())) throw new Error('Enter a whole interval index.');
  const start=Number(text);
  if (!Number.isSafeInteger(start) || start<0 || start>=intervalCount) throw new Error(`Interval must be from 0 to ${intervalCount-1}.`);
  return start;
}
