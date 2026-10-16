import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {pin,replaceExactly,patchSource} from './patches.mjs';

const here=path.dirname(fileURLToPath(import.meta.url));
const source=process.env.GREENLENS_OPEN_SKP_AUDIT_SOURCE ?? path.join(process.env.LOCALAPPDATA ?? '', 'GreenLens/dependencies/openskp-audit',pin,'packages/cpp');
test('strict patch refuses missing and ambiguous anchors',()=>{
  assert.throws(()=>replaceExactly('x','y','z','fixture'),/found 0/);
  assert.throws(()=>replaceExactly('xx','x','z','fixture'),/found 2/);
  assert.equal(replaceExactly('abc','b','B','fixture'),'aBc');
});
const patchedFiles=['src/internal.hpp','src/core.cpp','src/tlv.cpp','src/geometry.cpp','src/legacy.cpp','src/face_groups.cpp','src/triangulator.cpp','src/earcut.cpp','include/openskp/instanced_scene.hpp','src/instanced_scene.cpp','src/instanced_glb.cpp','src/vff.cpp'];
for(const file of patchedFiles)test(`exact pinned source patch: ${file}`,()=>{
  assert.ok(fs.existsSync(path.join(source,file)),'Required pinned audit source unavailable; not skipped');
  const original=fs.readFileSync(path.join(source,file),'utf8');
  const output=patchSource(file,original);
  assert.notEqual(output,original.replaceAll('\r\n','\n'));
  assert.equal(output,patchSource(file,original),'Patch deterministic');
  assert.throws(()=>patchSource(file,output),/anchor|absent/,'Refuse double patch');
});
test('legacy and modern incomplete-list returns become fatal',()=>{
  const legacy=patchSource('src/legacy.cpp',fs.readFileSync(path.join(source,'src/legacy.cpp'),'utf8'));
  const list=legacy.slice(legacy.indexOf('entity_list(uint32_t'),legacy.indexOf('// True when the u16'));
  assert.ok(!list.includes('catch') && !list.includes('break;'));
  const tlv=patchSource('src/tlv.cpp',fs.readFileSync(path.join(source,'src/tlv.cpp'),'utf8'));
  assert.match(tlv,/Trailing incomplete TLV record/);assert.match(tlv,/Trailing incomplete flat TLV record/);
  assert.ok(!tlv.includes(' - p - 6) break;'));
});
test('no texture decoder dependency or source-tail retry in owned target',()=>{
  const cmake=fs.readFileSync(path.join(here,'CMakeLists.txt'),'utf8');
  assert.ok(!cmake.includes('FetchContent') && !cmake.includes('fragments_export'));
  const worker=fs.readFileSync(path.join(here,'worker.cpp'),'utf8');
  assert.match(worker,/JOB_OBJECT_LIMIT_JOB_MEMORY/);assert.match(worker,/JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE/);
  assert.match(worker,/CREATE_NEW/);assert.match(worker,/unaccepted_candidate/);
  assert.equal((worker.match(/openskp::full_parse\(/g)??[]).length,1);
});
