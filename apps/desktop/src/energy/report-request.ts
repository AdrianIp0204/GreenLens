import type {Project} from '../types';

/** UI request context only. Rust report/run hashes remain the evidence authority. */
export type ReportRequestBinding={
  project_id:string;project_path:string;baseline_run_id:string;proposed_run_id:string;
  report_revision:string|null;request_key:string;portable_zip:boolean;
};
export type ReportOutput={destination:string;files:string[];renderer_status?:string;message?:string;request_binding?:ReportRequestBinding;dataset_sha256?:string};

export function reportRequestKey(path:string|null,project:Project|null,a:string,b:string,typstDir:string,htmlOnly:boolean):string{
  if(!project||!path)return '';
  const energy=project.energy,workbench=energy?.workbench;
  const {locks:_locks,...workbenchInputs}=workbench??{};
  // Camera/display state, daylight inputs and thermal edit locks do not enter an
  // energy report request. No numerical calculation or scientific hash is made.
  const inputs={schema_version:project.schema_version,id:project.id,name:project.name,
    energy:energy?{...energy,workbench:workbench?workbenchInputs:undefined}:energy,
    architecture:project.architecture};
  return JSON.stringify([path,inputs,a,b,typstDir.trim(),htmlOnly]);
}

export function reportRequestBinding(path:string,project:Project,a:string,b:string,typstDir:string,htmlOnly:boolean,portableZip:boolean):ReportRequestBinding{
  return{project_id:project.id,project_path:path,baseline_run_id:a,proposed_run_id:b,
    report_revision:project.energy?.workbench?.report_metadata?.revision??null,
    request_key:reportRequestKey(path,project,a,b,typstDir,htmlOnly),portable_zip:portableZip};
}

export function validReportRequestBinding(value:ReportRequestBinding|undefined):value is ReportRequestBinding{
  return Boolean(value&&[value.project_id,value.project_path,value.baseline_run_id,value.proposed_run_id,value.request_key].every(v=>typeof v==='string'&&v.length>0)
    &&(value.report_revision===null||typeof value.report_revision==='string')&&typeof value.portable_zip==='boolean');
}
