import { useState } from 'react';
import type { Project } from '../types';
import { useEnergyRead } from './useEnergyRead';

export type CapacityOperation = { kind: 'inspect' | 'import_reference' | 'report_issue' | 'design_report' } | { kind: 'energy_run'; alternative: string };
export type CapacityReport = {
  schema_version: 'greenlens.project-capacity.v1'; project_content_sha256: string;
  retained_bytes: number; retained_entries: number; available_bytes: number; available_entries: number;
  limits: { name: string; used: number; limit: number }[];
  incremental_lower_bound_bytes: number; incremental_lower_bound_entries: number;
  estimated_incremental_bytes: number | null; estimate_basis: string;
  can_start: boolean; warnings: string[]; inventory_scope: string; remedy: string;
};
type Props = { path: string | null; project: Project | null; operation: CapacityOperation };
const count = (n: unknown): n is number => typeof n === 'number' && Number.isSafeInteger(n) && n >= 0;
const text = (s: unknown): s is string => typeof s === 'string' && s.length <= 8192;
function validReport(value: CapacityReport | null): value is CapacityReport {
  return Boolean(value && value.schema_version === 'greenlens.project-capacity.v1' && /^[0-9a-f]{64}$/.test(value.project_content_sha256)
    && [value.retained_bytes, value.retained_entries, value.available_bytes, value.available_entries, value.incremental_lower_bound_bytes, value.incremental_lower_bound_entries].every(count)
    && (value.estimated_incremental_bytes === null || count(value.estimated_incremental_bytes)) && typeof value.can_start === 'boolean'
    && [value.estimate_basis, value.inventory_scope, value.remedy].every(text)
    && Array.isArray(value.warnings) && value.warnings.length <= 64 && value.warnings.every(text)
    && Array.isArray(value.limits) && value.limits.length <= 64 && value.limits.every(row => row && text(row.name) && count(row.used) && count(row.limit)));
}
const bytes = (value: number) => `${value.toLocaleString()} B (${(value / 1048576).toFixed(2)} MiB)`;
const operationLabel = (operation: CapacityOperation) => operation.kind === 'energy_run' ? `Energy run · ${operation.alternative}` : ({ inspect: 'Retained project', import_reference: 'Reference import', report_issue: 'Report issuance',design_report:'Analytical design report' } as const)[operation.kind];

/** Resource inventory only. Rust decides admission; this panel never predicts scientific output. */
export default function ProjectCapacity({ path, project, operation }: Props) {
  const [refresh, setRefresh] = useState(0);
  const read = useEnergyRead<CapacityReport>('project_capacity', project ? { path, project, operation } : null, refresh);
  const report = validReport(read.value) ? read.value : null;
  return <details className="energy-capacity"><summary>Project capacity · {operationLabel(operation)}</summary>
    <p>Retained-file inventory and declared estimates. This is not evidence-integrity verification or a promise that new output will fit.</p>
    <button disabled={!project || read.loading} onClick={() => setRefresh(value => value + 1)}>Refresh capacity inventory</button>
    {!project && <p>Select a project to inspect its retained capacity.</p>}
    {read.loading && <p role="status">Reading bounded retained-file inventory… Annual result arrays are not decoded by this check.</p>}
    {read.error && <p role="alert">Capacity inspection failed: {read.error}</p>}
    {read.value && !report && <p role="alert">Capacity response is malformed or unsupported. No capacity decision is displayed; refresh or inspect the project.</p>}
    {report && <>
      <p role="status"><strong>{report.can_start ? 'Known capacity checks permit starting.' : 'Cannot start: a known capacity restriction applies.'}</strong> Final publication and export still require exact admission checks.</p>
      <dl className="reference-facts"><dt>Retained size</dt><dd>{bytes(report.retained_bytes)}</dd><dt>Retained members</dt><dd>{report.retained_entries.toLocaleString()}</dd><dt>Remaining byte allowance</dt><dd>{bytes(report.available_bytes)}</dd><dt>Remaining member allowance</dt><dd>{report.available_entries.toLocaleString()}</dd><dt>Known addition lower bound</dt><dd>{bytes(report.incremental_lower_bound_bytes)}; {report.incremental_lower_bound_entries.toLocaleString()} members</dd><dt>Estimated addition</dt><dd>{report.estimated_incremental_bytes === null ? 'Unknown; no reliable total estimate is available.' : `${bytes(report.estimated_incremental_bytes)} — estimate, not an admission guarantee.`}</dd></dl>
      <p>{report.estimate_basis}</p>
      {report.warnings.map((warning, i) => <p key={i} role="alert">{warning}</p>)}
      <div className="reference-table"><table><caption>Rust-reported limits and inventory</caption><thead><tr><th scope="col">Limit</th><th scope="col">Inventoried use</th><th scope="col">Maximum</th></tr></thead><tbody>{report.limits.map((limit, i) => <tr key={`${limit.name}:${i}`}><th scope="row">{limit.name}</th><td>{limit.name.includes('bytes') || limit.name === 'largest retained entry' ? bytes(limit.used) : limit.used.toLocaleString()}</td><td>{limit.name.includes('bytes') || limit.name === 'largest retained entry' ? bytes(limit.limit) : limit.limit.toLocaleString()}</td></tr>)}</tbody></table></div>
      <p>{report.inventory_scope}</p><p>{report.remedy}</p>
      <p>Candidate project content: <code title={report.project_content_sha256}>{report.project_content_sha256}</code>. Refresh after external file changes; displayed inventory is a snapshot.</p>
    </>}
  </details>;
}
