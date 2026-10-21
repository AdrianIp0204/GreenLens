// Source checks complement actual patched-miniz fixtures; no SKP parser runs.
import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import {patchMiniz, minizPin} from './patches.mjs';
import {patchMinizExtraBounds} from './miniz-allocation-patches.mjs';
const root = process.env.GREENLENS_MINIZ_AUDIT_SOURCE ??
  path.join(process.env.LOCALAPPDATA ?? '', 'GreenLens/dependencies/openskp-audit', `miniz-${minizPin}`);
const original = fs.readFileSync(path.join(root, 'miniz_zip.c'), 'utf8');

test('extra fields exceeding the directory reject before direct allocation/read', () => {
  const patched = patchMinizExtraBounds('miniz_zip.c', patchMiniz('miniz_zip.c', original));
  assert.equal(patched, patchMinizExtraBounds('miniz_zip.c', patchMiniz('miniz_zip.c', original)));
  const guard = patched.indexOf('if (MZ_ZIP_CENTRAL_DIR_HEADER_SIZE + filename_size + ext_data_size > n)');
  assert.ok(guard >= 0);
  assert.match(patched.slice(guard, guard + 215), /MZ_ZIP_INVALID_HEADER_OR_CORRUPTED/);
  assert.ok(guard < patched.indexOf('pExtra_data = p + MZ_ZIP_CENTRAL_DIR_HEADER_SIZE + filename_size;', guard));
  assert.ok(!patched.includes('MZ_MALLOC('), 'The pinned ZIP unit has no other direct allocator path');
  assert.ok(!patched.includes('MZ_FREE(buf)'), 'No obsolete cleanup may free a directory view');
  assert.ok(patched.includes('if ((total_header_size = MZ_ZIP_CENTRAL_DIR_HEADER_SIZE'), 'Retain full header/comment validation');
  assert.ok(patched.includes('greenlens_zip_entry_count_admitted'), 'Retain prior entry-count admission');
  assert.throws(() => patchMinizExtraBounds('miniz_zip.c', patched), /already present/);
});

test('exact-anchor drift fails closed; other sources only normalize line endings', () => {
  assert.throws(() => patchMinizExtraBounds('miniz_zip.c', original.replace('buf = MZ_MALLOC(ext_data_size);', 'buf = changed_allocator(ext_data_size);')), /found 0/);
  assert.throws(() => patchMinizExtraBounds('miniz_zip.c', original.replace('} while (extra_size_remaining);', '} while (changed);')), /missing pinned/);
  assert.equal(patchMinizExtraBounds('miniz.c', 'unchanged\r\n'), 'unchanged\n');
});
