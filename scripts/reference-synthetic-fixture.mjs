// Original synthetic reference geometry, Apache-2.0. No architectural source
// file is read. This is an importer/workflow fixture, not a private-case stand-in.
import fs from 'node:fs';
import path from 'node:path';
import crypto from 'node:crypto';

const destination=process.argv[2];
const usage='Usage: node scripts/reference-synthetic-fixture.mjs <new-output-directory> [--stress-triangles N]';
if(!destination)throw Error(usage);
const options=process.argv.slice(3);
let stressTriangles=null;
if(options.length){
  if(options.length!==2||options[0]!=='--stress-triangles'||!/^\d+$/.test(options[1]))throw Error(usage+'; N must be an integer from 1 through 1000001.');
  stressTriangles=Number(options[1]);
  if(!Number.isSafeInteger(stressTriangles)||stressTriangles<1||stressTriangles>1000001)throw Error('Stress triangle count must be from 1 through 1000001 (one above the admission triangle limit).');
}
if(fs.existsSync(destination))throw Error('Choose a new destination; existing files are preserved.');
if(stressTriangles!==null){
  // One original nondegenerate triangle, repeated exactly N times by indices.
  // This is a resource/admission probe, not a physical or architectural model.
  const positionBytes=36,indexBytes=stressTriangles*3*4;
  const binary=Buffer.alloc(positionBytes+indexBytes);
  [0,0,0,1,0,0,0,1,0].forEach((v,i)=>binary.writeFloatLE(v,i*4));
  for(let i=0;i<stressTriangles;i++){
    const offset=positionBytes+i*12;
    binary.writeUInt32LE(0,offset);binary.writeUInt32LE(1,offset+4);binary.writeUInt32LE(2,offset+8);
  }
  const label='Original synthetic repeated-triangle resource probe — physically meaningless';
  const document={asset:{version:'2.0',generator:'GreenLens original synthetic resource fixture v1'},
    buffers:[{byteLength:binary.length}],
    bufferViews:[{buffer:0,byteOffset:0,byteLength:positionBytes,target:34962},{buffer:0,byteOffset:positionBytes,byteLength:indexBytes,target:34963}],
    accessors:[{bufferView:0,componentType:5126,count:3,type:'VEC3',min:[0,0,0],max:[1,1,0]},{bufferView:1,componentType:5125,count:stressTriangles*3,type:'SCALAR',min:[0],max:[2]}],
    materials:[{name:'Synthetic resource probe only — no thermal properties',pbrMetallicRoughness:{baseColorFactor:[0.53,0.68,0.63,1],metallicFactor:0,roughnessFactor:1}}],
    meshes:[{name:label,primitives:[{attributes:{POSITION:0},indices:1,material:0,mode:4}]}],
    nodes:[{name:label,mesh:0}],scenes:[{nodes:[0]}],scene:0};
  const raw=Buffer.from(JSON.stringify(document));
  const metadata=Buffer.alloc(Math.ceil(raw.length/4)*4,0x20);raw.copy(metadata);
  const bytes=Buffer.alloc(28+metadata.length+binary.length);
  if(bytes.length>13_000_000)throw Error('Synthetic stress source exceeds its declared 13 MB generator budget.');
  bytes.writeUInt32LE(0x46546c67,0);bytes.writeUInt32LE(2,4);bytes.writeUInt32LE(bytes.length,8);
  bytes.writeUInt32LE(metadata.length,12);bytes.writeUInt32LE(0x4e4f534a,16);metadata.copy(bytes,20);
  bytes.writeUInt32LE(binary.length,20+metadata.length);bytes.writeUInt32LE(0x004e4942,24+metadata.length);binary.copy(bytes,28+metadata.length);
  const filename='synthetic-repeated-triangles.glb';
  const receipt={schema_version:'greenlens.synthetic-reference-fixture.v1',
    fixture_kind:'repeated_triangle_resource_probe',source:'Original GreenLens synthetic geometry',license:'Apache-2.0',
    filename,source_bytes:bytes.length,sha256:crypto.createHash('sha256').update(bytes).digest('hex'),
    physically_meaningful:false,requested_stress_triangles:stressTriangles,
    expected:{triangles:stressTriangles,occurrences:1,source_nodes:1,source_meshes:1,source_materials:1,source_textures:0,accessors:2,unique_positions:3,indices:stressTriangles*3,bounds_m:{min:[0,0,0],max:[1,0,1]}},
    coordinates:'glTF metres, X east, Y up, -Z north; default GreenLens transform [x,y,z] -> [x,-z,y]. No import scaling or rotation.',
    admission_expectation:stressTriangles===1000001?'Reject: triangle count exceeds the 1000000 admission limit.':'Evaluate admission and measured resource limits; generation does not establish successful import.',
    limits:'Physically meaningless repeated coincident triangles, each nondegenerate. Resource/admission probe only; no architectural fidelity, thermal inputs, scientific qualification or private-case coverage is implied. Maximum generated GLB is 13000000 bytes.'};
  fs.mkdirSync(path.dirname(destination),{recursive:true});
  fs.mkdirSync(destination);
  fs.writeFileSync(path.join(destination,filename),bytes,{flag:'wx'});
  fs.writeFileSync(path.join(destination,'fixture-receipt.json'),JSON.stringify(receipt,null,2)+'\n',{flag:'wx'});
  console.log(`Created original synthetic resource probe with exactly ${stressTriangles} repeated triangles (${bytes.length} bytes). Physically meaningless; not a case-study model.`);
  process.exit(0);
}
const vertices=[[0,0,0],[1,0,0],[1,1,0],[0,1,0],[0,0,1],[1,0,1],[1,1,1],[0,1,1]];
const faces=[[0,2,1],[0,3,2],[4,5,6],[4,6,7],[0,1,5],[0,5,4],[1,2,6],[1,6,5],[2,3,7],[2,7,6],[3,0,4],[3,4,7]];
const binary=Buffer.alloc(vertices.length*12+faces.length*6);
vertices.flat().forEach((v,i)=>binary.writeFloatLE(v,i*4));
faces.flat().forEach((v,i)=>binary.writeUInt16LE(v,vertices.length*12+i*2));
// glTF X east, Y up, -Z north. GreenLens maps [x,y,z] -> [x,-z,y].
// Four 8x6x3 m volumes on two storeys; appearance supplies no thermal properties.
const document={asset:{version:'2.0',generator:'GreenLens original synthetic reference fixture v1'},
  buffers:[{byteLength:binary.length}],bufferViews:[{buffer:0,byteOffset:0,byteLength:96,target:34962},{buffer:0,byteOffset:96,byteLength:72,target:34963}],
  accessors:[{bufferView:0,componentType:5126,count:8,type:'VEC3',min:[0,0,0],max:[1,1,1]}, {bufferView:1,componentType:5123,count:36,type:'SCALAR'}],
  materials:[{name:'Synthetic reference surface — no construction assignment',pbrMetallicRoughness:{baseColorFactor:[0.53,0.68,0.63,1],metallicFactor:0,roughnessFactor:1}}],
  meshes:[{primitives:[{attributes:{POSITION:0},indices:1,material:0}]}],
  nodes:[{name:'Original synthetic four-volume reference',children:[1,2]},
    {name:'Lower storey',children:[3,4]},{name:'Upper storey',translation:[0,3,0],children:[5,6]},
    {name:'West zone — repeated mass',mesh:0,translation:[0,0,-6],scale:[8,3,6]},
    {name:'East zone — mirrored repeated mass',mesh:0,translation:[16,0,-6],scale:[-8,3,6]},
    {name:'West zone — repeated mass',mesh:0,translation:[0,0,-6],scale:[8,3,6]},
    {name:'East zone — mirrored repeated mass',mesh:0,translation:[16,0,-6],scale:[-8,3,6]}],
  scenes:[{nodes:[0]}],scene:0};
const raw=Buffer.from(JSON.stringify(document));
const metadata=Buffer.alloc(Math.ceil(raw.length/4)*4,0x20);raw.copy(metadata);
const bytes=Buffer.alloc(28+metadata.length+binary.length);
bytes.writeUInt32LE(0x46546c67,0);bytes.writeUInt32LE(2,4);bytes.writeUInt32LE(bytes.length,8);
bytes.writeUInt32LE(metadata.length,12);bytes.writeUInt32LE(0x4e4f534a,16);metadata.copy(bytes,20);
bytes.writeUInt32LE(binary.length,20+metadata.length);bytes.writeUInt32LE(0x004e4942,24+metadata.length);binary.copy(bytes,28+metadata.length);
fs.mkdirSync(destination,{recursive:true});
fs.writeFileSync(path.join(destination,'synthetic-four-volumes.glb'),bytes);
fs.writeFileSync(path.join(destination,'fixture-receipt.json'),JSON.stringify({schema_version:'greenlens.synthetic-reference-fixture.v1',
  source:'Original GreenLens synthetic geometry',license:'Apache-2.0',sha256:crypto.createHash('sha256').update(bytes).digest('hex'),
  expected:{triangles:48,occurrences:7,bounds_m:{min:[0,0,0],max:[16,6,6]},volume_count:4},
  limits:'Geometric workflow fixture only. No thermal properties, scientific qualification or private-case coverage is implied.'},null,2)+'\n');
console.log('Created original synthetic GLB and expected-geometry receipt.');
