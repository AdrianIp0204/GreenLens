import type { OpenProgress } from '../workspace/project-intake';
type Props={label:string;read:{loading:boolean;progress:OpenProgress|null;cancellable:boolean;cancel:()=>void}};
/** Observable work, never an inferred completion percentage or integrity claim. */
export default function EvidenceInspection({label,read}:Props){
  if(!read.loading)return null;
  const p=read.progress;
  return <div className="energy-inspection" role="status" aria-live="polite"><span>{label} — {p?`${p.stage.replaceAll('_',' ')} · ${(p.bytes_read/1048576).toFixed(1)} MiB read · ${p.runs_completed}/${p.runs_total} runs`:'Starting evidence inspection…'}</span>{read.cancellable&&<button aria-label={`Cancel inspection: ${label}`} onClick={read.cancel}>Cancel inspection</button>}</div>;
}
