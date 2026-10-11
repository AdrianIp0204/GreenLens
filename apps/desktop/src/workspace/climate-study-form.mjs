/** Form serialization only. Rust validates study bounds and calculates all metrics. */
export function minuteText(value) {
  return `${String(Math.floor(value / 60)).padStart(2, '0')}:${String(value % 60).padStart(2, '0')}`;
}

export function parseMinute(text) {
  const match = /^(\d{2}):(\d{2})$/.exec(text.trim());
  if (!match || Number(match[1]) > 24 || Number(match[2]) > 59 || (match[1] === '24' && match[2] !== '00')) {
    throw new Error('Enter a local standard time as HH:MM, from 00:00 through 24:00.');
  }
  return Number(match[1]) * 60 + Number(match[2]);
}

export function sameWeather(a, b) {
  return !!a && !!b && ['original_sha256', 'normalized_sha256', 'time_axis_sha256', 'interpretation'].every(key => a[key] === b[key]);
}

export function draftFromStudy(study, weather) {
  const calculation = study.calculation, occupancy = study.metrics.occupancy;
  return {
    mode: calculation.mode,
    weatherId: calculation.mode === 'static_overcast' ? weather[0]?.id ?? '' : weather.find(item => sameWeather(item.binding, calculation.weather))?.id ?? '',
    profile: calculation.mode === 'static_overcast' ? 'shipping' : calculation.profile,
    interval: String(calculation.mode === 'weather_hour' ? calculation.interval_index : 0),
    da: String(study.metrics.da_threshold_lux), lower: String(study.metrics.udi_lower_lux), upper: String(study.metrics.udi_upper_lux),
    occupancyKind: occupancy.kind,
    start: occupancy.kind === 'daily' ? minuteText(occupancy.start_minute) : '',
    end: occupancy.kind === 'daily' ? minuteText(occupancy.end_minute) : '',
    mask: occupancy.kind === 'mask' ? JSON.stringify(occupancy.weights) : '',
  };
}

function number(text, label) {
  if (!text.trim() || !Number.isFinite(Number(text))) throw new Error(`${label} needs a finite number.`);
  return Number(text);
}

export function studyFromDraft(base, draft, weather) {
  const result = structuredClone(base);
  if (draft.mode === 'static_overcast') result.calculation = { mode: 'static_overcast' };
  else {
    const selected = weather.find(item => item.id === draft.weatherId);
    if (!selected) throw new Error('Import and select a retained weather file first.');
    result.calculation = { mode: draft.mode, weather: structuredClone(selected.binding), profile: draft.profile };
    if (draft.mode === 'weather_hour') result.calculation.interval_index = number(draft.interval, 'Weather interval index');
  }
  // Preserve unused metric settings verbatim in static/hour modes.
  if (draft.mode === 'annual') {
    let occupancy;
    if (draft.occupancyKind === 'daily') occupancy = { kind: 'daily', start_minute: parseMinute(draft.start), end_minute: parseMinute(draft.end) };
    else {
      let weights;
      try { weights = JSON.parse(draft.mask); } catch { throw new Error('Occupancy mask must be a JSON array of 8,760 numeric weights.'); }
      if (!Array.isArray(weights) || weights.some(value => typeof value !== 'number' || !Number.isFinite(value))) {
        throw new Error('Occupancy mask must contain numeric weights, with no missing or nonfinite values.');
      }
      occupancy = { kind: 'mask', weights };
    }
    result.metrics = { da_threshold_lux: number(draft.da, 'DA threshold'), udi_lower_lux: number(draft.lower, 'UDI lower bound'), udi_upper_lux: number(draft.upper, 'UDI upper bound'), occupancy };
  }
  return result;
}

/** Omit directories from inspector labels; full paths remain local in controls. */
export function fileLabel(path) { return path.split(/[\\/]/).filter(Boolean).at(-1) ?? ''; }

/** A late response is displayable only for the exact current weather and index. */
export function matchingInterval(view, weather, indexText) {
  const index = indexText.trim() ? Number(indexText) : NaN;
  return view && weather && Number.isInteger(index) && view.weather_id === weather.id
    && view.interval.index === index && view.hour.index === index && sameWeather(view.binding, weather.binding) ? view : null;
}
