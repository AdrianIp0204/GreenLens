import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync, readdirSync } from 'node:fs';

// Read source text only: no workflow, subprocess, engine or network execution.
// This deliberately accepts only the workflows' existing block-mapping layout.
// Reject ambiguous/unsupported YAML rather than pretending to be a YAML parser.
const workflowDirectory = new URL('../.github/workflows/', import.meta.url);
const retiredJobs = new Map([
  ['windows.yml', ['native', 'selected_hour', 'annual', 'annual_convergence', 'annual_replay']],
  ['reference-a8504-pilot.yml', ['baseline']],
  ['reference-depth-order.yml', ['diagnostic', 'descriptive_aggregate']],
  ['reference-permutation.yml', ['diagnostic', 'fixed_comparisons']],
  ['reference-sensitivity.yml', ['sensitivity']],
]);
const currentJobs = new Map([
  ['energy-windows.yml', ['build', 'static_compatibility', 'energy']],
  ['observer-windows.yml', ['observer']],
]);
const disabledCondition = '${{ false }}';

function mapping(lines, indentation, label) {
  const entries = new Map();
  const prefix = ' '.repeat(indentation);
  const keyPattern = new RegExp(`^${prefix}([A-Za-z_][A-Za-z0-9_-]*):(?: +(.*))?$`);
  let active;
  for (const line of lines) {
    assert.ok(!line.includes('\t'), `${label}: tabs are unsupported`);
    if (/^\s*(?:#.*)?$/.test(line)) continue;
    const spaces = line.match(/^ */)[0].length;
    assert.ok(spaces >= indentation, `${label}: unexpected indentation`);
    if (spaces === indentation) {
      const match = keyPattern.exec(line);
      assert.ok(match, `${label}: unsupported mapping line ${line}`);
      assert.ok(!entries.has(match[1]), `${label}: duplicate key ${match[1]}`);
      active = { value: match[2] ?? '', lines: [] };
      entries.set(match[1], active);
    } else {
      assert.ok(active, `${label}: content before a mapping key`);
      active.lines.push(line);
    }
  }
  return entries;
}

function block(entries, key, indentation, label) {
  const entry = entries.get(key);
  assert.ok(entry, `${label}: missing ${key}`);
  assert.equal(entry.value, '', `${label}: ${key} must be a block mapping`);
  return mapping(entry.lines, indentation, `${label}.${key}`);
}

function source(filename) {
  return readFileSync(new URL(filename, workflowDirectory), 'utf8');
}

function workflow(text, label) {
  assert.ok(!text.includes('\r') || !/\r(?!\n)/.test(text), `${label}: lone CR`);
  const top = mapping(text.replaceAll('\r\n', '\n').split('\n'), 0, label);
  return { top, jobs: block(top, 'jobs', 2, label) };
}

function assertRetired(text, filename) {
  const { jobs } = workflow(text, filename);
  assert.deepEqual([...jobs.keys()], retiredJobs.get(filename), `${filename}: historical job inventory changed`);
  for (const [name, job] of jobs) {
    assert.equal(job.value, '', `${filename}.${name}: expected job block`);
    const fields = mapping(job.lines, 4, `${filename}.${name}`);
    assert.equal(fields.get('if')?.value, disabledCondition,
      `${filename}.${name}: consumed protocol must be unconditionally disabled`);
    assert.deepEqual(fields.get('if').lines, [], `${filename}.${name}: multiline condition is unsupported`);
  }
}

function assertCurrent(text, filename) {
  const { top, jobs } = workflow(text, filename);
  const events = block(top, 'on', 2, filename);
  const push = block(events, 'push', 4, filename);
  assert.deepEqual([...push.keys()], ['branches'], `${filename}: main verification cannot use path filters`);
  const branches = push.get('branches');
  assert.match(branches.value, /^\[[A-Za-z0-9_./, -]+\]$/, `${filename}: expected explicit branch list`);
  assert.deepEqual(branches.lines, [], `${filename}: unsupported branch continuation`);
  assert.ok(branches.value.slice(1, -1).split(',').map(value => value.trim()).includes('main'),
    `${filename}: main must receive current checks`);
  const pullRequest = events.get('pull_request');
  assert.ok(pullRequest, `${filename}: pull_request checks are required`);
  assert.equal(pullRequest.value, '', `${filename}: pull_request must be unfiltered`);
  assert.deepEqual(pullRequest.lines, [], `${filename}: pull_request must be unfiltered`);
  assert.deepEqual([...jobs.keys()], currentJobs.get(filename), `${filename}: current job inventory changed`);
  for (const [name, job] of jobs) {
    assert.equal(job.value, '', `${filename}.${name}: expected job block`);
    const fields = mapping(job.lines, 4, `${filename}.${name}`);
    assert.ok(!fields.has('if'), `${filename}.${name}: current checks cannot be conditionally skipped`);
    assert.ok(!fields.has('continue-on-error'), `${filename}.${name}: current checks must retain failures`);
    if (name === 'energy' || name === 'static_compatibility') {
      assert.equal(fields.get('needs')?.value, 'build', `${filename}.${name}: must depend on guarded build`);
    } else {
      assert.ok(!fields.has('needs'), `${filename}.${name}: independent gate cannot wait on retired jobs`);
      const steps = fields.get('steps');
      assert.equal(steps?.value, '', `${filename}.${name}: missing steps`);
      const firstRun = steps.lines.findIndex(line => /^        run:/.test(line));
      assert.ok(firstRun >= 0, `${filename}.${name}: missing source policy gate`);
      assert.ok(!steps.lines.slice(0, firstRun).some(line => /^        (?:if|continue-on-error):/.test(line)),
        `${filename}.${name}: source policy gate and its setup cannot be skipped or ignored`);
      assert.equal(steps.lines[firstRun], '        run: |');
      assert.equal(steps.lines[firstRun + 1], '          node --test scripts/test-consolidation-workflows.mjs',
        `${filename}.${name}: policy gate must precede scripts, builds and engines`);
      assert.equal(steps.lines[firstRun + 2], "          if ($LASTEXITCODE -ne 0) { throw 'Consolidated workflow policy failed' }",
        `${filename}.${name}: policy failure must stop the job`);
    }
  }
}

test('every executable workflow belongs to the reviewed consolidation inventory', () => {
  const actual = readdirSync(workflowDirectory).filter(name => /\.ya?ml$/i.test(name)).sort();
  assert.deepEqual(actual, [...retiredJobs.keys(), ...currentJobs.keys()].sort());
});

for (const filename of retiredJobs.keys()) {
  test(`${filename}: every retained historical job is inert`, () => assertRetired(source(filename), filename));
}

for (const filename of currentJobs.keys()) {
  test(`${filename}: main and PR checks stay enabled behind the early source guard`, () => assertCurrent(source(filename), filename));
}

test('the retired-workflow guard rejects removed, permissive, duplicate and extra job conditions', () => {
  const filename = 'reference-a8504-pilot.yml';
  const original = source(filename).replaceAll('\r\n', '\n');
  const condition = `    if: ${disabledCondition}\n`;
  for (const replacement of ['', '    if: always()\n', '    if: ${{ false || true }}\n',
    `${condition}    if: true\n`, '    if: |\n      false\n']) {
    assert.throws(() => assertRetired(original.replace(condition, replacement), filename));
  }
  assert.throws(() => assertRetired(`${original}\n  additional:\n    runs-on: windows-latest\n`, filename));
  assert.throws(() => workflow('jobs: { baseline: { if: false } }\n', 'unsupported flow mapping'));
});

test('the active-workflow guard rejects missing main/PR, disabled jobs and engine work before the guard', () => {
  const filename = 'energy-windows.yml';
  const original = source(filename).replaceAll('\r\n', '\n');
  for (const changed of [
    original.replace('branches: [main, ', 'branches: ['),
    original.replace('  pull_request:\n', ''),
    original.replace('  pull_request:\n', '  pull_request:\n    paths: [docs/**]\n'),
    original.replace('  build:\n', `  build:\n    if: ${disabledCondition}\n`),
    original.replace('    needs: build\n', '    needs: native\n'),
    original.replace('      - name: Prevent consumed scientific workflow execution\n',
      '      - name: Prevent consumed scientific workflow execution\n        if: false\n'),
    original.replace('node --test scripts/test-consolidation-workflows.mjs', './scripts/install-radiance.ps1'),
    original.replace("if ($LASTEXITCODE -ne 0) { throw 'Consolidated workflow policy failed' }", "Write-Output 'ignored'"),
  ]) assert.throws(() => assertCurrent(changed, filename));
});
