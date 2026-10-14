// Identity/IPC shape guards only. Rust computes and validates scientific values.
export function assertEnergyResponse(command,request,value){
  if(command==='energy_copy_design_preview'&&(!value||value.schema_version!=='greenlens.energy-design-copy-preview.v1'||value.source!==request.source||value.target!==request.target||value.source===value.target||!['A','B'].includes(value.source)||!['A','B'].includes(value.target)||typeof value.valid!=='boolean'||!/^[0-9a-f]{64}$/.test(value.preview_sha256)))throw new Error('Design copy preview does not bind the requested source, destination and reviewed identity.');
  if(command==='energy_sensitivity_inspect'){
    const reference=request.project?.energy?.workbench?.sensitivity?.find(r=>r.id===request.id);
    if(!reference||value?.reference?.id!==reference.id||value.reference.plan_sha256!==reference.plan_sha256||!Array.isArray(value?.plan?.slots)||value.plan.slots.length>4)throw new Error('Sensitivity response does not bind the selected immutable declaration.');
  }
  const summary=id=>request.project?.energy?.runs.find(r=>r.id===id);
  const caseMatches=(c,id)=>c?.run_id===id&&c.record_sha256===summary(id)?.record_sha256;
  if(command==='energy_default_runs'&&(!value||typeof value!=='object'||Array.isArray(value)||Object.entries(value).some(([alternative,id])=>!['A','B'].includes(alternative)||summary(id)?.alternative_id!==alternative)))throw new Error('Automatic energy snapshot selection differs from the retained alternatives.');
  if(command==='energy_result'&&!caseMatches(value?.case,request.runId))throw new Error('Energy response does not bind the selected immutable run.');
  if(command==='energy_compare'&&(!caseMatches(value?.data?.baseline,request.a)||!caseMatches(value?.data?.proposed,request.b)))throw new Error('Energy comparison does not bind both selected immutable runs.');
  if(command==='energy_rows'&&(value?.run_id!==request.runId||value.start!==request.start||!Array.isArray(value.rows)||value.rows.length>request.count||!value.series?.channels?.length||value.rows.some(row=>!Array.isArray(row)||row.length!==value.series.channels.length||row.some(v=>!Number.isFinite(v)))||!Array.isArray(value.demand_10min)||value.demand_10min.length>value.rows.length||value.demand_10min.some(r=>!Number.isFinite(r.facility_kw)||!r.end_uses_kw||Object.values(r.end_uses_kw).some(v=>!Number.isFinite(v))||!r.timestamp||!Number.isInteger(r.timestamp.interval_index)||r.timestamp.interval_index<0||r.timestamp.interval_index>=52560)))throw new Error('Energy interval response shape or snapshot differs.');
  return value;
}
