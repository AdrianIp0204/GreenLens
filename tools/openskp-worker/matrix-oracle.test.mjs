// Source-only tests with original in-memory bytes. No SKP reader or process.
import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import {fileURLToPath} from 'node:url';
import {MiB,crc32,tlv,ownedZip,negativeCases,modernPrefix,unsupportedVersion,
  clippedArea,triangleOverlap,inspectGlb,verifyGeometry,sha256} from './matrix-oracle.mjs';

function fakeModern(){return Buffer.concat([Buffer.from('Owned synthetic header\0'),Buffer.from('{26.2.243}','utf16le'),ownedZip([{name:'model.dat',data:Buffer.from([1,2,3])}])]);}
test('owned ZIP CRC matches standard independently known vector',()=>assert.equal(crc32(Buffer.from('123456789')),0xcbf43926));
test('owned ZIP stores declared resource size separately from actual bytes',()=>{
  const zip=ownedZip([{name:'model.dat',data:Buffer.from([5]),declared:64*MiB+1}]);
  assert.equal(zip.readUInt32LE(18),1);assert.equal(zip.readUInt32LE(22),64*MiB+1);
  const central=zip.indexOf(Buffer.from([0x50,0x4b,1,2]));assert.equal(zip.readUInt32LE(central+24),64*MiB+1);
});
test('fixed matrix contains exactly slots5–20 and original modifications',()=>{
  const modern=fakeModern(), originals=[Buffer.alloc(42,9),modern,Buffer.alloc(46,7),modern], rows=negativeCases(originals);
  assert.deepEqual(rows.map(x=>x.slot),Array.from({length:16},(_,i)=>i+5));
  assert.equal(rows[0].bytes.length,21);assert.equal(rows[4].bytes.length,45);
  assert.equal(rows[6].bytes[0],8);assert.equal(rows[8].zero_bytes,128*MiB+1);
  assert.ok(rows.every(x=>x.expect==='reject'));assert.equal(rows[14].name,'entry-count');
  assert.equal(rows[14].bytes.readUInt16LE(rows[14].bytes.length-12),5001);
});
test('unsupported version modifies only the predeclared two UTF16 digits',()=>{
  const input=fakeModern(),output=unsupportedVersion(input);assert.equal(output.length,input.length);
  assert.equal(output.subarray(0,modernPrefix(input).length).includes(Buffer.from('{99.2.243}','utf16le')),true);
  assert.throws(()=>unsupportedVersion(Buffer.alloc(40)),/exact modern encoded/);
});
test('TLV fixture encodes deliberate range failure without parser invocation',()=>{
  const bytes=tlv('F601',Buffer.alloc(0),1);assert.equal(bytes.length,6);assert.equal(bytes.readUInt32LE(2),1);
});
test('oversized owned fixture requests fail before construction',()=>{
  assert.throws(()=>ownedZip(Array.from({length:5002},()=>({name:'x'}))),/entry budget/);
  assert.throws(()=>ownedZip([{name:'x',data:Buffer.alloc(4097)}]),/member bound/);
});
test('rectangle intersection detects triangles crossing a hole despite centroid outside',()=>{
  const triangle=[[0,0,0],[4,0,0],[0,4,0]];
  assert.equal(clippedArea(triangle,[1,1,2,2]),1);
  assert.equal(clippedArea(triangle,[5,5,6,6]),0);
});
test('triangle overlap distinguishes shared edges from duplicated coverage',()=>{
  const a=[[0,0],[1,0],[0,1]],b=[[1,0],[1,1],[0,1]];
  assert.equal(triangleOverlap(a,b),0);assert.equal(triangleOverlap(a,a),0.5);
});

function ownedGlb(){
  const positions=[0,0,0, 2,0,0, 2,0,-1, 0,0,-1], normals=Array.from({length:4},()=>[0,1,0]).flat(), indices=[0,1,2,0,2,3];
  const bin=Buffer.alloc((positions.length+normals.length+indices.length)*4);let o=0;
  for(const v of [...positions,...normals]){bin.writeFloatLE(v,o);o+=4;}for(const v of indices){bin.writeUInt32LE(v,o);o+=4;}
  const json={asset:{version:'2.0'},scene:0,scenes:[{nodes:[0]}],nodes:[{mesh:0}],meshes:[{primitives:[{attributes:{POSITION:0,NORMAL:1},indices:2,mode:4}]}],buffers:[{byteLength:bin.length}],
    bufferViews:[{buffer:0,byteOffset:0,byteLength:48},{buffer:0,byteOffset:48,byteLength:48},{buffer:0,byteOffset:96,byteLength:24}],
    accessors:[{bufferView:0,componentType:5126,type:'VEC3',count:4,min:[0,0,-1],max:[2,0,0]},{bufferView:1,componentType:5126,type:'VEC3',count:4},{bufferView:2,componentType:5125,type:'SCALAR',count:6}]};
  return {json,bin};
}
function encode({json,bin}){
  const text=Buffer.from(JSON.stringify(json)), padded=Buffer.alloc(Math.ceil(text.length/4)*4,32);text.copy(padded);
  const head=Buffer.alloc(20),bh=Buffer.alloc(8);head.writeUInt32LE(0x46546c67);head.writeUInt32LE(2,4);head.writeUInt32LE(28+padded.length+bin.length,8);head.writeUInt32LE(padded.length,12);head.writeUInt32LE(0x4e4f534a,16);bh.writeUInt32LE(bin.length);bh.writeUInt32LE(0x004e4942,4);return Buffer.concat([head,padded,bh,bin]);
}
const identity=[1,0,0,0,0,1,0,0,0,0,1,0,0,0,0,1];
function ownedMetadata(){return {source:{root_face_inventory:[{loop_edge_counts:[4]}],definition_inventory:[],placed_instance_count:0},scene:{node_count:1,expanded_triangles:2,bounds_gltf_m:{min:[0,0,-1],max:[2,0,0]},occurrences:[{glb_node_index:0,occurrence_path:'0',source_hidden:false,tag_hidden:false,local_matrix_gltf:identity,world_matrix_gltf:identity}]}};}
test('original GLB mathematical area is checked without granting missing simple coordinate evidence',()=>{
  const bytes=encode(ownedGlb()),result=verifyGeometry(bytes,ownedMetadata(),'simple-limited');
  assert.equal(result.area_m2,2);assert.equal(result.status,'limited_oracle_pass');assert.ok(result.unassessed.includes('origin'));
});
test('missing source face or changed metadata does not pass limited oracle',()=>{
  const bytes=encode(ownedGlb()),meta=ownedMetadata();meta.source.root_face_inventory=[];
  assert.throws(()=>verifyGeometry(bytes,meta,'simple-limited'),/one source face/);
  const bad=ownedMetadata();bad.scene.occurrences[0].glb_node_index=1;assert.throws(()=>verifyGeometry(bytes,bad,'simple-limited'),/identity/);
});
test('GLB range, nonfinite, unsupported extension and hierarchy corruption fail closed',()=>{
  const truncated=encode(ownedGlb()).subarray(0,-1);assert.throws(()=>inspectGlb(truncated),/GLB/);
  let fixture=ownedGlb();fixture.json.accessors[0].count=100;assert.throws(()=>inspectGlb(encode(fixture)),/range/);
  fixture=ownedGlb();fixture.bin.writeFloatLE(NaN,0);assert.throws(()=>inspectGlb(encode(fixture)),/Nonfinite/);
  fixture=ownedGlb();fixture.json.extensionsRequired=['fake'];assert.throws(()=>inspectGlb(encode(fixture)),/Unsupported/);
  fixture=ownedGlb();fixture.json.nodes[0].children=[0];assert.throws(()=>inspectGlb(encode(fixture)),/cycle/);
});
test('collector cannot launch a worker, importer, renderer or arbitrary executable',()=>{
  const source=fs.readFileSync(fileURLToPath(new URL('./matrix-oracle.mjs',import.meta.url)),'utf8');
  assert.ok(!source.includes('child_process'));assert.ok(!source.includes('execFile'));assert.ok(!source.includes('spawn('));
  assert.match(source,/prepared_not_executed/);assert.match(source,/execution_and_resource_acceptance/);
  assert.equal(sha256(Buffer.from('abc')),'ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad');
});
