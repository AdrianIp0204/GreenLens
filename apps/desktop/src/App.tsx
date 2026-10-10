import { lazy, Suspense, useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { invoke, isTauri } from '@tauri-apps/api/core';
import type { Comparison, JobStatus, Model, Project, ProjectView, RunRecord, Statistics, ValidatedModel } from './types';
import ModelEditor from './ModelEditor';
import { activeJob, luxColor, visibleResult } from './presentation.mjs';

const SceneView = lazy(() => import('./SceneView'));

const method = 'Static daylight illuminance under a prescribed CIE standard overcast sky.';
const number = (value: number, places = 1) => Number.isFinite(value) ? value.toLocaleString('en', { maximumFractionDigits: places, minimumFractionDigits: places }) : '—';
const errorText = (error: unknown) => typeof error === 'string' ? error : error instanceof Error ? error.message : JSON.stringify(error);
const json = (value: unknown) => JSON.stringify(value, null, 2);

function Stats({ statistics, prefix = '' }: { statistics: Statistics; prefix?: string }) {
  return <dl className="stats">{(['min', 'mean', 'max'] as const).map(key => <div key={key}><dt>{prefix}{key}</dt><dd>{number(statistics[key])}<small> lx</small></dd></div>)}</dl>;
}
function Legend({ maximum }: { maximum: number }) {
  return <div className="legend"><div className="legend-strip" /><div><span>0 lx</span><span>{number(maximum / 2)} lx</span><span>{number(maximum)} lx</span></div></div>;
}
function Heatmap({ run, maximum, caption }: { run: RunRecord; maximum: number; caption: string }) {
  const { rows, columns, actual_spacing_x_m, actual_spacing_y_m } = run.validated.sensor_grid;
  return <figure className="heatmap"><figcaption>{caption}</figcaption><div className="heatmap-grid" style={{ gridTemplateColumns: `repeat(${columns}, 1fr)`, aspectRatio: `${columns * actual_spacing_x_m}/${rows * actual_spacing_y_m}` }}>
    {Array.from({ length: rows }, (_, y) => rows - 1 - y).flatMap(y => Array.from({ length: columns }, (_, x) => {
      const i = y * columns + x;
      return <span key={i} style={{ backgroundColor: luxColor(run.lux[i], maximum) }} title={`${run.validated.sensor_grid.sensors[i].id}: ${number(run.lux[i])} lx`} />;
    }))}
  </div><p className="hint">Local +Y ↑ · window at bottom · {run.lux.length} sensors</p></figure>;
}

export default function App() {
  const [project, setProject] = useState<Project | null>(null);
  const [revision, setRevision] = useState<string | null>(null);
  const [results, setResults] = useState<RunRecord[]>([]);
  const [alternative, setAlternative] = useState('A');
  const [validated, setValidated] = useState<ValidatedModel | null>(null);
  const [validationError, setValidationError] = useState('');
  const [validating, setValidating] = useState(false);
  const [job, setJob] = useState<JobStatus | null>(null);
  const [engineDir, setEngineDir] = useState('');
  const [engineInfo, setEngineInfo] = useState<unknown>(null);
  const [directory, setDirectory] = useState('');
  const [savedDirectory, setSavedDirectory] = useState('');
  const [destination, setDestination] = useState('');
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState('');
  const [error, setError] = useState('');
  const [view, setView] = useState<'model' | 'compare' | 'evidence'>('model');
  const [comparison, setComparison] = useState<Comparison | null>(null);
  const [comparisonError, setComparisonError] = useState('');
  const [selectedSensor, setSelectedSensor] = useState<number | null>(null);
  const [historicalId, setHistoricalId] = useState('');
  const [dirty, setDirty] = useState(false);
  const collecting = useRef<string | null>(null);
  const attemptedCollection = useRef<string | null>(null);
  const projectRef = useRef(project); const revisionRef = useRef(revision); const directoryRef = useRef(savedDirectory);
  useEffect(() => { projectRef.current = project; revisionRef.current = revision; directoryRef.current = savedDirectory; }, [project, revision, savedDirectory]);

  const applyView = useCallback((value: ProjectView, path: string) => {
    setProject(value.project); setRevision(value.revision); setResults(value.results); setSavedDirectory(path); setDirectory(path); setDirty(false);
    projectRef.current = value.project; revisionRef.current = value.revision; directoryRef.current = path;
  }, []);
  const initialize = useCallback(async () => {
    setError('');
    if (!isTauri()) { setError('Open the native GreenLens application to use its Rust backend. A browser preview cannot run or verify simulations.'); return; }
    try {
      const value = await invoke<Project>('example');
      setProject(value); setRevision(null); setResults([]); setJob(null); setDirectory(''); setSavedDirectory(''); setAlternative('A'); setHistoricalId(''); setDirty(true); setMessage('Synthetic example ready. Choose a project directory, then run Alternative A.');
    } catch (e) { setError(errorText(e)); }
  }, []);
  useEffect(() => { void initialize(); }, [initialize]);
  const model = project?.alternatives.find(a => a.id === alternative)?.model ?? null;
  useEffect(() => {
    if (!model) return;
    let live = true; setValidating(true); setValidationError(''); setValidated(null);
    const timer = window.setTimeout(() => {
      invoke<ValidatedModel>('validate', { model }).then(value => { if (live) { setValidated(value); setValidationError(''); } }).catch(e => { if (live) setValidationError(errorText(e)); }).finally(() => { if (live) setValidating(false); });
    }, 180);
    return () => { live = false; window.clearTimeout(timer); };
  }, [model]);
  const running = activeJob(job);
  useEffect(() => {
    if (!running) return;
    let live = true;
    const timer = window.setInterval(() => { invoke<JobStatus>('job_status').then(value => { if (live) setJob(value); }).catch(e => { if (live) setError(errorText(e)); }); }, 500);
    return () => { live = false; window.clearInterval(timer); };
  }, [running]);

  const collect = useCallback(async (status: JobStatus) => {
    const p = projectRef.current, path = directoryRef.current;
    if (!p || !path || !status.result || collecting.current === status.id) return;
    collecting.current = status.id; attemptedCollection.current = status.id; setBusy(true);
    try {
      const value = await invoke<ProjectView>('collect_run', { path, project: p, expectedRevision: revisionRef.current });
      applyView(value, path); setMessage(`Alternative ${status.result.alternative_id} completed and evidence saved.`);
    } catch (e) { setError(`Calculation completed, but evidence was not saved: ${errorText(e)}. Use Save completed evidence after resolving the conflict.`); }
    finally { collecting.current = null; setBusy(false); }
  }, [applyView]);
  useEffect(() => { if (job?.state === 'completed' && attemptedCollection.current !== job.id) void collect(job); }, [job, collect]);

  const records = useMemo(() => {
    const all = [...results];
    if (job?.result && !all.some(run => run.id === job.result?.id)) all.push(job.result);
    return all;
  }, [results, job]);
  const latestA = records.filter(r => r.alternative_id === 'A').at(-1) ?? null;
  const latestB = records.filter(r => r.alternative_id === 'B').at(-1) ?? null;
  const latest = alternative === 'A' ? latestA : latestB;
  const selectedRun = records.find(run => run.id === historicalId && run.alternative_id === alternative) ?? latest;
  const currentResult = visibleResult(validated, selectedRun, validating) as RunRecord | null;
  const stale = Boolean(selectedRun && (!validated || selectedRun.validated.scientific_input_hash !== validated.scientific_input_hash));
  const uncollected = Boolean(job?.result && !results.some(run => run.id === job.result?.id));
  useEffect(() => {
    let live = true; setComparison(null); setComparisonError('');
    if (latestA && latestB) invoke<Comparison>('compare_runs', { a: latestA, b: latestB }).then(value => { if (live) setComparison(value); }).catch(e => { if (live) setComparisonError(errorText(e)); });
    return () => { live = false; };
  }, [latestA, latestB]);
  const scale = comparison?.scale_max_lux ?? currentResult?.statistics.max ?? 0;

  const action = async (body: () => Promise<void>) => { setBusy(true); setError(''); setMessage(''); try { await body(); } catch (e) { setError(errorText(e)); } finally { setBusy(false); } };
  const save = async () => {
    if (!project || !directory.trim()) throw new Error('Enter a project directory first.');
    const value = await invoke<ProjectView>('save_project', { path: directory.trim(), project, expectedRevision: directory.trim() === savedDirectory ? revision : null });
    applyView(value, directory.trim()); return value;
  };
  const run = () => action(async () => {
    if (!model || !validated || validating) throw new Error('Resolve model validation before running.');
    if (uncollected) throw new Error('Save the completed evidence before starting another run.');
    await save();
    attemptedCollection.current = null;
    const status = await invoke<JobStatus>('start_run', { model, alternativeId: alternative, engineDir: engineDir.trim() || null });
    setJob(status); setHistoricalId(''); setMessage(`Alternative ${alternative} queued. One local simulation runs at a time.`);
  });
  const updateModel = (value: Model) => { if (!project) return; setProject({ ...project, alternatives: project.alternatives.map(a => a.id === alternative ? { ...a, model: value } : a) }); setDirty(true); setSelectedSensor(null); };

  return <div className="app">
    <header className="topbar"><div className="brand"><span className="brand-mark" aria-hidden="true">◉</span><strong>GreenLens</strong><span className="version">v0.1</span></div><span className="project-title">{project?.name ?? 'Local daylight workspace'}{dirty ? ' · unsaved edits' : ''}</span><div className="top-actions"><button disabled={busy || running || uncollected} onClick={() => void action(initialize)}>New example</button><button disabled={busy || running || uncollected || !directory} onClick={() => void action(async () => { const value = await invoke<ProjectView>('open_project', { path: directory.trim() }); applyView(value, directory.trim()); setJob(null); setHistoricalId(''); setMessage('Project and checksummed evidence reopened.'); })}>Open project</button><button disabled={busy || running || !project || !directory || Boolean(validationError)} onClick={() => void action(async () => { await save(); setMessage('Project saved.'); })}>Save project</button></div></header>
    <div className="method-bar"><span className="status-dot" />{method}<span className="method-note">Local calculation · no annual or certification claim</span></div>
    <div className="project-paths"><label>Project directory<input aria-label="Project directory" placeholder="Choose a portable project folder, e.g. D:\Daylight study" value={directory} disabled={running || busy} onChange={e => setDirectory(e.target.value)} /></label><details className="engine-settings"><summary>Engine setup</summary><div><label>Radiance directory<input aria-label="Radiance directory" placeholder="Automatic local detection, or installed engine folder" value={engineDir} disabled={running} onChange={e => setEngineDir(e.target.value)} /></label><button disabled={busy || running} onClick={() => void action(async () => { const info = await invoke('doctor', { engineDir: engineDir.trim() || null }); setEngineInfo(info); setMessage('Engine inspection returned. See configuration details.'); })}>Check engine</button><p className="hint">Official Radiance 6.0.2 must be installed separately. The path remains machine configuration.</p>{engineInfo !== null && <pre className="small-pre">{json(engineInfo)}</pre>}</div></details></div>
    {(error || message) && <div className={`notice ${error ? 'error' : 'success'}`} role={error ? 'alert' : 'status'}>{error || message}<button aria-label="Dismiss message" onClick={() => { setError(''); setMessage(''); }}>×</button></div>}
    <main className="workspace">
      <aside className="inspector"><div className="inspector-heading"><span className="eyebrow">DESIGN ALTERNATIVE</span><div className="segmented">{['A', 'B'].map(id => <button key={id} aria-pressed={alternative === id} disabled={running || busy} onClick={() => { setAlternative(id); setHistoricalId(''); setSelectedSensor(null); }}>{id === 'A' ? 'A · Baseline' : 'B · Alternative'}</button>)}</div></div>
        {model && <ModelEditor model={model} onChange={updateModel} disabled={running || busy} />}
        <div className="run-panel">{validationError ? <div className="validation-error" role="alert">{validationError}</div> : <p className="validation-state">{validating ? 'Validating geometry…' : validated ? `${validated.sensor_grid.sensors.length} validated sensors · ${number(validated.sensor_grid.actual_spacing_x_m, 3)} × ${number(validated.sensor_grid.actual_spacing_y_m, 3)} m cells` : 'Waiting for the Rust backend.'}</p>}
          <button className="primary run-button" disabled={busy || running || !validated || validating || !directory.trim() || uncollected} onClick={() => void run()}>Run Alternative {alternative}<span>→</span></button>
          {!directory.trim() && <p className="hint">Enter a project directory to retain evidence.</p>}
          {uncollected && <button disabled={busy || collecting.current !== null} onClick={() => job && void collect(job)}>Save completed evidence</button>}
        </div>
      </aside>
      <section className="analysis"><nav className="view-tabs" aria-label="Analysis view"><button aria-current={view === 'model' ? 'page' : undefined} onClick={() => setView('model')}>Model & results</button><button aria-current={view === 'compare' ? 'page' : undefined} onClick={() => setView('compare')}>A/B comparison</button><button aria-current={view === 'evidence' ? 'page' : undefined} onClick={() => setView('evidence')}>Evidence & export</button><span className={`job-state ${job?.state ?? 'idle'}`} aria-live="polite">{job ? `Job ${job.state}` : 'Ready'}</span></nav>
        <div className="analysis-scroll">
          {view === 'model' && <>
            <div className="view-heading"><div><span className="eyebrow">ALTERNATIVE {alternative}</span><h1>{currentResult ? 'Workplane illuminance' : 'Room & sensor model'}</h1></div><div className="view-metadata">{validated && <><span>{number(validated.model.room.width_m)} × {number(validated.model.room.depth_m)} × {number(validated.model.room.height_m)} m</span><span>{number(validated.model.sky.exterior_horizontal_illuminance_lux, 0)} lx exterior</span></>}</div></div>
            {stale && <div className="stale-banner" role="status">Historical result is stale relative to the current inputs. Its heatmap is hidden on this geometry. Run again, or inspect its immutable snapshot in Evidence.</div>}
            <Suspense fallback={<div className="scene-shell"><div className="scene-empty">Loading local WebGL2 view…</div></div>}><SceneView validated={validated} lux={currentResult?.lux ?? null} scale={scale} selectedSensor={selectedSensor} onSelect={setSelectedSensor} /></Suspense>
            {currentResult ? <><div className="result-summary"><Stats statistics={currentResult.statistics} /><div><span className="eyebrow">{comparison ? 'SHARED A/B SCALE' : 'ILLUMINANCE SCALE'}</span><Legend maximum={scale} /></div></div></> : <p className="empty-results">{running ? 'Radiance is calculating. Results appear only after every sensor and process exit passes validation.' : 'Run the native Radiance calculation to populate illuminance values.'}</p>}
            {validated && <section className="sensor-section"><div className="section-heading"><h2>Sensor table</h2><span>{validated.sensor_grid.sensors.length} equal-area cells · {number(validated.sensor_grid.cell_area_m2, 4)} m² per cell</span></div><div className="table-scroll"><table><thead><tr><th>Sensor</th><th>X (m)</th><th>Y (m)</th><th>Z (m)</th><th>Illuminance (lx)</th></tr></thead><tbody>{validated.sensor_grid.sensors.map((sensor, i) => <tr key={sensor.id} className={selectedSensor === i ? 'selected-row' : ''} onClick={() => setSelectedSensor(i)}><td><button onClick={() => setSelectedSensor(i)}>{sensor.id}</button></td>{sensor.position.map((v, axis) => <td key={axis}>{number(v, 3)}</td>)}<td>{currentResult ? number(currentResult.lux[i], 3) : '—'}</td></tr>)}</tbody></table></div></section>}
          </>}
          {view === 'compare' && <><div className="view-heading"><div><span className="eyebrow">MATCHED SENSOR EVIDENCE</span><h1>Alternative A / B</h1></div></div><p className="body-note">Comparison uses the latest immutable completed run for each alternative. Geometry, sky, environment, sensors, engine and numerical settings must meet the Rust comparison rules.</p>
            {comparisonError && <div className="notice error" role="alert">{comparisonError}</div>}
            {comparison && latestA && latestB ? <><div className="comparison-maps"><div><Heatmap run={latestA} maximum={comparison.scale_max_lux} caption="A · Baseline" /><Stats statistics={latestA.statistics} /></div><div><Heatmap run={latestB} maximum={comparison.scale_max_lux} caption="B · Alternative" /><Stats statistics={latestB.statistics} /></div></div><div className="shared-legend"><span className="eyebrow">SHARED ILLUMINANCE SCALE</span><Legend maximum={comparison.scale_max_lux} /></div><div className="section-heading"><h2>Absolute difference · B − A</h2><span>Lux · percentage differences omitted</span></div><Stats statistics={comparison.statistics} prefix="Δ " /><p className="hint">Reflective shading can increase some sensor values. A design difference does not imply a uniform reduction.</p><div className="table-scroll"><table><thead><tr><th>Sensor</th><th>A (lx)</th><th>B (lx)</th><th>B − A (lx)</th></tr></thead><tbody>{comparison.deltas_lux.map((delta, i) => <tr key={i}><td>{latestA.validated.sensor_grid.sensors[i].id}</td><td>{number(latestA.lux[i], 3)}</td><td>{number(latestB.lux[i], 3)}</td><td>{number(delta, 3)}</td></tr>)}</tbody></table></div><details className="evidence-details"><summary>Comparison conditions and design differences</summary><pre>{json({ conditions: comparison.conditions, design_differences: comparison.design_differences })}</pre></details></> : !comparisonError && <div className="empty-state"><span>A + B</span><h2>Run both alternatives to compare</h2><p>Only completed real calculations can populate this view.</p></div>}
          </>}
          {view === 'evidence' && <><div className="view-heading"><div><span className="eyebrow">PORTABLE & INSPECTABLE</span><h1>Scientific evidence</h1></div><span>{results.length} saved runs</span></div><p className="body-note">Runs retain normalized inputs, generated text files, raw RGB, lux results, process logs, provenance and checksums. Export includes an offline HTML report and replay recipe. Engine binaries are installed separately.</p><div className="export-row"><label>Evidence ZIP destination<input aria-label="Evidence ZIP destination" placeholder="Full path ending in .zip" value={destination} onChange={e => setDestination(e.target.value)} /></label><button className="primary" disabled={busy || running || !savedDirectory || !destination || !results.length || uncollected} onClick={() => void action(async () => { await save(); const output = await invoke<{ path: string }>('export_project', { path: directory.trim(), destination }); setMessage(`Evidence exported to ${output.path}`); })}>Export evidence</button></div>
            <div className="section-heading"><h2>Immutable run snapshots</h2><span>Alternative {alternative}</span></div><label className="run-select">Saved / completed run<select aria-label="Run snapshot" value={selectedRun?.id ?? ''} onChange={e => setHistoricalId(e.target.value)}>{records.filter(r => r.alternative_id === alternative).map(run => <option value={run.id} key={run.id}>{run.id} · {results.some(r => r.id === run.id) ? 'saved' : 'pending save'}</option>)}</select></label>
            {selectedRun ? <><Stats statistics={selectedRun.statistics} /><div className="evidence-snapshot"><Heatmap run={selectedRun} maximum={selectedRun.statistics.max} caption={`Historical snapshot · Alternative ${selectedRun.alternative_id}`} /><div><p>Displayed with its original inputs and sensor positions.</p><p className="hash">Input: {selectedRun.validated.scientific_input_hash}</p><p className="hash">Sensors: {selectedRun.validated.sensor_hash}</p><p className="hint">This snapshot remains immutable if you edit the current alternative.</p></div></div><details className="evidence-details"><summary>Input snapshot</summary><pre>{json(selectedRun.model)}</pre></details><details className="evidence-details"><summary>Provenance, engine identity & numerical settings</summary><pre>{json(selectedRun.provenance)}</pre></details></> : <p className="empty-results">No completed run for this alternative.</p>}
            <div className="limitations"><h2>Method & limits</h2><p>{method} The prescribed sky is azimuth-symmetric and has no simulated date or annual time sequence. Geometry is one rectangular room, one window, one optional overhang and one optional detached obstruction.</p><p>Equivalent single-pane neutral glazing and diffuse surfaces simplify real materials. Analytical fixtures establish limited behavior; agreement with a direct engine invocation verifies integration. Neither establishes accuracy for arbitrary buildings.</p><p>A simulation metric is not a criterion evaluation. A criterion evaluation is not formal certification.</p></div>
          </>}
        </div>
        <section className="job-panel"><div className="job-heading"><strong>Calculation log</strong><span className={`job-state ${job?.state ?? 'idle'}`}>{job?.state ?? 'No active job'}</span>{running && <button className="cancel" onClick={() => void action(async () => { setJob(await invoke<JobStatus>('cancel_run')); setMessage('Cancellation requested. The full engine process tree is being stopped.'); })}>Cancel calculation</button>}</div><pre aria-live="polite">{job ? [...job.log, ...(job.error ? [`ERROR: ${job.error}`] : [])].join('\n') : 'A native Radiance job will record preparation, process execution and validation here.'}</pre></section>
      </section>
    </main><footer><span>GreenLens · local-first daylight analysis</span><span>Metres · lux · UTC provenance · Apache-2.0</span></footer>
  </div>;
}
