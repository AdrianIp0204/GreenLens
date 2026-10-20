// Source-wiring checks supplement (not replace) the actual C/C++ unit target.
import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {minizPin,patchMiniz} from './patches.mjs';
const here=path.dirname(fileURLToPath(import.meta.url));
const source=process.env.GREENLENS_MINIZ_AUDIT_SOURCE ??
  path.join(process.env.LOCALAPPDATA??'','GreenLens/dependencies/openskp-audit',`miniz-${minizPin}`);
test('pinned miniz count guard runs after ZIP64 decoding and before directory allocation',()=>{
  const file=path.join(source,'miniz_zip.c');
  assert.ok(fs.existsSync(file),'Required pinned miniz source unavailable; not skipped');
  const input=fs.readFileSync(file,'utf8');
  const output=patchMiniz('miniz_zip.c',input);
  assert.equal(output,patchMiniz('miniz_zip.c',input));
  assert.throws(()=>patchMiniz('miniz_zip.c',output),/already present/);
  assert.throws(()=>patchMiniz('miniz_zip.c',input.replace('#include "miniz.h"','')),/found 0/);
  const count=output.indexOf('if (!greenlens_zip_entry_count_admitted(');
  assert.ok(count>output.indexOf('pZip->m_total_files = (mz_uint32)zip64_cdir_total_entries;'));
  assert.ok(count<output.indexOf('mz_zip_array_resize(pZip, &pZip->m_pState->m_central_dir,'));
  assert.ok(output.includes('buf = MZ_MALLOC(ext_data_size);'),'Do not pretend this repair replaces every allocator');
});
test('prepared worker includes the shared helper in both compiled dependency paths',()=>{
  const preparation=fs.readFileSync(path.join(here,'prepare-source.mjs'),'utf8');
  const cmake=fs.readFileSync(path.join(here,'CMakeLists.txt'),'utf8');
  assert.match(preparation,/openskp\/src\/preallocation_bounds\.h/);
  assert.match(preparation,/miniz\/preallocation_bounds\.h/);
  assert.match(preparation,/prepared_sha256:sha\(bounds\)/);
  assert.match(cmake,/foreach\(i RANGE 0/);
  assert.match(cmake,/file\(SHA256/);
  assert.match(preparation,/file === 'miniz_zip\.c'/);
});
test('native unit target is separate from worker and validates all prepared bytes',()=>{
  const cmake=fs.readFileSync(path.join(here,'tests/CMakeLists.txt'),'utf8');
  assert.match(cmake,/PREPARATION_FAILED/);
  assert.match(cmake,/file\(SHA256/);
  assert.ok(!cmake.includes('worker.cpp')&&!cmake.includes('openskp/src'));
  const source=fs.readFileSync(path.join(here,'tests/preallocation_test.cpp'),'utf8');
  assert.match(source,/mz_zip_reader_init_mem/);
  assert.match(source,/allocations\.calls == 1/);
  assert.match(source,/MZ_ZIP_TOO_MANY_FILES/);
  assert.match(source,/zip_case\(5000, true/);
  assert.match(source,/zip_case\(5001, true/);
});
