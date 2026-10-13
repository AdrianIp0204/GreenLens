// Identity/IPC shape guards only. Rust computes and validates scientific values.
export function assertEnergyResponse(command,request,value){
  const summary=id=>request.project?.energy?.runs.find(r=>r.id===id);
  const caseMatches=(c,id)=>c?.run_id===id&&c.record_sha256===summary(id)?.record_sha256;
  if(command==='energy_default_runs'&&(!value||typeof value!=='object'||Array.isArray(value)||Object.entries(value).some(([alternative,id])=>!['A','B'].includes(alternative)||summary(id)?.alternative_id!==alternative)))throw new Error('Automatic energy snapshot selection differs from the retained alternatives.');
  if(command==='energy_result'&&!caseMatches(value?.case,request.runId))throw new Error('Energy response does not bind the selected immutable run.');
  if(command==='energy_compare'&&(!caseMatches(value?.data?.baseline,request.a)||!caseMatches(value?.data?.proposed,request.b)))throw new Error('Energy comparison does not bind both selected immutable runs.');
  if(command==='energy_rows'&&(value?.run_id!==request.runId||value.start!==request.start||!Array.isArray(value.rows)||value.rows.length>request.count||!value.series?.channels?.length||value.rows.some(row=>!Array.isArray(row)||row.length!==value.series.channels.length||row.some(v=>!Number.isFinite(v)))||!Array.isArray(value.demand_10min)||value.demand_10min.length>value.rows.length||value.demand_10min.some(r=>!Number.isFinite(r.facility_kw)||!r.end_uses_kw||Object.values(r.end_uses_kw).some(v=>!Number.isFinite(v))||!r.timestamp||!Number.isInteger(r.timestamp.interval_index)||r.timestamp.interval_index<0||r.timestamp.interval_index>=52560)))throw new Error('Energy interval response shape or snapshot differs.');
  return value;
}
