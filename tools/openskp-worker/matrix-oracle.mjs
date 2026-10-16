// Original GreenLens synthetic oracle/collector, Apache-2.0. No process launch.
// This does not parse SKP geometry. It prepares owned adversarial bytes and
// independently inspects an already produced candidate GLB. Not an app importer.
import fs from 'node:fs';
import path from 'node:path';
import {createHash} from 'node:crypto';
import {fileURLToPath} from 'node:url';

export const schema = 'greenlens.openskp-development-matrix.v1';
export const MiB = 1024 * 1024;
const identity = [1,0,0,0, 0,1,0,0, 0,0,1,0, 0,0,0,1];
const check = (ok, message) => { if (!ok) throw new Error(message); };
export const sha256 = value => createHash('sha256').update(value).digest('hex');
const equal = (a,b,message) => check(JSON.stringify(a) === JSON.stringify(b), message);
export const close = (a,b,label) => check(Number.isFinite(a) && Math.abs(a-b) <= Math.max(1e-6,1e-6*Math.abs(b)), `${label}: ${a} differs from ${b}`);
export const complexOracle = Object.freeze({root_instances:4, reachable_nested_definitions:3,
  placed_instances_including_groups:8, expanded_faces:6, expanded_surface_area_m2:64,
  bounds_m:{min:[0,0,0],max:[13,13,2]}});

function safeFile(file, max) {
  const stat=fs.lstatSync(file); check(stat.isFile() && !stat.isSymbolicLink() && stat.size>0 && stat.size<=max,'Missing, nonregular or oversized input');
  return fs.readFileSync(file);
}
function fileDigest(file) {
  const fd=fs.openSync(file,'r'), block=Buffer.alloc(1024*1024), h=createHash('sha256');
  try { for(let n; (n=fs.readSync(fd,block,0,block.length,null))>0;)h.update(block.subarray(0,n)); } finally { fs.closeSync(fd); }
  return h.digest('hex');
}
function writeNew(file,value) { fs.writeFileSync(file,value,{flag:'wx'}); }
function jsonNew(file,value) { writeNew(file,JSON.stringify(value,null,2)+'\n'); }
function externalNew(directory) {
  const allowed=fs.realpathSync(path.join(process.env.LOCALAPPDATA ?? '', 'GreenLens'));
  const parent=fs.realpathSync(path.dirname(path.resolve(directory)));
  const relative=path.relative(allowed,parent);
  check(relative==='' || (!relative.startsWith('..') && !path.isAbsolute(relative)),'New external destination must be under LOCALAPPDATA/GreenLens');
  check(!fs.existsSync(directory),'Preserve existing matrix directory; no overwrite/resume here');
  fs.mkdirSync(directory);
}
export function crc32(bytes) {
  let crc=0xffffffff;
  for (const byte of bytes) { crc^=byte; for(let bit=0;bit<8;bit++)crc=(crc>>>1)^((crc&1)?0xedb88320:0); }
  return (crc^0xffffffff)>>>0;
}
// Stored entries, known owned payloads. Declared lengths deliberately diverge
// only in negative resource fixtures; no decompressor/parser is invoked here.
export function ownedZip(entries) {
  check(entries.length<=5001,'Fixture entry budget');
  let offset=0;const locals=[],centrals=[];
  for(const e of entries){
    const name=Buffer.from(e.name,'utf8'), data=e.data ?? Buffer.alloc(0);
    check(name.length>0 && name.length<256 && data.length<=4096,'Fixture ZIP member bound');
    const declared=e.declared ?? data.length;
    check(Number.isSafeInteger(declared) && declared>=0 && declared<=0xffffffff,'ZIP declaration bound');
    const local=Buffer.alloc(30), central=Buffer.alloc(46), crc=crc32(data);
    local.writeUInt32LE(0x04034b50);local.writeUInt16LE(20,4);local.writeUInt16LE(0x800,6);
    local.writeUInt32LE(crc,14);local.writeUInt32LE(data.length,18);local.writeUInt32LE(declared,22);local.writeUInt16LE(name.length,26);
    central.writeUInt32LE(0x02014b50);central.writeUInt16LE(20,4);central.writeUInt16LE(20,6);central.writeUInt16LE(0x800,8);
    central.writeUInt32LE(crc,16);central.writeUInt32LE(data.length,20);central.writeUInt32LE(declared,24);central.writeUInt16LE(name.length,28);central.writeUInt32LE(offset,42);
    locals.push(local,name,data);centrals.push(central,name);offset+=local.length+name.length+data.length;
  }
  const central=Buffer.concat(centrals), end=Buffer.alloc(22);
  end.writeUInt32LE(0x06054b50);end.writeUInt16LE(entries.length,8);end.writeUInt16LE(entries.length,10);
  end.writeUInt32LE(central.length,12);end.writeUInt32LE(offset,16);
  return Buffer.concat([...locals,central,end]);
}
export function tlv(tag,payload,declared=payload.length) {
  const head=Buffer.alloc(6);Buffer.from(tag,'hex').copy(head);head.writeUInt32LE(declared,2);return Buffer.concat([head,payload]);
}
export function modernPrefix(bytes) {
  const offset=bytes.subarray(0,4096).indexOf(Buffer.from([0x50,0x4b,3,4]));
  check(offset>0,'Native modern fixture has no bounded ZIP prefix');return bytes.subarray(0,offset);
}
export function unsupportedVersion(bytes) {
  const needle=Buffer.from('{26.2.243}','utf16le'), at=bytes.subarray(0,4096).indexOf(needle);
  check(at>=0 && bytes.subarray(at+needle.length,4096).indexOf(needle)<0,'Expected exact modern encoded version once');
  const out=Buffer.from(bytes);Buffer.from('99','utf16le').copy(out,at+2);return out;
}
export function negativeCases(originals) {
  check(originals.length===4 && originals.every(b=>b.length>8 && b.length<=128*MiB),'Four fixed originals required');
  const cases=originals.map((b,i)=>({slot:i+5,name:`half-${i+1}`,bytes:b.subarray(0,Math.floor(b.length/2)),expect:'reject'}));
  for(let i=2;i<4;i++)cases.push({slot:i+7,name:`tail-minus-one-${i+1}`,bytes:originals[i].subarray(0,-1),expect:'reject'});
  const magic=Buffer.from(originals[0]);magic[0]^=1;cases.push({slot:11,name:'bad-magic',bytes:magic,expect:'reject'});
  cases.push({slot:12,name:'unsupported-version',bytes:unsupportedVersion(originals[3]),expect:'reject'});
  cases.push({slot:13,name:'oversized-source',zero_bytes:128*MiB+1,expect:'reject'});
  const prefix=modernPrefix(originals[3]);
  const zipCase=(slot,name,entries)=>cases.push({slot,name,bytes:Buffer.concat([prefix,ownedZip(entries)]),expect:'reject'});
  zipCase(14,'entry-budget',[{name:'model.dat',data:Buffer.from([0]),declared:64*MiB+1}]);
  zipCase(15,'duplicate-name',[{name:'model.dat'},{name:'model.dat'}]);
  let chain=tlv('0000',Buffer.alloc(0));for(let i=0;i<65;i++)chain=tlv('F401',chain);
  zipCase(16,'depth-65',[{name:'model.dat',data:tlv('F601',chain)}]);
  zipCase(17,'cross-range',[{name:'model.dat',data:tlv('F601',Buffer.alloc(0),1)}]);
  zipCase(18,'aggregate-budget',Array.from({length:5},(_,i)=>({name:i===0?'model.dat':`owned-${i}`,data:Buffer.from([0]),declared:64*MiB})));
  zipCase(19,'entry-count',Array.from({length:5001},(_,i)=>({name:i===0?'model.dat':`owned-${i}`})));
  zipCase(20,'missing-model',[{name:'owned-unrelated.dat'}]);return cases;
}

export function prepareMatrix(simpleDirectory,complexDirectory,destination) {
  const simpleAuditBytes=safeFile(path.join(simpleDirectory,'authoring-receipt.json'),MiB);
  const complexAuditBytes=safeFile(path.join(complexDirectory,'native-authoring-receipt.json'),MiB);
  const expectedBytes=safeFile(path.join(complexDirectory,'expected-before-authoring.json'),MiB);
  const simple=JSON.parse(simpleAuditBytes), actual=JSON.parse(complexAuditBytes), expected=JSON.parse(expectedBytes);
  check(simple.application==='26.2.243' && simple.face_count===1 && simple.edge_count===4 && simple.area_m2===2 && simple.legacy.saved===true && simple.modern.saved===true,'Simple independent native authoring receipt differs');
  check(expected.schema==='greenlens.independent-skp-geometry.v1' && actual.application==='26.2.243','Complex independent source identity differs');
  for(const key of Object.keys(complexOracle))equal(expected[key],complexOracle[key],`Predeclared complex ${key} differs`);
  check(actual.panel_faces===1 && actual.panel_edges===8 && actual.concave_faces===1 && actual.concave_edges===6 && actual.root_instances===4,'Native topology oracle differs');
  close(actual.panel_area_m2,11,'Native panel area');close(actual.concave_area_m2,5,'Native concave area');
  const inputs=[path.join(simpleDirectory,'legacy.skp'),path.join(simpleDirectory,'modern.skp'),path.join(complexDirectory,'legacy_2013.skp'),path.join(complexDirectory,'modern.skp')];
  const originals=inputs.map(p=>safeFile(p,128*MiB));
  check(originals[0].length===simple.legacy.bytes && originals[1].length===simple.modern.bytes && originals[2].length===actual.legacy_2013.bytes && originals[3].length===actual.modern.bytes,'Native source byte sizes differ');
  const fixed=originals.map((bytes,i)=>({slot:i+1,name:['simple-legacy','simple-modern','complex-legacy','complex-modern'][i],bytes,expect:i<2?'simple-limited':'complex-full'}));
  const all=[...fixed,...negativeCases(originals)];
  externalNew(destination);
  const manifest={schema_version:schema,status:'prepared_not_executed',source_only:true,
    source_hashes:originals.map(sha256),authoring_receipts:{simple:sha256(simpleAuditBytes),complex:sha256(complexAuditBytes),expected:sha256(expectedBytes)},
    collector_sha256:sha256(fs.readFileSync(fileURLToPath(import.meta.url))),
    simple_oracle:{faces:1,edges:4,area_m2:2,limitation:'No independent origin, plane, coordinate or hierarchy oracle was retained. These two cases cannot establish those properties.'},
    complex_oracle:expected,
    limits:{attempts:20,existing_ceiling:24,spare_not_retry_allowance:4,cancellation_or_crash:2,complex_repeats:2,total_wall_seconds:3600,attempt_wall_seconds:120,job_committed_bytes:1024*MiB,scratch_bytes:256*MiB},
    cases:[]};
  try {
    for(const fixture of all){
      const filename=String(fixture.slot).padStart(2,'0')+'-'+fixture.name+'.skp', file=path.join(destination,filename);
      if(fixture.zero_bytes){const fd=fs.openSync(file,'wx');try{fs.ftruncateSync(fd,fixture.zero_bytes);}finally{fs.closeSync(fd);}}
      else writeNew(file,fixture.bytes);
      manifest.cases.push({slot:fixture.slot,name:fixture.name,input:filename,bytes:fs.statSync(file).size,sha256:fileDigest(file),expected:fixture.expect});
    }
    inputs.forEach((file,i)=>check(fileDigest(file)===manifest.source_hashes[i],'Original source changed during fixture preparation'));
    jsonNew(path.join(destination,'matrix.json'),manifest);return manifest;
  } catch(error){jsonNew(path.join(destination,'preparation-failed.json'),{status:'failed',message:String(error)});throw error;}
}

function multiply(a,b) { const out=Array(16).fill(0);for(let c=0;c<4;c++)for(let r=0;r<4;r++)for(let k=0;k<4;k++)out[c*4+r]+=a[k*4+r]*b[c*4+k];return out; }
function transform(m,v) {return [0,1,2].map(r=>m[r]*v[0]+m[4+r]*v[1]+m[8+r]*v[2]+m[12+r]);}
const zUp=v=>[v[0],-v[2],v[1]];
function vecClose(a,b,label){check(Array.isArray(a)&&a.length===b.length,label+' shape');a.forEach((v,i)=>close(v,b[i],`${label}[${i}]`));}
function area(triangle){const[a,b,c]=triangle,u=b.map((v,i)=>v-a[i]),v=c.map((x,i)=>x-a[i]);return Math.hypot(u[1]*v[2]-u[2]*v[1],u[2]*v[0]-u[0]*v[2],u[0]*v[1]-u[1]*v[0])/2;}
export function clippedArea(triangle,rectangle){
  let polygon=triangle.map(v=>v.slice(0,2));
  for(const[axis,bound,sign]of [[0,rectangle[0],1],[0,rectangle[2],-1],[1,rectangle[1],1],[1,rectangle[3],-1]]){
    const out=[];
    for(let i=0;i<polygon.length;i++){const a=polygon[i],b=polygon[(i+1)%polygon.length],da=sign*(a[axis]-bound),db=sign*(b[axis]-bound);if(da>=0)out.push(a);if((da>=0)!==(db>=0)){const t=da/(da-db);out.push(a.map((v,j)=>v+t*(b[j]-v)));}}
    polygon=out;
  }
  return Math.abs(polygon.reduce((sum,p,i)=>{const q=polygon[(i+1)%polygon.length];return sum+p[0]*q[1]-q[0]*p[1];},0))/2;
}
export function triangleOverlap(a,b) {
  let polygon=a.map(v=>v.slice(0,2));
  for(let i=0;i<3;i++){
    const p=b[i],q=b[(i+1)%3],side=v=>(q[0]-p[0])*(v[1]-p[1])-(q[1]-p[1])*(v[0]-p[0]),out=[];
    for(let j=0;j<polygon.length;j++){const x=polygon[j],y=polygon[(j+1)%polygon.length],dx=side(x),dy=side(y);if(dx>=0)out.push(x);if((dx>=0)!==(dy>=0)){const t=dx/(dx-dy);out.push(x.map((v,k)=>v+t*(y[k]-v)));}}
    polygon=out;
  }
  return Math.abs(polygon.reduce((sum,p,i)=>{const q=polygon[(i+1)%polygon.length];return sum+p[0]*q[1]-q[0]*p[1];},0))/2;
}
export function inspectGlb(bytes) {
  check(bytes.length>=28 && bytes.length<=64*MiB && bytes.readUInt32LE(0)===0x46546c67 && bytes.readUInt32LE(4)===2 && bytes.readUInt32LE(8)===bytes.length,'Invalid/oversized GLB');
  let offset=12,json,bin;
  while(offset<bytes.length){check(offset+8<=bytes.length,'GLB chunk header truncated');const n=bytes.readUInt32LE(offset),type=bytes.readUInt32LE(offset+4);offset+=8;check(n%4===0 && offset+n<=bytes.length,'GLB chunk span');
    if(type===0x4e4f534a){check(!json&&!bin&&n<=4*MiB,'GLB JSON order/size');json=JSON.parse(new TextDecoder('utf-8',{fatal:true}).decode(bytes.subarray(offset,offset+n)).trim());}
    else if(type===0x004e4942){check(json&&!bin,'GLB BIN order');bin=bytes.subarray(offset,offset+n);}else throw new Error('Unexpected GLB chunk');offset+=n;}
  check(json?.asset?.version==='2.0'&&bin&&json.buffers?.length===1&&!json.buffers[0].uri&&json.buffers[0].byteLength<=bin.length,'Self-contained GLB required');
  check(!(json.extensionsRequired?.length)&&!(json.extensionsUsed?.length)&&!(json.images?.length)&&!(json.textures?.length)&&!(json.animations?.length)&&!(json.skins?.length),'Unsupported GLB content');
  check(json.scenes?.length===1&&json.scenes[0].nodes?.length===1&&(json.scene??0)===0,'One fixed scene required');
  for(const key of ['nodes','meshes','accessors','bufferViews'])check(Array.isArray(json[key])&&json[key].length<=50000,'Bounded GLB arrays required');
  const accessor=(index,components,types)=>{
    check(Number.isInteger(index)&&index>=0&&index<json.accessors.length,'Accessor identity');const a=json.accessors[index],view=json.bufferViews[a.bufferView];
    check(a&&!a.sparse&&!a.normalized&&a.type===(components===3?'VEC3':'SCALAR')&&types.includes(a.componentType)&&view&&view.buffer===0&&Number.isInteger(a.count)&&a.count>0&&a.count<=3000000,'Accessor contract');
    const size=a.componentType===5123?2:4,stride=view.byteStride??components*size,start=(view.byteOffset??0)+(a.byteOffset??0),length=(a.count-1)*stride+components*size;
    check(Number.isSafeInteger(start)&&start>=0&&stride>=components*size&&stride<=252&&Number.isSafeInteger(view.byteLength)&&view.byteLength>0&&(a.byteOffset??0)+length<=view.byteLength&&start+length<=json.buffers[0].byteLength,'Accessor range');
    const read=a.componentType===5126?'readFloatLE':a.componentType===5123?'readUInt16LE':'readUInt32LE';
    return Array.from({length:a.count},(_,i)=>Array.from({length:components},(_,j)=>{const value=bin[read](start+i*stride+j*size);check(Number.isFinite(value),'Nonfinite accessor');return value;}));
  };
  const seen=new Set(),placements=[];let triangles=0;
  const walk=(index,parent,occurrencePath,depth)=>{
    check(depth<=64&&Number.isInteger(index)&&index>=0&&index<json.nodes.length&&!seen.has(index),'Node cycle, repeat or invalid identity');seen.add(index);const node=json.nodes[index];
    check(!node.translation&&!node.rotation&&!node.scale&&!node.skin,'Expected explicit matrix contract');const local=node.matrix??identity;
    check(local.length===16&&local.every(Number.isFinite)&&local[3]===0&&local[7]===0&&local[11]===0&&local[15]===1,'Invalid affine matrix');const world=multiply(parent,local),faceTriangles=[],normals=[];
    if(node.mesh!==undefined){check(Number.isInteger(node.mesh)&&node.mesh>=0&&node.mesh<json.meshes.length,'Invalid mesh');const mesh=json.meshes[node.mesh];check(Array.isArray(mesh.primitives)&&mesh.primitives.length>0&&mesh.primitives.length<=1000,'Primitive bound');
      for(const primitive of mesh.primitives){check((primitive.mode??4)===4,'Triangles required');const vertices=accessor(primitive.attributes?.POSITION,3,[5126]),ns=accessor(primitive.attributes?.NORMAL,3,[5126]),indices=accessor(primitive.indices,1,[5123,5125]).flat();check(ns.length===vertices.length&&indices.length%3===0,'Triangle/normal shape');
        for(const n of ns){close(Math.hypot(...n),1,'Normal length');normals.push(zUp(n));}
        triangles+=indices.length/3;check(triangles<=1000000,'Expanded triangle bound');
        for(let i=0;i<indices.length;i+=3){const t=indices.slice(i,i+3).map(id=>{check(id<vertices.length,'Index exceeds vertices');return vertices[id];});check(area(t)>1e-12,'Degenerate triangle');faceTriangles.push({local:t.map(zUp),world:t.map(v=>zUp(transform(world,v)))});}
      }
    }
    placements.push({index,path:occurrencePath,local,world,triangles:faceTriangles,normals});
    check(!node.children||Array.isArray(node.children),'Children shape');for(const [i,child]of(node.children??[]).entries())walk(child,world,occurrencePath+'/'+i,depth+1);
  };
  walk(json.scenes[0].nodes[0],identity,'0',0);check(seen.size===json.nodes.length,'Unreachable GLB node');check(triangles>0,'Empty geometry');
  return {placements,triangles};
}

const expectedPlacements=[
  {path:'0',translation:[0,0,0],scale:[1,1,1]},
  {path:'0/0',translation:[0,0,0],scale:[1,1,1]},
  {path:'0/0/0',translation:[0,0,2],scale:[1,1,1],shape:'panel'},
  {path:'0/0/1',translation:[5,0,0],scale:[1,1,1],shape:'concave'},
  {path:'0/1',translation:[12,0,0],scale:[-1,2,0.5]},
  {path:'0/1/0',translation:[12,0,1],scale:[-1,2,0.5],shape:'panel'},
  {path:'0/1/1',translation:[7,0,0],scale:[-1,2,0.5],shape:'concave'},
  {path:'0/2',translation:[0,10,0],scale:[1,1,1],shape:'panel',hidden:true},
  {path:'0/3',translation:[10,10,0],scale:[1,1,1],shape:'concave',tagHidden:true},
];
export function verifyGeometry(bytes,metadata,kind) {
  const measured=inspectGlb(bytes), actual=metadata.scene;
  check(actual&&Array.isArray(actual.occurrences)&&actual.occurrences.length===measured.placements.length,'Metadata occurrence count');
  check(actual.node_count===measured.placements.length&&actual.expanded_triangles===measured.triangles,'Metadata geometry counts');
  let total=0,meshPlacements=0;const lo=[Infinity,Infinity,Infinity],hi=[-Infinity,-Infinity,-Infinity];
  for(let i=0;i<measured.placements.length;i++){
    const p=measured.placements[i],m=actual.occurrences[i];
    check(m.glb_node_index===p.index&&m.occurrence_path===p.path&&typeof m.source_hidden==='boolean'&&typeof m.tag_hidden==='boolean','Occurrence identity/visibility metadata');
    vecClose(m.local_matrix_gltf,p.local,'Metadata local matrix');vecClose(m.world_matrix_gltf,p.world,'Metadata world matrix');
    if(p.triangles.length)meshPlacements++;
    for(const t of p.triangles){total+=area(t.world);for(const vertex of t.world)for(let axis=0;axis<3;axis++){lo[axis]=Math.min(lo[axis],vertex[axis]);hi[axis]=Math.max(hi[axis],vertex[axis]);}}
  }
  // Worker GLB bounds are Y-up. Oracle values below are source Z-up metres.
  vecClose(actual.bounds_gltf_m?.min,[lo[0],lo[2],-hi[1]],'Metadata minimum');vecClose(actual.bounds_gltf_m?.max,[hi[0],hi[2],-lo[1]],'Metadata maximum');
  if(kind==='simple-limited'){
    const faces=[...(metadata.source?.root_face_inventory??[]),...(metadata.source?.definition_inventory??[]).flatMap(d=>d.faces??[])];
    check(faces.length===1,'Simple one source face');equal(faces[0].loop_edge_counts,[4],'Simple four-edge loop');close(total,2,'Simple total area');
    return {status:'limited_oracle_pass',area_m2:total,triangles:measured.triangles,unassessed:['absolute coordinates','origin','plane','hierarchy','orientation'],rust_admission:'required_separately'};
  }
  check(kind==='complex-full','Unknown oracle');check(measured.placements.length===9&&meshPlacements===6&&metadata.source?.placed_instance_count===8,'Complex occurrence multiplicity');
  let expectedTriangleArea=0;
  const unmatched=new Set(measured.placements.map((_,i)=>i)), matchedPaths=new Map();
  for(const e of expectedPlacements){
    // Official authoring specifies hierarchy, placement and multiplicity, but
    // not save-file sibling ordering. Match unique basis/origin at the declared
    // depth, then require its parent is the previously matched oracle parent.
    // Names are never join keys. The actual GLB index/path remains exact above.
    const matches=[...unmatched].filter(i=>{const p=measured.placements[i],origin=zUp(transform(p.world,[0,0,0]));return p.path.split('/').length===e.path.split('/').length&&origin.every((v,j)=>Math.abs(v-e.translation[j])<=Math.max(1e-6,1e-6*Math.abs(e.translation[j])));});
    check(matches.length===1,'Missing or ambiguous independently placed occurrence');const i=matches[0];unmatched.delete(i);
    const p=measured.placements[i],m=actual.occurrences[i];
    if(e.path!=='0')check(p.path.slice(0,p.path.lastIndexOf('/'))===matchedPaths.get(e.path.slice(0,e.path.lastIndexOf('/'))),'Independent parent hierarchy differs');matchedPaths.set(e.path,p.path);
    // Compare transformed basis and origin independently from worker matrices.
    const origin=zUp(transform(p.world,[0,0,0]));vecClose(origin,e.translation,'Occurrence origin');
    for(let axis=0;axis<3;axis++){const v=[0,0,0];v[axis]=1;const gltf=[v[0],v[2],-v[1]],point=zUp(transform(p.world,gltf));vecClose(point,e.translation.map((x,j)=>x+(j===axis?e.scale[axis]:0)),'Occurrence basis');}
    check(m.source_hidden===Boolean(e.hidden)&&m.tag_hidden===Boolean(e.tagHidden),'Hidden occurrence/tag state differs');
    if(!e.shape){check(p.triangles.length===0,'Unexpected root/container geometry');continue;}
    check(p.triangles.length>0,'Missing placed surface');const expectedArea=e.shape==='panel'?11:5,outer=e.shape==='panel'?[0,0,4,3]:[0,0,3,3],hole=e.shape==='panel'?[1,1,2,2]:[1,1,3,3];let localArea=0;
    for(const n of p.normals)vecClose(n,[0,0,1],'Independently authored +Z normal');
    for(const t of p.triangles){const a=area(t.local);localArea+=a;for(const v of t.local){close(v[2],0,'Local planar surface');check(v[0]>=outer[0]-1e-6&&v[0]<=outer[2]+1e-6&&v[1]>=outer[1]-1e-6&&v[1]<=outer[3]+1e-6,'Surface outside declared outline');}
      const [a0,b,c]=t.local;const signed=((b[0]-a0[0])*(c[1]-a0[1])-(b[1]-a0[1])*(c[0]-a0[0]))/2;check(signed>0,'Authored +Z triangle winding');close(clippedArea(t.local,hole),0,'Hole/concave missing quadrant remains empty');}
    check(p.triangles.length<=128,'Original fixture triangulation exceeded declared oracle work bound');
    for(let a=0;a<p.triangles.length;a++)for(let b=a+1;b<p.triangles.length;b++)close(triangleOverlap(p.triangles[a].local,p.triangles[b].local),0,'Duplicate/overlapping source triangles');
    close(localArea,expectedArea,'Per-occurrence local area');expectedTriangleArea+=expectedArea*Math.abs(e.scale[0]*e.scale[1]);
  }
  close(total,64,'Expanded total area');close(expectedTriangleArea,64,'Independent expanded oracle');vecClose(lo,[0,0,0],'Source minimum');vecClose(hi,[13,13,2],'Source maximum');
  const faces=(metadata.source.definition_inventory??[]).flatMap(d=>d.faces??[]);
  check(faces.length===2&&(metadata.source.root_face_inventory??[]).length===0,'Two independently defined source faces');
  equal(faces.map(f=>f.loop_edge_counts).sort((a,b)=>a.length-b.length),[[6],[4,4]],'Native face loops');
  return {status:'complex_oracle_pass',expanded_faces:6,placed_instances:8,area_m2:total,triangles:measured.triangles,bounds_m:{min:lo,max:hi},rust_admission:'required_separately',qualification:'Original synthetic surface subset only; no private-source or general reader claim'};
}

export function inspectCandidate(matrixFile,slot,stage,exitCode) {
  const manifestBytes=safeFile(matrixFile,4*MiB),manifest=JSON.parse(manifestBytes);
  check(manifest.schema_version===schema&&manifest.status==='prepared_not_executed'&&manifest.cases.length===20,'Fixed matrix manifest required');
  const row=manifest.cases.find(c=>c.slot===slot);check(row&&/^[0-9]{2}-[a-z0-9-]+\.skp$/.test(row.input),'Fixed case missing');
  const source=path.join(path.dirname(matrixFile),row.input);check(fs.statSync(source).size===row.bytes&&fileDigest(source)===row.sha256,'Prepared input changed');
  check(Number.isInteger(exitCode),'Observed worker exit required');const completionPath=path.join(stage,'completion.json');
  if(row.expected==='reject'){
    check(exitCode!==0&&!fs.existsSync(completionPath)&&!fs.existsSync(path.join(stage,'candidate.glb')),'Negative fixture unexpectedly completed or emitted candidate');
    return {schema_version:'greenlens.openskp-oracle-observation.v1',slot,status:'rejection_observed',source_sha256:row.sha256,matrix_sha256:sha256(manifestBytes),exit_code:exitCode,resource_and_diagnostic_reason:'Parent must verify exact rejection stage and resource receipt separately'};
  }
  check(exitCode===0,'Required original fixture rejected; stop candidate');
  const completionBytes=safeFile(completionPath,4*MiB),completion=JSON.parse(completionBytes),metadataBytes=safeFile(path.join(stage,'metadata.json'),4*MiB),glb=safeFile(path.join(stage,'candidate.glb'),64*MiB);
  check(completion.schema_version==='greenlens.openskp-development-completion.v1'&&completion.status==='unaccepted_candidate'&&completion.source_sha256===row.sha256,'Completion source/identity');
  check(completion.glb_sha256===sha256(glb)&&completion.metadata_sha256===sha256(metadataBytes)&&completion.glb_bytes===glb.length&&completion.metadata_bytes===metadataBytes.length,'Candidate byte identity');
  const metadata=JSON.parse(metadataBytes);check(metadata.schema_version==='greenlens.openskp-development-candidate.v1'&&metadata.status==='unaccepted_candidate'&&metadata.source_sha256===row.sha256&&metadata.candidate_glb_sha256===sha256(glb),'Metadata identity');
  const observation=verifyGeometry(glb,metadata,row.expected);
  check(fileDigest(source)===row.sha256,'Prepared source changed during inspection');
  return {schema_version:'greenlens.openskp-oracle-observation.v1',slot,...observation,source_sha256:row.sha256,matrix_sha256:sha256(manifestBytes),glb_sha256:sha256(glb),metadata_sha256:sha256(metadataBytes),completion_sha256:sha256(completionBytes),execution_and_resource_acceptance:'Required separately; this collector does not start or supervise workers'};
}

if(process.argv[1]&&path.resolve(process.argv[1])===fileURLToPath(import.meta.url)){
  const [command,...args]=process.argv.slice(2);
  try {
    let result;
    if(command==='prepare'&&args.length===3)result=prepareMatrix(...args);
    else if(command==='inspect'&&args.length===4)result=inspectCandidate(args[0],Number(args[1]),args[2],Number(args[3]));
    else throw new Error('Usage: matrix-oracle.mjs prepare SIMPLE_DIR COMPLEX_DIR NEW_EXTERNAL_DIR | inspect MATRIX_JSON SLOT STAGE OBSERVED_EXIT');
    process.stdout.write(JSON.stringify(result,null,2)+'\n');
  }catch(error){process.stderr.write(String(error)+'\n');process.exitCode=1;}
}
