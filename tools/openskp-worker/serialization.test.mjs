import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import {pin,patchSource} from './patches.mjs';
import {patchSerialization} from './serialization-patches.mjs';
const source=process.env.GREENLENS_OPEN_SKP_AUDIT_SOURCE??path.join(process.env.LOCALAPPDATA,'GreenLens/dependencies/openskp-audit',pin,'packages/cpp');
test('pinned exporter uses counted JSON and one exact output buffer',()=>{
  const original=patchSource('src/instanced_glb.cpp',fs.readFileSync(path.join(source,'src/instanced_glb.cpp'),'utf8'));
  const patched=patchSerialization('src/instanced_glb.cpp',original);
  assert.equal(patched,patchSerialization('src/instanced_glb.cpp',original));
  assert.throws(()=>patchSerialization('src/instanced_glb.cpp',patched));
  const body=patched.slice(patched.indexOf('ByteBuffer to_instanced_glb('),patched.indexOf('void export_instanced_glb('));
  assert.ok(!/ostringstream|stream\.str|JsonToString|SerializeGltfBufferBin/.test(body));
  assert.ok(body.indexOf('if (options.textures)')<body.indexOf('make_model('));
  assert.match(body,/SerializeGltfModel/);assert.match(body,/encode_glb\(output, model\.buffers\[0\]\.data, kGlbLimit\)/);
});
test('worker fences original bytes without rereading a whole input or dumping JSON strings',()=>{
  const worker=fs.readFileSync(new URL('./worker.cpp',import.meta.url),'utf8');
  assert.equal((worker.match(/read_source\(input.value\)/g)??[]).length,1);
  assert.match(worker,/digest_held_file\(input.value,source_size/);
  assert.ok(!worker.includes('.dump('));
  assert.match(worker,/metadata\["source"\]=std::move\(source_inventory\)/);
  assert.ok(worker.indexOf('raw={};')<worker.indexOf('Json metadata='));
  assert.ok(worker.indexOf('digest_held_file(input.value')<worker.indexOf('write_new(stage/L"candidate.glb"'));
  assert.ok(worker.indexOf('write_new(stage/L"completion.json"')>worker.indexOf('write_new(stage/L"metadata.json"'));
  assert.match(worker,/failure_diagnostic\(error.what\(\)\)/);
});
