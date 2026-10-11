import { useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import type {
  ClimateCommands, Study, WeatherAssetInspection, WeatherInspection, WeatherIntervalView,
  WeatherReference, WeatherSource, WeatherSourceProfile,
} from '../climate-types';
import { draftFromStudy, fileLabel, matchingInterval, sameWeather, studyFromDraft, type StudyDraft } from './climate-study-form.mjs';
import './climate-study.css';
import { WeatherCalendarPicker } from './WeatherCalendarPicker';

export interface ClimateStudyPanelProps {
  study: Study | null;
  weather: WeatherReference[];
  path: string;
  disabled: boolean;
  onCommitStudy: (candidate: Study, label?: string) => Promise<boolean>;
  onImportWeather: (file: string, profile: WeatherSourceProfile, source: WeatherSource) => Promise<boolean>;
  onError: (message: string) => void;
}

const call = <K extends keyof ClimateCommands>(command: K, args: ClimateCommands[K]['args']) =>
  invoke<ClimateCommands[K]['result']>(command, args);
const message = (error: unknown) => error instanceof Error ? error.message : String(error);
const EMPTY_SOURCE: WeatherSource = { citation: '', source_url: null, license: '', rights_note: '', synthetic_non_climatic: false };
const METHOD = {
  static_overcast: 'Static daylight illuminance under a prescribed CIE standard overcast sky.',
  weather_hour: 'Selected-hour daylight illuminance from imported weather.',
  annual: 'Annual climate-based daylight illuminance and occupancy-weighted metrics.',
};

function Quality({ inspection }: { inspection: WeatherInspection }) {
  const { location, summary, time_policy: time } = inspection;
  return <>
    <dl className="climate-facts">
      <div><dt>Location</dt><dd>{location.city}, {location.country}</dd></div>
      <div><dt>Coordinates</dt><dd>{location.latitude_deg_north}° north / {location.longitude_deg_east}° east</dd></div>
      <div><dt>Elevation</dt><dd>{location.elevation_m} m</dd></div>
      <div><dt>Fixed UTC offset</dt><dd>{location.utc_offset_minutes > 0 ? '+' : ''}{location.utc_offset_minutes} minutes</dd></div>
      <div><dt>Validated intervals</dt><dd>{summary.intervals.toLocaleString('en')}</dd></div>
    </dl>
    {inspection.warnings.length > 0 && <ul className="climate-quality-warning">{inspection.warnings.map(warning => <li key={warning.code}>{warning.message} ({warning.count})</li>)}</ul>}
    <details className="climate-details"><summary>Time interpretation and quality</summary>
      <p>{time.interpretation}</p>
      <p>Fixed local standard time; no daylight-saving or operating-system timezone conversion. Source years in a typical year do not define a real calendar or weekdays.</p>
      <dl className="climate-facts">
        <div><dt>Source / canonical minute</dt><dd>{time.source_minute} / {time.canonical_minute}</dd></div>
        <div><dt>Interval duration</dt><dd>{time.interval_duration_minutes} min</dd></div>
        <div><dt>Representative offset from end</dt><dd>{time.representative_offset_from_interval_end_minutes} min</dd></div>
        <div><dt>Declared irradiation offset</dt><dd>{time.declared_irradiance_offset_hours === null ? 'None' : `${time.declared_irradiance_offset_hours} h`}</dd></div>
        <div><dt>Zero solar-input intervals</dt><dd>{summary.zero_solar_intervals}</dd></div>
        <div><dt>Dew point range</dt><dd>{summary.minimum_dew_point_c}–{summary.maximum_dew_point_c} °C</dd></div>
      </dl>
      <p>Source years by month: {summary.source_years_by_month.map((years, index) => `${index + 1}: ${years.join(', ')}`).join(' · ')}</p>
    </details>
  </>;
}

/** Mount with a project/alternative key. Drafts are never simulation inputs until Apply succeeds. */
export function ClimateStudyPanel({ study, weather, path, disabled, onCommitStudy, onImportWeather, onError }: ClimateStudyPanelProps) {
  const [base, setBase] = useState<Study | null>(study);
  const [defaults, setDefaults] = useState<Study | null>(null);
  const [draft, setDraft] = useState<StudyDraft | null>(study ? draftFromStudy(study, weather) : null);
  const [busy, setBusy] = useState(false), [error, setError] = useState(''), [status, setStatus] = useState('');
  const [asset, setAsset] = useState<WeatherAssetInspection | null>(null), [assetError, setAssetError] = useState('');
  const [interval, setInterval] = useState<WeatherIntervalView | null>(null), [intervalError, setIntervalError] = useState('');
  const [importOpen, setImportOpen] = useState(false), [file, setFile] = useState('');
  const [profile, setProfile] = useState<WeatherSourceProfile>('epw_hour_ending');
  const [inspection, setInspection] = useState<WeatherInspection | null>(null);
  const [source, setSource] = useState<WeatherSource>({ ...EMPTY_SOURCE });
  const context = useRef(0), inFlight = useRef(false), disabledNow = useRef(disabled);
  disabledNow.current = disabled;
  const studyKey = JSON.stringify(study);

  useEffect(() => {
    const generation = ++context.current;
    inFlight.current = false; setBusy(false); setError(''); setStatus('');
    setImportOpen(false); setFile(''); setInspection(null); setSource({ ...EMPTY_SOURCE });
    setBase(study); setDraft(study ? draftFromStudy(study, weather) : null);
    void call('study_defaults', {}).then(value => {
      if (context.current !== generation) return;
      setDefaults(value);
      if (!study) { setBase(value); setDraft(draftFromStudy(value, weather)); }
    }).catch(reason => { if (context.current === generation) setError(`Study defaults unavailable: ${message(reason)}`); });
    return () => { ++context.current; };
    // The serialized accepted study changes on undo/redo. A project/alternative
    // key supplied by the parent also resets equal-valued or legacy drafts.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [path, studyKey]);

  useEffect(() => {
    if (weather.length) setDraft(current => current && !current.weatherId ? { ...current, weatherId: weather[0].id } : current);
  }, [weather]);

  const selectedId = draft?.weatherId ?? '';
  useEffect(() => {
    let active = true;
    setAsset(null); setAssetError('');
    if (path && selectedId) void call('weather_asset_inspect', { path, weatherId: selectedId }).then(value => {
      if (active) setAsset(value);
    }).catch(reason => { if (active) setAssetError(message(reason)); });
    return () => { active = false; };
  }, [path, selectedId]);

  useEffect(() => {
    let active = true;
    setInterval(null); setIntervalError('');
    const index = draft?.interval.trim() ? Number(draft.interval) : NaN;
    if (draft?.mode !== 'weather_hour' || !path || !selectedId) return;
    if (!Number.isInteger(index) || index < 0 || index > 8759) { setIntervalError('Choose an interval index from 0 through 8759.'); return; }
    const timeout = window.setTimeout(() => {
      void call('weather_interval', { path, weatherId: selectedId, intervalIndex: index }).then(value => {
        if (active && value.weather_id === selectedId && value.interval.index === index) setInterval(value);
      }).catch(reason => { if (active) setIntervalError(message(reason)); });
    }, 250);
    return () => { active = false; window.clearTimeout(timeout); };
  }, [path, selectedId, draft?.mode, draft?.interval]);

  const update = <K extends keyof StudyDraft>(key: K, value: StudyDraft[K]) => {
    setDraft(current => current ? { ...current, [key]: value } : current); setStatus(''); setError('');
  };
  const fail = (reason: unknown) => { const text = message(reason); setError(text); onError(text); };
  const task = async (action: (stillCurrent: () => boolean) => Promise<void>) => {
    if (inFlight.current || disabledNow.current) return;
    const generation = context.current;
    const current = () => context.current === generation && !disabledNow.current;
    inFlight.current = true; setBusy(true); setError(''); setStatus('');
    try { await action(current); } catch (reason) { if (context.current === generation) fail(reason); }
    finally { if (context.current === generation) { inFlight.current = false; setBusy(false); } }
  };
  const apply = () => task(async current => {
    if (!base || !draft || !current()) return;
    const accepted = await onCommitStudy(studyFromDraft(base, draft, weather), 'Apply daylight study');
    if (current()) setStatus(accepted ? 'Study applied.' : 'Not applied. Review the validation message; your draft is retained.');
  });
  const chooseFile = () => task(async current => {
    const selected = await call('dialog_import_epw', { initialPath: file || path || null });
    if (selected && current()) { setFile(selected); setInspection(null); setSource({ ...EMPTY_SOURCE }); }
  });
  const inspectFile = () => task(async current => {
    setInspection(null);
    const inspected = await call('weather_inspect', { path: file, profile });
    if (current()) setInspection(inspected);
  });
  const readSource = () => task(async current => {
    const selected = await call('dialog_weather_source', { initialPath: file || path || null });
    if (!selected || !current()) return;
    const declaration = await call('weather_read_source', { path: selected });
    if (current()) setSource(declaration);
  });
  const importFile = () => task(async current => {
    if (!inspection || !file || !current()) return;
    const accepted = await onImportWeather(file, profile, { ...source, source_url: source.source_url?.trim() || null });
    if (current()) {
      if (accepted) { setImportOpen(false); setFile(''); setInspection(null); setStatus('Weather retained in this project. Select the study mode, then Apply.'); }
      else setStatus('Weather was not imported. The source declaration is retained here; review the document message before retrying.');
    }
  });
  const reset = () => { if (base) { setDraft(draftFromStudy(base, weather)); setError(''); setStatus('Draft reset to the accepted study.'); } };
  const useDaily = () => {
    if (!defaults) return;
    const initial = draftFromStudy(defaults, weather);
    setDraft(current => current ? { ...current, occupancyKind: 'daily', start: current.start || initial.start, end: current.end || initial.end } : current);
  };
  const locked = disabled || busy;
  const selectedReference = weather.find(value => value.id === selectedId);
  const inspectedAsset = asset && sameWeather(asset.asset.binding, selectedReference?.binding) ? asset : null;
  const currentInterval = draft?.mode === 'weather_hour' ? matchingInterval(interval, selectedReference, draft.interval) : null;
  const draftChanged = base && draft && JSON.stringify(draft) !== JSON.stringify(draftFromStudy(base, weather));

  return <section className="climate-study" aria-label="Daylight study">
    <div className="climate-heading"><h3>Daylight study</h3><small>Draft → Apply → Run</small></div>
    {!draft || !base ? <p className="climate-muted">Loading authoritative study settings…</p> : <form onSubmit={event => { event.preventDefault(); void apply(); }}>
      <fieldset disabled={locked}>
        <label>Calculation<select value={draft.mode} onChange={event => update('mode', event.target.value as StudyDraft['mode'])}>
          <option value="static_overcast">Static overcast</option><option value="weather_hour">Selected weather hour</option><option value="annual">Annual daylight</option>
        </select></label>
        <p className="climate-method">{METHOD[draft.mode]}</p>
        {draft.mode !== 'static_overcast' && <>
          <label>Retained weather<select value={selectedId} onChange={event => update('weatherId', event.target.value)}>
            <option value="">Select imported weather</option>{weather.map(item => <option key={item.id} value={item.id}>{item.id === selectedId && inspectedAsset ? `${inspectedAsset.inspection.location.city} · ` : ''}{item.id.slice(0, 8)}</option>)}
          </select></label>
          {!path && <p className="climate-muted">Save this project before importing weather.</p>}
          {!weather.length && <p className="climate-muted">Import one complete non-leap hourly EPW year below.</p>}
          {assetError && <p className="climate-error">{assetError}</p>}
          {inspectedAsset && <details className="climate-details"><summary>{inspectedAsset.inspection.location.city} · weather and source details</summary><Quality inspection={inspectedAsset.inspection}/><p>{inspectedAsset.asset.source.citation}</p><p>License: {inspectedAsset.asset.source.license}. {inspectedAsset.asset.source.rights_note}</p>{inspectedAsset.asset.source.synthetic_non_climatic && <p className="climate-quality-warning">Synthetic fixture; not climatic data.</p>}</details>}
          <label>Numerical profile<select value={draft.profile} onChange={event => update('profile', event.target.value as StudyDraft['profile'])}><option value="shipping">Shipping</option><option value="reference">Reference — higher sampling</option></select></label>
          {draft.mode === 'weather_hour' && <>
            <WeatherCalendarPicker path={path} weatherId={selectedId||null} current={currentInterval} disabled={locked} onSelect={index=>update('interval',String(index))}/>
            <label>Weather interval index (0–8759)<input inputMode="numeric" value={draft.interval} onChange={event => update('interval', event.target.value)}/></label>
            {intervalError && <p className="climate-error">{intervalError}</p>}
            {currentInterval && <div className="climate-interval"><strong>Month {currentInterval.hour.month}, day {currentInterval.hour.day} · hour ending {currentInterval.hour.hour_ending}:00</strong><p className="climate-muted">Source year {currentInterval.hour.source_year}; fixed local standard time. This selects a weather interval, not a forecast date.</p><details><summary>Exact interval mapping and inputs</summary><pre className="climate-mapping">{JSON.stringify(currentInterval.interval, null, 2)}</pre><p className="climate-muted">Mapping values are minutes on the unwrapped non-leap axis returned by Rust. DNI {currentInterval.hour.direct_normal_wh_m2}, DHI {currentInterval.hour.diffuse_horizontal_wh_m2}, GHI {currentInterval.hour.global_horizontal_wh_m2} Wh/m² per hourly interval.</p></details></div>}
          </>}
        </>}
        {draft.mode === 'annual' && <div className="climate-section"><h4>Occupancy and metric conventions</h4>
          <label>Occupancy<select value={draft.occupancyKind} onChange={event => event.target.value === 'daily' ? useDaily() : update('occupancyKind', 'mask')}><option value="daily">Daily local standard time</option><option value="mask">Explicit hourly weight mask</option></select></label>
          {draft.occupancyKind === 'daily' ? <><div className="climate-columns"><label>Start (HH:MM)<input value={draft.start} placeholder="08:00" onChange={event => update('start', event.target.value)}/></label><label>End (HH:MM)<input value={draft.end} placeholder="18:00" onChange={event => update('end', event.target.value)}/></label></div><p className="climate-muted">Every day, including occupied nighttime. An end before the start spans midnight. No inferred weekdays.</p></> : <label>8,760 weights in weather order<textarea className="climate-mask" value={draft.mask} maxLength={524288} placeholder="Paste a JSON array of 8,760 weights" onChange={event => update('mask', event.target.value)}/><span className="climate-muted">Each weight is 0–1, including fractions. Missing values and empty occupancy are rejected by Rust.</span></label>}
          <label>DA / cDA threshold (lx)<input inputMode="decimal" value={draft.da} onChange={event => update('da', event.target.value)}/></label>
          <div className="climate-columns"><label>UDI lower (lx)<input inputMode="decimal" value={draft.lower} onChange={event => update('lower', event.target.value)}/></label><label>UDI upper (lx)<input inputMode="decimal" value={draft.upper} onChange={event => update('upper', event.target.value)}/></label></div>
          <p className="climate-muted">DA includes its threshold; UDI includes both bounds. Underlit is below the lower bound; overlit is above the upper bound. Values are study conventions, not compliance limits.</p>
          <button type="button" disabled={!defaults} onClick={() => { if (defaults) { const initial = draftFromStudy(defaults, weather); setDraft({ ...draft, da: initial.da, lower: initial.lower, upper: initial.upper, occupancyKind: initial.occupancyKind, start: initial.start, end: initial.end, mask: initial.mask }); setStatus('Default conventions restored in the draft. Apply to accept.'); } }}>Use default conventions</button>
        </div>}
        {draftChanged && <p className="climate-muted">Unapplied draft. Run uses the accepted study until you Apply.</p>}
        <div className="climate-actions"><button type="submit" className="primary">{busy ? 'Working…' : 'Apply study'}</button><button type="button" onClick={reset}>Reset draft</button></div>
      </fieldset>
    </form>}
    {status && <p className="climate-status" role="status">{status}</p>}
    {error && <p className="climate-error" role="alert">{error}</p>}
    <button type="button" className="climate-import-toggle" aria-expanded={importOpen} disabled={locked || !path} onClick={() => setImportOpen(value => !value)}>{importOpen ? 'Close weather import' : 'Import local EPW…'}</button>
    {importOpen && <div className="climate-section"><h4>Import weather</h4><fieldset disabled={locked}>
      <button type="button" onClick={() => void chooseFile()}>Choose EPW file…</button>{file && <span className="climate-file" title={file}>{fileLabel(file)}</span>}
      <label>Source time profile<select value={profile} onChange={event => { setProfile(event.target.value as WeatherSourceProfile); setInspection(null); }}><option value="epw_hour_ending">EPW hourly interval ending</option><option value="pvgis_tmy">PVGIS TMY — supported offset interpretation</option></select></label>
      <p className="climate-muted">Choose the documented source convention. Unsupported offsets and incomplete years are rejected; no missing radiation is replaced with zero.</p>
      <button type="button" disabled={!file} onClick={() => void inspectFile()}>Inspect file and time policy</button>
      {inspection && <><Quality inspection={inspection}/><div className="climate-section"><h4>Source and reuse rights</h4><button type="button" onClick={() => void readSource()}>Load source declaration JSON…</button>
        <label>Citation<textarea value={source.citation} maxLength={4000} onChange={event => setSource({ ...source, citation: event.target.value })}/></label>
        <label>Source URL (optional)<input type="url" value={source.source_url ?? ''} onChange={event => setSource({ ...source, source_url: event.target.value || null })}/></label>
        <label>License or explicit unknown-rights note<input value={source.license} maxLength={4000} onChange={event => setSource({ ...source, license: event.target.value })}/></label>
        <label>Rights / redistribution note<textarea value={source.rights_note} maxLength={4000} onChange={event => setSource({ ...source, rights_note: event.target.value })}/></label>
        <label className="climate-check"><input type="checkbox" checked={source.synthetic_non_climatic} onChange={event => setSource({ ...source, synthetic_non_climatic: event.target.checked })}/>Original synthetic fixture, not climatic weather data</label>
        <p className="climate-muted">A file format is not a reuse license. Your declaration is retained with the original bytes; importing does not establish rights.</p>
        <div className="climate-actions"><button type="button" className="primary" disabled={!source.citation.trim() || !source.license.trim() || !source.rights_note.trim()} onClick={() => void importFile()}>Retain weather in project</button></div>
      </div></>}
    </fieldset></div>}
    <details className="climate-details"><summary>Scope and limitations</summary><p>Climate studies use fixed geometry and shading, one simplified glass pane, 8,760 non-leap hourly intervals and at most 256 sensors. Reference sampling is slower; it is not a guarantee of accuracy for arbitrary buildings.</p><p>Schedule and threshold changes can derive new metrics from matching annual evidence without repeating light transport. Weather-hour changes select a different calculation input; inspecting another hour of an existing annual run does not.</p><p>Metrics are not sDA, ASE, criterion evaluations or certification. The static overcast study has no simulated date or weather sequence.</p></details>
  </section>;
}

export default ClimateStudyPanel;
