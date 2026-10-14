import { test } from 'node:test';
import assert from 'node:assert/strict';
import { describeInventoryDifference as difference } from './license-inventory-diff.mjs';

test('identical serialized inventories keep the strict gate and do not mutate inputs', () => {
  const expected = { packages: [{ name: 'original-fixture', installed: true }] };
  const snapshot = JSON.stringify(expected);
  assert.deepEqual(difference(expected, structuredClone(expected)), {
    identical: true, key_order_only: false, differences: [], truncated: false,
  });
  assert.equal(JSON.stringify(expected), snapshot);
});

test('a nested changed license digest identifies the exact relative JSON path', () => {
  const before = { packages: [{ license_evidence: [{ path: 'LICENSE', sha256: 'a'.repeat(64) }] }] };
  const after = structuredClone(before);
  after.packages[0].license_evidence[0].sha256 = 'b'.repeat(64);
  const result = difference(before, after);
  assert.equal(result.identical, false);
  assert.equal(result.key_order_only, false);
  assert.deepEqual(result.differences, [{
    path: 'packages[0].license_evidence[0].sha256', kind: 'value_changed',
    expected: JSON.stringify('a'.repeat(64)), actual: JSON.stringify('b'.repeat(64)),
  }]);
});

test('object key ordering alone is explicit and remains a strict failure', () => {
  const before = { packages: [{ native_archive_sha256: { alloc: 'a', core: 'b' } }] };
  const after = { packages: [{ native_archive_sha256: { core: 'b', alloc: 'a' } }] };
  const result = difference(before, after);
  assert.equal(result.identical, JSON.stringify(before) === JSON.stringify(after));
  assert.equal(result.identical, false);
  assert.equal(result.key_order_only, true);
  assert.deepEqual(result.differences, [{
    path: 'packages[0].native_archive_sha256', kind: 'object_key_order',
    expected: '"key 0: alloc"', actual: '"key 0: core"',
  }]);
});

test('mixed object ordering and semantic changes never become ordering-only', () => {
  const result = difference({ a: 1, b: 2 }, { b: 3, a: 1 });
  assert.equal(result.key_order_only, false);
  assert.deepEqual(result.differences.map(entry => [entry.path, entry.kind]), [
    ['[root]', 'object_key_order'], ['b', 'value_changed'],
  ]);
});

test('added and removed nested properties remain separate from missing versus null', () => {
  const result = difference({ p: { removed: true, stable: null } }, { p: { stable: null, added: null } });
  assert.deepEqual(result.differences, [
    { path: 'p.removed', kind: 'removed', expected: 'true', actual: '[missing]' },
    { path: 'p.added', kind: 'added', expected: '[missing]', actual: 'null' },
  ]);
});

test('array reorder is semantic, with positional additions and removals', () => {
  const reordered = difference({ p: ['first', 'second'] }, { p: ['second', 'first'] });
  assert.equal(reordered.key_order_only, false);
  assert.deepEqual(reordered.differences.map(entry => entry.path), ['p[0]', 'p[1]']);
  assert.deepEqual(difference([1], [1, 2]).differences, [
    { path: '[1]', kind: 'added', expected: '[missing]', actual: '2' },
  ]);
  assert.deepEqual(difference([1, 2], [1]).differences, [
    { path: '[1]', kind: 'removed', expected: '2', actual: '[missing]' },
  ]);
});

test('type changes retain false, zero, null and compact collection summaries', () => {
  assert.deepEqual(difference({ p: false }, { p: 0 }).differences, [
    { path: 'p', kind: 'type_changed', expected: 'false', actual: '0' },
  ]);
  assert.deepEqual(difference({ p: null }, { p: [] }).differences, [
    { path: 'p', kind: 'type_changed', expected: 'null', actual: 'array (0 entries)' },
  ]);
  assert.equal(difference(null, { p: true }).differences[0].actual, 'object (1 keys)');
});

test('diagnostics stop at twenty differences and bound summaries and paths', () => {
  const before = Object.fromEntries(Array.from({ length: 40 }, (_, index) => [`field${index}`, 'a'.repeat(1000)]));
  const after = Object.fromEntries(Object.keys(before).map(key => [key, 'b'.repeat(1000)]));
  const result = difference(before, after);
  assert.equal(result.differences.length, 20);
  assert.equal(result.truncated, true);
  assert.ok(result.differences.every(entry => entry.expected.length <= 120 && entry.actual.length <= 120));
  const longPath = difference({ ['x'.repeat(1000)]: 0 }, { ['x'.repeat(1000)]: 1 });
  assert.ok(longPath.differences[0].path.length <= 240);
  assert.equal(difference(before, after, { maxDifferences: 2, maxSummaryChars: 16 }).differences.length, 2);
  const exactlyTwenty = difference(Array(20).fill(0), Array(20).fill(1));
  assert.equal(exactlyTwenty.truncated, false);
});

test('small diagnostic options remain hard bounded and invalid options fail explicitly', () => {
  for (const maxDifferences of [0, -1, 21, 1.5, NaN, Infinity]) {
    assert.throws(() => difference({}, {}, { maxDifferences }));
  }
  for (const maxSummaryChars of [0, -1, 161, 1.5, NaN, Infinity]) {
    assert.throws(() => difference({}, {}, { maxSummaryChars }));
  }
  const short = difference('a'.repeat(100), 'b'.repeat(100), { maxSummaryChars: 1 });
  assert.equal(short.differences[0].expected.length, 1);
  for (const [before, after] of [[null, []], [null, {}], [[], [1]], [[1], []]]) {
    const diagnostic = difference(before, after, { maxSummaryChars: 1 }).differences[0];
    assert.ok(diagnostic.expected.length <= 1 && diagnostic.actual.length <= 1);
  }
});

test('absolute filesystem paths and controls cannot leak through summaries or unusual keys', () => {
  for (const value of ['C:\\private\\model', 'C:/private/model', '\\\\host\\private', '/home/private/model', 'prefix /tmp/private']) {
    const result = difference({ p: '' }, { p: value });
    assert.equal(result.differences[0].actual, '"[absolute path omitted]"');
    assert.ok(!JSON.stringify(difference({ [value]: 0 }, { [value]: 1 })).includes(value));
  }
  const result = difference({ p: '' }, { p: 'line\ncontrol\u0001' });
  assert.ok(!result.differences[0].actual.includes('\n'));
  assert.ok(!result.differences[0].actual.includes('\u0001'));
  assert.equal(difference({ p: '' }, { p: 'https://example.invalid/source' }).differences[0].actual,
    '"https://example.invalid/source"');
});

test('serialized JSON semantics and special property names match the unchanged strict check', () => {
  assert.equal(difference({ omitted: undefined, p: null }, { p: null }).identical, true);
  const before = JSON.parse('{"__proto__":{"v":1},"constructor":2}');
  const after = JSON.parse('{"constructor":2,"__proto__":{"v":1}}');
  assert.equal(difference(before, after).key_order_only, true);
  assert.throws(() => difference(undefined, {}), /JSON values/);
});
