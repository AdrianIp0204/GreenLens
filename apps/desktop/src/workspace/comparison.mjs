// Selection and async presentation only. Rust owns comparison compatibility,
// sensor order, scales and deltas; never substitute an incompatible pair.
export function historicalSelection(documentId, records, previous) {
  const retained=previous?.documentId===documentId?previous:null;
  const latest=id=>[...records].reverse().find(run=>run.alternative_id===id)?.id??null;
  return {documentId,aId:retained?.aId??latest('A'),bId:retained?.bId??latest('B')};
}
export function selectedHistoricalRuns(records, selection) {
  return {
    a:records.find(run=>run.id===selection.aId&&run.alternative_id==='A')??null,
    b:records.find(run=>run.id===selection.bId&&run.alternative_id==='B')??null,
  };
}
export function chooseHistoricalRun(records, selection, alternative, id) {
  if(!['A','B'].includes(alternative)||!records.some(run=>run.id===id&&run.alternative_id===alternative))throw new Error('Choose a retained snapshot from the indicated alternative.');
  return {...selection,[alternative==='A'?'aId':'bId']:id};
}
export function comparisonKey(selection) { return JSON.stringify([selection.documentId,selection.aId,selection.bId]); }
export function snapshotRelation(run, scientificHash) {
  if(!scientificHash)return 'Current design validation pending or unavailable';
  return scientificHash===run.validated.scientific_input_hash?'Matches current design':'Current-design mismatch — immutable historical result';
}
export class ComparisonRequestGate {
  generation=0;
  cancel(){this.generation++;}
  async compare(a,b,backend){
    const generation=++this.generation;
    try{
      const comparison=await backend(a,b);
      if(generation!==this.generation)return null;
      if(comparison.a_id!==a.id||comparison.b_id!==b.id)throw new Error('Comparison response does not match the selected snapshots. Select the pair again.');
      return {comparison,error:''};
    }catch(error){
      if(generation!==this.generation)return null;
      return {comparison:null,error:typeof error==='string'?error:error instanceof Error?error.message:JSON.stringify(error)};
    }
  }
}
