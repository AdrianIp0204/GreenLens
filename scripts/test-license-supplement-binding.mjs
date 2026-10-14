import { test } from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { createHash } from 'node:crypto';
import { validateNpmSupplement } from './license-supplement-binding.mjs';

const hash = bytes => createHash('sha256').update(bytes).digest('hex');
function fixture(t) {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), 'greenlens-license-binding-'));
  t.after(() => fs.rmSync(directory, { recursive: true, force: true }));
  const manifest = JSON.stringify({ name: 'original-test-fixture', version: '1.0.0', license: 'ISC' });
  fs.writeFileSync(path.join(directory, 'package.json'), manifest);
  fs.writeFileSync(path.join(directory, 'LICENSE'), 'Original synthetic notice fixture.');
  const record = { ecosystem: 'npm', installed: true, name: 'original-test-fixture', version: '1.0.0', declared_license: 'ISC', source: 'https://example.invalid/original.tgz', integrity: `sha512-${Buffer.alloc(64).toString('base64')}` };
  const supplement = { declared_license: record.declared_license, source_download: record.source, source_integrity: record.integrity, package_files_sha256: { 'package.json': hash(manifest), LICENSE: hash('Original synthetic notice fixture.') }, redistribution: { review: 'manual_reviewed', requirements: ['Preserve this original fixture notice.'] } };
  return { directory, record, supplement };
}
test('exact npm review needs no Cargo metadata', t => {
  const f = fixture(t);
  assert.doesNotThrow(() => validateNpmSupplement(f.record, f.directory, f.supplement));
});
test('changed lock identity, license, ecosystem or installation cannot inherit review', t => {
  const f = fixture(t);
  for (const delta of [{ integrity: `sha512-${Buffer.alloc(64, 1).toString('base64')}` }, { source: 'https://example.invalid/other.tgz' }, { declared_license: 'MIT' }, { ecosystem: 'cargo' }, { installed: false }, { version: '2.0.0' }, { name: 'other' }]) {
    assert.throws(() => validateNpmSupplement({ ...f.record, ...delta }, f.directory, f.supplement));
  }
});
test('changed actual source or notice cannot inherit archived review', t => {
  const f = fixture(t);
  fs.appendFileSync(path.join(f.directory, 'LICENSE'), ' changed');
  assert.throws(() => validateNpmSupplement(f.record, f.directory, f.supplement));
});
test('unsafe and missing member bindings are rejected', t => {
  const f = fixture(t);
  for (const relative of ['../LICENSE', '/LICENSE', 'C:/LICENSE', 'a\\LICENSE', './LICENSE', 'missing']) {
    assert.throws(() => validateNpmSupplement(f.record, f.directory, { ...f.supplement, package_files_sha256: { ...f.supplement.package_files_sha256, [relative]: 'a'.repeat(64) } }));
  }
  assert.throws(() => validateNpmSupplement(f.record, f.directory, { ...f.supplement, package_files_sha256: {} }));
});
test('manual policy must retain explicit reviewed requirements', t => {
  const f = fixture(t);
  for (const redistribution of [{ review: 'approved', requirements: ['x'] }, { review: 'manual_reviewed', requirements: [] }, { review: 'manual_reviewed', requirements: [''] }]) {
    assert.throws(() => validateNpmSupplement(f.record, f.directory, { ...f.supplement, redistribution }));
  }
});
