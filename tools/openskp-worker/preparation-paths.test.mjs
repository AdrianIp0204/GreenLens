// Original filesystem fixtures only. No build, SKP parsing, network or cleanup.
// All directories, junctions and diagnostics are retained outside the checkout.
import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import {randomUUID} from 'node:crypto';
import {validateNewExternalDestination} from './preparation-paths.mjs';

test('external preparation rejects existing, escaping and linked destinations before writes', () => {
  assert.equal(process.platform, 'win32', 'Windows junction checks are required, not skipped');
  assert.ok(process.env.LOCALAPPDATA?.trim(), 'An external LOCALAPPDATA test location is required');
  const local = process.env.LOCALAPPDATA;
  const external = path.resolve(local, 'GreenLens');
  const root = validateNewExternalDestination(path.join(external, 'verification', `preparation-paths-${randomUUID()}`));
  fs.mkdirSync(root, {recursive:true});
  const checks = [];
  try {
    for (const value of [undefined, null, '', '   '])
      assert.throws(() => validateNewExternalDestination(value), /destination is required/);
    try {
      delete process.env.LOCALAPPDATA;
      assert.throws(() => validateNewExternalDestination(path.join(root, 'missing-setting')), /LOCALAPPDATA is required/);
      process.env.LOCALAPPDATA = '   ';
      assert.throws(() => validateNewExternalDestination(path.join(root, 'blank-setting')), /LOCALAPPDATA is required/);
    } finally { process.env.LOCALAPPDATA = local; }
    checks.push('missing/blank destination and LOCALAPPDATA reject');

    const existingDirectory = path.join(root, 'existing');
    const existingFile = path.join(root, 'existing.txt');
    fs.mkdirSync(existingDirectory);
    fs.writeFileSync(existingFile, 'original synthetic sentinel\n', {flag:'wx'});
    for (const value of [root, existingDirectory, existingFile])
      assert.throws(() => validateNewExternalDestination(value), /Destination exists/);
    for (const value of [external, local, path.join(local, 'GreenLens-sibling', 'output'),
      path.join(external, '..', 'outside-output'), path.join(external, '..', 'GreenLens-sibling', 'output')])
      assert.throws(() => validateNewExternalDestination(value), /under LOCALAPPDATA\/GreenLens/);
    checks.push('existing files/directories, root, outside, sibling and escaping traversal reject');

    const nested = path.join(root, 'absent-parent', 'absent-child', 'output');
    assert.equal(validateNewExternalDestination(nested), path.resolve(nested));
    assert.equal(validateNewExternalDestination(path.join(root, 'uncreated', '..', 'normal-output')), path.join(root, 'normal-output'));
    assert.equal(fs.lstatSync(path.join(root, 'absent-parent'), {throwIfNoEntry:false}), undefined);
    assert.equal(fs.lstatSync(path.join(root, 'normal-output'), {throwIfNoEntry:false}), undefined);
    checks.push('valid missing ancestor levels normalize without writes');

    const liveTarget = path.join(root, 'live-target');
    const liveLink = path.join(root, 'live-junction');
    fs.mkdirSync(liveTarget);
    fs.writeFileSync(path.join(liveTarget, 'sentinel.txt'), 'unchanged target\n', {flag:'wx'});
    fs.symlinkSync(liveTarget, liveLink, 'junction');
    assert.ok(fs.lstatSync(liveLink).isSymbolicLink(), 'The fixture must be an actual junction');
    assert.throws(() => validateNewExternalDestination(liveLink), /Destination exists/);
    for (const value of [path.join(liveLink, 'output'), path.join(liveLink, 'missing-one', 'missing-two', 'output')])
      assert.throws(() => validateNewExternalDestination(value), /Reparse\/symlink destination ancestry/);
    assert.deepEqual(fs.readdirSync(liveTarget), ['sentinel.txt']);
    assert.equal(fs.readFileSync(path.join(liveTarget, 'sentinel.txt'), 'utf8'), 'unchanged target\n');
    checks.push('existing junction and higher junction above missing parents reject without target writes');

    const absentTarget = path.join(root, 'deliberately-absent-target');
    const danglingLink = path.join(root, 'dangling-junction');
    fs.symlinkSync(absentTarget, danglingLink, 'junction');
    assert.equal(fs.existsSync(danglingLink), false, 'The fixture must expose the old existsSync blind spot');
    assert.ok(fs.lstatSync(danglingLink).isSymbolicLink());
    assert.throws(() => validateNewExternalDestination(danglingLink), /Destination exists/);
    for (const value of [path.join(danglingLink, 'output'), path.join(danglingLink, 'missing-one', 'missing-two', 'output')])
      assert.throws(() => validateNewExternalDestination(value), /Reparse\/symlink destination ancestry/);
    assert.equal(fs.lstatSync(absentTarget, {throwIfNoEntry:false}), undefined);
    checks.push('dangling junction and higher dangling ancestor reject without creating its target');

    assert.equal(fs.readFileSync(existingFile, 'utf8'), 'original synthetic sentinel\n');
    assert.deepEqual(fs.readdirSync(existingDirectory), []);
    fs.writeFileSync(path.join(root, 'guard-tests.json'), JSON.stringify({
      schema_version:'greenlens.preparation-path-guards.v1', status:'passed', checks,
      skp_parses:0, worker_executions:0, builds:0, retained_artifacts:true,
    }, null, 2) + '\n', {flag:'wx'});
  } catch (error) {
    fs.writeFileSync(path.join(root, 'GUARD_TEST_FAILED.txt'), `${String(error?.stack ?? error)}\n`, {flag:'wx'});
    throw error;
  }
});
