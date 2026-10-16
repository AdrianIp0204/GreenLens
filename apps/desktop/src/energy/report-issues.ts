import type {ProjectView} from '../types';
import type {ScopeRevision,ScopeAssessment} from '../reference/types';
import type {EnergyReportData,EnergyReportMetadata} from './types';

// Versioned Rust wire contracts. Inspection verifies retained bytes in Rust;
// TypeScript checks only bounded response shape and exact selection bindings.
export interface IssueRunBinding {run_id:string;record_sha256:string;design_id:string}
export interface ReportIssueSummary {
  id:string;path:string;content_sha256:string;content_file_sha256:string;
  products_sha256:string;products_file_sha256:string;receipt_file_sha256:string;
  scope_revision_sha256:string;reference_asset_id:string;reference_manifest_sha256:string;
  baseline:IssueRunBinding;proposed:IssueRunBinding;sensitivity_session_id:string|null;
  sensitivity_run_ids:string[];title:string;report_revision:string;include_pdf:boolean;
}
export interface ReportIssueIndex {schema_version:'greenlens.report-issue-index.v1';issues:ReportIssueSummary[]}
export interface IssueProduct {path:string;bytes:number;sha256:string}
export interface ReportIssueInspection {
  summary:ReportIssueSummary;
  content:{schema_version:'greenlens.energy-report-issue-content.v1';content_sha256:string;
    workbench:{base:EnergyReportData;metadata:EnergyReportMetadata};
    scope_revision:ScopeRevision;scope_assessment:ScopeAssessment;
    sensitivity:{session_id:string;result:{assessment:{numerical_sensitivity_passed:boolean;criteria:{id:string;explanation:string;passed:boolean}[];missing_slots:string[]}}}|null;figures:unknown[];include_pdf:boolean;
    renderer:{method:string;document_schema:string;html_method:string;figure_method:string;
      template_sha256:string;typst_version:string;typst_executable_sha256:string;
      font_sha256:Record<string,string>}};
  products:{schema_version:'greenlens.report-issue-products.v1';content_sha256:string;products_sha256:string;products:IssueProduct[]};
  receipt:{schema_version:string;content_sha256:string;issued_utc:string;renderer_status:string;process_count:number;elapsed_ms:number;peak_working_set_bytes:number;peak_scratch_bytes:number;process_trees_empty:boolean};
  regeneration_available:boolean;regeneration_message:string;
}
export type ReportIssueAction={kind:'create';a:string;b:string;scope_sha256:string;html_only:boolean}|{kind:'regenerate';id:string}|{kind:'recover';token:string}|{kind:'export'};
export type ReportIssueActionResult={view?:ProjectView;issue?:ReportIssueInspection;destination?:string;issue_id?:string;artifacts?:unknown;[key:string]:unknown};
export interface ReportIssueProduct {path:string;media_type:string;svg?:string;bytes?:number[];issue_id?:string;sha256?:string}

export function productPurpose(path:string):string {
  if(/(?:executive|technical|assessment)\.(?:pdf|html)$/.test(path))return 'Assessment reports';
  if(/(?:performance|comparison)/.test(path))return 'Performance tables and images';
  if(/\.(?:svg|png)$/.test(path))return 'Retained report figures';
  if(/(?:appendix|inputs|input-provenance|scope-revision|inventory)/.test(path))return 'Inputs and evidence inventories';
  return 'Reproducibility and machine-readable evidence';
}
