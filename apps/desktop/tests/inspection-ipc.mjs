// Explicit test-only IPC adapter. Panel fixtures keep their original reply
// factories; the actual hook still reserves and invokes the inspection API.
// These factories do not establish native cancellation or evidence integrity.
export function inspectionCore(globalName,extra=''){
  return 'data:text/javascript,'+encodeURIComponent(`
    let ticket=0;
    export class Channel { onmessage=()=>{}; }
    export const invoke=async(command,args)=>{
      globalThis.__inspectionProtocol?.(command,args);
      if(command==='prepare_evidence_inspection')return ++ticket;
      if(command==='cancel_evidence_inspection')return;
      if(command==='inspect_energy_evidence')return globalThis[${JSON.stringify(globalName)}](args.query.command,args.query.arguments);
      return globalThis[${JSON.stringify(globalName)}](command,args);
    };
    ${extra}
  `);
}
