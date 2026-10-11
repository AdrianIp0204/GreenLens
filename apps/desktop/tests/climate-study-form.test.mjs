import test from 'node:test';
import assert from 'node:assert/strict';
import { draftFromStudy, fileLabel, matchingInterval, minuteText, parseMinute, sameWeather, studyFromDraft } from '../src/workspace/climate-study-form.mjs';

// Original mocked contracts test UI serialization only. Scientific validation,
// weather interpretation and metric arithmetic remain Rust responsibilities.
const binding = () => ({ original_sha256: 'a'.repeat(64), normalized_sha256: 'b'.repeat(64), time_axis_sha256: 'c'.repeat(64), interpretation: 'Synthetic fixture interpretation' });
const weather = () => [{ id: 'weather-a', path: 'weather/weather-a', binding: binding() }];
const base = () => ({ schema_version: 'greenlens.study.v0.3', calculation: { mode: 'static_overcast' }, metrics: { da_threshold_lux: 300, udi_lower_lux: 100, udi_upper_lux: 3000, occupancy: { kind: 'daily', start_minute: 480, end_minute: 1080 } } });

test('unchanged legacy defaults serialize without changing static or scientific fields', () => {
  const value = base(), retained = weather(), draft = draftFromStudy(value, retained);
  assert.deepEqual(studyFromDraft(value, draft, retained), value);
  assert.equal(draft.start, '08:00'); assert.equal(draft.end, '18:00');
  assert.equal(draft.weatherId, retained[0].id);
});

test('selected-hour serialization uses the complete retained binding and preserves inactive metrics', () => {
  const value = base(), original = structuredClone(value), retained = weather(), draft = draftFromStudy(value, retained);
  Object.assign(draft, { mode: 'weather_hour', profile: 'reference', interval: '8759', da: 'not used', mask: 'not used' });
  const candidate = studyFromDraft(value, draft, retained);
  assert.deepEqual(candidate.calculation, { mode: 'weather_hour', profile: 'reference', interval_index: 8759, weather: binding() });
  assert.deepEqual(candidate.metrics, value.metrics); assert.deepEqual(value, original);
  candidate.calculation.weather.original_sha256 = 'different';
  assert.equal(retained[0].binding.original_sha256, 'a'.repeat(64));
  draft.weatherId = 'removed'; assert.throws(() => studyFromDraft(value, draft, retained), /select a retained/);
});

test('daily clock entry preserves minutes, midnight and overnight direction without calculating weights', () => {
  for (const minutes of [0, 1, 479, 1080, 1439, 1440]) assert.equal(parseMinute(minuteText(minutes)), minutes);
  for (const bad of ['', '8:00', '24:01', '25:00', '12:60', '12:30junk']) assert.throws(() => parseMinute(bad), /HH:MM/);
  const value = base(), retained = weather(), draft = draftFromStudy(value, retained);
  Object.assign(draft, { mode: 'annual', start: '22:15', end: '06:30', da: '500', lower: '75', upper: '2500' });
  assert.deepEqual(studyFromDraft(value, draft, retained).metrics, { da_threshold_lux: 500, udi_lower_lux: 75, udi_upper_lux: 2500, occupancy: { kind: 'daily', start_minute: 1335, end_minute: 390 } });
});

test('explicit masks retain every fractional weight in order and defer scientific bounds to Rust', () => {
  const value = base(), retained = weather(), draft = draftFromStudy(value, retained), weights = Array(8760).fill(0);
  weights[0] = .25; weights[8759] = 1;
  Object.assign(draft, { mode: 'annual', occupancyKind: 'mask', mask: JSON.stringify(weights) });
  assert.deepEqual(studyFromDraft(value, draft, retained).metrics.occupancy, { kind: 'mask', weights });
  draft.mask = '[0, -1, 2]';
  assert.deepEqual(studyFromDraft(value, draft, retained).metrics.occupancy.weights, [0, -1, 2], 'length/range/denominator rejection belongs to the shared Rust commit');
  for (const bad of ['[0,]', 'null', '["1"]', '[null]', '[1e999]']) {
    draft.mask = bad; assert.throws(() => studyFromDraft(value, draft, retained), /mask/);
  }
});

test('missing or nonfinite numeric form text is not silently converted to zero or JSON null', () => {
  const value = base(), retained = weather(), draft = draftFromStudy(value, retained);
  draft.mode = 'annual';
  for (const bad of ['', ' ', 'NaN', 'Infinity', '1e999', '300 lx']) {
    draft.da = bad; assert.throws(() => studyFromDraft(value, draft, retained), /finite number/);
  }
});

test('draft restoration selects all four weather binding fields and preserves a saved mask', () => {
  const value = base(), retained = weather();
  value.calculation = { mode: 'annual', profile: 'reference', weather: binding() };
  value.metrics.occupancy = { kind: 'mask', weights: [.5, 0] };
  const draft = draftFromStudy(value, retained);
  assert.equal(draft.weatherId, retained[0].id); assert.equal(draft.mask, '[0.5,0]');
  for (const field of Object.keys(binding())) {
    const changed = binding(); changed[field] = 'other';
    assert.equal(sameWeather(binding(), changed), false);
    assert.equal(draftFromStudy(value, [{ ...retained[0], binding: changed }]).weatherId, '');
  }
});

test('late interval replies cannot display on another weather selection or index', () => {
  const selected = weather()[0], view = { weather_id: selected.id, binding: binding(), hour: { index: 42 }, interval: { index: 42 } };
  assert.equal(matchingInterval(view, selected, '42'), view);
  for (const index of ['', '41', '42.1', 'NaN']) assert.equal(matchingInterval(view, selected, index), null);
  assert.equal(matchingInterval(view, { ...selected, id: 'other' }, '42'), null);
  assert.equal(matchingInterval(view, { ...selected, binding: { ...binding(), time_axis_sha256: 'd'.repeat(64) } }, '42'), null);
  assert.equal(matchingInterval({ ...view, hour: { index: 41 } }, selected, '42'), null);
  assert.equal(matchingInterval(null, selected, '42'), null);
});

test('inspector filename labels omit both Windows and portable directory paths', () => {
  assert.equal(fileLabel('C:\\private\\weather\\year.epw'), 'year.epw');
  assert.equal(fileLabel('/private/weather/year.epw'), 'year.epw');
});
