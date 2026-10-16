// Read-only JSON byte measurement. Does not deserialize GreenLens through its
// admission services, rehash semantic records, alter a project, or inspect files
// referenced by the project. Invoke: node measure-manifest.cjs <project.json>
'use strict';
const fs = require('node:fs');
const crypto = require('node:crypto');
const input = process.argv[2];
if (!input || process.argv.length !== 3) throw Error('Provide exactly one existing project.json path');
const bytes = fs.readFileSync(input);
if (bytes.length > 16 * 1024 * 1024) throw Error('Existing manifest exceeds the audited history capacity');
const text = bytes.toString('utf8');
const project = JSON.parse(text);
const sha = value => crypto.createHash('sha256').update(value).digest('hex');
function space(index) { while (/\s/.test(text[index])) index++; return index; }
function end(index) {
  const first = text[index];
  if (first === '"') {
    for (let cursor = index + 1; cursor < text.length; cursor++) {
      if (text[cursor] === '\\') cursor++;
      else if (text[cursor] === '"') return cursor + 1;
    }
    throw Error('Unclosed JSON string');
  }
  if (first === '{' || first === '[') {
    let depth = 1;
    for (let cursor = index + 1; cursor < text.length; cursor++) {
      if (text[cursor] === '"') cursor = end(cursor) - 1;
      else if (text[cursor] === '{' || text[cursor] === '[') depth++;
      else if (text[cursor] === '}' || text[cursor] === ']') {
        if (--depth === 0) return cursor + 1;
      }
    }
    throw Error('Unclosed JSON container');
  }
  let cursor = index;
  while (cursor < text.length && !/[\s,}\]]/.test(text[cursor])) cursor++;
  return cursor;
}
function span(start) {
  const finish = end(start);
  const indent = text.slice(text.lastIndexOf('\n', start) + 1, start);
  return {start, end: finish, bytes: Buffer.byteLength(text.slice(start, finish)), indent: /^ *$/.test(indent) ? indent.length : null};
}
function fields(index) {
  if (text[index] !== '{') throw Error('Expected object');
  const result = {};
  index = space(index + 1);
  while (text[index] !== '}') {
    const keyEnd = end(index);
    const key = JSON.parse(text.slice(index, keyEnd));
    index = space(keyEnd);
    if (text[index] !== ':') throw Error('Expected colon');
    index = space(index + 1);
    if (Object.hasOwn(result, key)) throw Error('Duplicate object key');
    result[key] = span(index);
    index = space(result[key].end);
    if (text[index] === ',') index = space(index + 1);
  }
  return result;
}
function items(index) {
  if (text[index] !== '[') throw Error('Expected array');
  const result = [];
  index = space(index + 1);
  while (text[index] !== ']') {
    result.push(span(index));
    index = space(result[result.length - 1].end);
    if (text[index] === ',') index = space(index + 1);
  }
  return result;
}
const root = fields(space(0));
const architecture = fields(root.architecture.start);
const history = fields(architecture.scope_history.start);
const historySpans = items(history.revisions.start);
const activeHash = project.architecture.scope_history.active_revision_sha256;
const activeIndex = project.architecture.scope_history.revisions.findIndex(revision => revision.revision_sha256 === activeHash);
if (activeIndex < 0) throw Error('No active retained revision');
const active = project.architecture.scope_history.revisions[activeIndex];
const activeSpan = historySpans[activeIndex];
const revision = fields(activeSpan.start);
const provenance = fields(revision.provenance.start);
const rowSpans = items(provenance.rows.start);
if (rowSpans.length !== active.provenance.rows.length) throw Error('Row span mismatch');
const bindings = Object.entries(active.designs).map(([id, snapshot]) => {
  const current = project.energy.designs[id];
  // calendar_for uses assigned study.calendar first; no fallback is assumed.
  if (!current?.study) throw Error('This audit requires the declared assigned-study context');
  return {
    design_id: id,
    current_model_equals_snapshot: JSON.stringify(current.model) === JSON.stringify(snapshot.model),
    current_study_equals_snapshot: JSON.stringify(current.study) === JSON.stringify(snapshot.study),
    current_assigned_study_calendar_equals_snapshot: JSON.stringify(current.study.calendar) === JSON.stringify(snapshot.calendar),
    retained_model_sha256: active.scope.design_model_sha256[id],
  };
});
if (bindings.some(binding => !binding.current_model_equals_snapshot || !binding.current_study_equals_snapshot || !binding.current_assigned_study_calendar_equals_snapshot)) throw Error('Current context differs; this lower bound does not apply');
if (project.architecture.active_asset_id !== active.reference.asset_id) throw Error('Current reference differs');
if (JSON.stringify(project.architecture.scope) !== JSON.stringify(active.scope)) throw Error('Current scope projection differs');
let retainedRows = 0, retainedRawBytes = 0, firstLineIndentBytes = 0, shortestStatusSaving = 0;
let excludedCalendarRows = 0, excludedCalendarBytes = 0, excludedDeclaredRows = 0, excludedDeclaredBytes = 0;
for (let index = 0; index < rowSpans.length; index++) {
  const row = active.provenance.rows[index], measured = rowSpans[index];
  if (row.object.kind === 'calendar') { excludedCalendarRows++; excludedCalendarBytes += measured.bytes; continue; }
  if (row.source_declarations.length) { excludedDeclaredRows++; excludedDeclaredBytes += measured.bytes; continue; }
  if (measured.indent !== 14) throw Error('Unexpected provenance indentation; do not reuse this calculation');
  retainedRows++;
  retainedRawBytes += measured.bytes;
  firstLineIndentBytes += measured.indent;
  // 'assumed' and 'missing' are the shortest serialized status labels.
  // Allow that shortening even without the extra declarations it would need.
  shortestStatusSaving += Math.max(0, row.status.length - 7);
}
let modelsAndStudiesBytes = 0;
for (const design of Object.values(fields(revision.designs.start))) {
  const snapshot = fields(design.start);
  modelsAndStudiesBytes += snapshot.model.bytes + snapshot.study.bytes;
}
// Allow both existing thermal declaration entries to disappear from the active
// compatibility projection solely to make the lower bound more conservative.
// This is an arithmetic exclusion, NOT a project operation or recommendation.
const scopeFields = fields(architecture.scope.start);
const assumptionSpans = items(scopeFields.assumptions.start);
let projectionAllowance = 0, projectionAllowanceRows = 0;
for (let index = 0; index < assumptionSpans.length; index++) {
  if (project.architecture.scope.assumptions[index].link.kind === 'thermal_object') {
    const measured = assumptionSpans[index];
    projectionAllowance += measured.bytes + (measured.indent ?? 0) + 2;
    projectionAllowanceRows++;
  }
}
const headroom = 16 * 1024 * 1024 - bytes.length;
const lower = revision.reference.bytes + modelsAndStudiesBytes + retainedRawBytes + firstLineIndentBytes - shortestStatusSaving;
const netLower = lower - projectionAllowance;
const separators = activeSpan.indent + 2;
const result = {
  schema_version: 'greenlens.read-only-manifest-capacity-audit.v1',
  application_source: 'bb1edc92b3c5afe00b3efa965fc114904973c469',
  measurement_script_sha256: sha(fs.readFileSync(__filename)),
  input: {name: 'project.json', sha256: sha(bytes), bytes: bytes.length, retained_revisions: historySpans.length, retained_issues: project.report_issues.issues.length},
  recorded_context: {active_revision_sha256: activeHash, active_revision_schema: active.schema_version, reference_asset_id: active.reference.asset_id, reference_manifest_sha256: active.reference.manifest_sha256, active_reference_matches: true, active_scope_projection_matches: true, designs: bindings, first_issue: project.report_issues.issues.map(issue => ({id: issue.id, scope_revision_sha256: issue.scope_revision_sha256}))},
  limits: {manifest_bytes: 16777216, remaining_manifest_bytes: headroom, unchanged_shared_package_bytes: 268435456, unchanged_shared_entry_bytes: 67108864, unchanged_shared_entries: 5000},
  raw_active_revision: {bytes_at_actual_nesting: activeSpan.bytes, first_line_indent: activeSpan.indent, lines: text.slice(activeSpan.start, activeSpan.end).split('\n').length, same_size_append_with_separator_bytes: activeSpan.bytes + separators, comparable_projected_manifest_bytes: bytes.length + activeSpan.bytes + separators, comparable_overage_bytes: activeSpan.bytes + separators - headroom, blocks: Object.fromEntries(['reference','designs','scope','provenance','review_decisions'].map(key => [key, revision[key].bytes]))},
  conservative_lower_bound: {unchanged_reference_block_bytes: revision.reference.bytes, unchanged_model_and_study_blocks_without_calendar_bytes: modelsAndStudiesBytes, retained_nondeclared_noncalendar_rows: retainedRows, row_raw_bytes_without_first_line_indentation: retainedRawBytes, mandatory_first_line_indentation_bytes: firstLineIndentBytes, hypothetical_shortest_status_savings_bytes: shortestStatusSaving, excluded_calendar_rows: excludedCalendarRows, excluded_calendar_row_bytes: excludedCalendarBytes, excluded_source_declared_rows: excludedDeclaredRows, excluded_source_declared_row_bytes: excludedDeclaredBytes, excluded_further_content: ['all new scope content', 'all review decisions', 'all v3 reviewed_rows', 'all calendar snapshots', 'provenance envelope and row separators', 'revision envelope and separators', 'new report issue summary', 'new captured views and no-holidays policy rows'], bytes_before_active_projection_allowance: lower, active_projection_thermal_declaration_allowance_rows: projectionAllowanceRows, active_projection_thermal_declaration_allowance_bytes: projectionAllowance, net_minimum_growth_bytes: netLower, excess_over_manifest_headroom_bytes: netLower - headroom},
  qualifications: ['Conditional lower-bound measurement for the declared unchanged reference, A/B models, studies/calendar, retained mappings/reference assumptions and append-only history workflow.', 'No candidate was constructed, hashed, accepted or persisted. No semantic hash was recomputed. This is not a successful Rust admission or an evidence-integrity check.', 'No run/member traversal, application, CLI, renderer or simulation was executed. Only this project.json and this measurement script were read.', 'Exact Rust validation remains authoritative. No limits were raised and no data was pruned.', 'The issue-only estimate covers the largest prior issued product set; it does not budget a future revision.'],
  source_locations: [
    'crates/greenlens-runner/src/study_revisions.rs:45 bounded_manifest serializes pretty JSON with the trailing newline reserved under 16 MiB',
    'crates/greenlens-runner/src/study_revisions.rs:340 append validates the cloned candidate manifest capacity before return',
    'crates/greenlens-runner/src/study_revisions.rs:482 accept appends only after exact-context/candidate/plan validation',
    'crates/greenlens-runner/src/capacity.rs:268 ReportIssue estimates largest prior issue, without a proposed revision',
    'crates/greenlens-core/src/study_revision.rs:904 provenance_checklist_version preserves noncalendar rows across v2/v3; v3 adds the bounded calendar policy',
    'crates/greenlens-runner/src/energy_workbench.rs:87 calendar_for selects assigned study.calendar first',
    'crates/greenlens-runner/src/report_issues.rs:1426 final issue candidate validation precedes publication',
    'crates/greenlens-runner/src/persistence.rs:651 atomic_project_cancellable preserves history and checks exact publication capacity'
  ],
  conclusion: netLower > headroom ? 'The declared same-context append cannot fit the existing manifest limit, even under the stated conservative omissions. Do not spend a native issuance attempt to rediscover this boundary.' : 'No guaranteed capacity failure established by this lower bound; exact admission remains required.'
};
console.log(JSON.stringify(result, null, 2));