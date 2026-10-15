export interface ProjectAdmission {
  schema_version:string;project_id:string;project_name:string;manifest_sha256:string;
  static_runs:number;climate_runs:number;energy_runs:number;evidence_checked:false;
}
export interface OpenProgress {
  schema_version:string;stage:string;run_id:string|null;runs_completed:number;runs_total:number;
  bytes_read:number;files_verified:number;numeric_passes:Record<string,number>;elapsed_ms:number;
}
export interface ProjectVerificationReceipt {
  schema_version:string;started_utc:string;finished_utc:string;manifest_sha256:string;
  content_snapshot_sha256:string;progress:OpenProgress;scope:string;
}
export type OpenOperation={ticket:number;name:string;cancelled:boolean;progress:OpenProgress|null};
