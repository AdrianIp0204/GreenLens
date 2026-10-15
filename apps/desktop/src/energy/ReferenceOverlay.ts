import * as THREE from 'three';
import type { ReferenceOccurrence, ReferencePoint } from '../reference/types';
import { occurrenceVisibility, pointWithinSection, type ReferenceOverlayData, type ReferencePresentation, type ReferenceSection, type ReferenceVertexId } from './reference-presentation';

type ChunkMesh={mesh:THREE.Mesh<THREE.BufferGeometry,THREE.MeshStandardMaterial>;source:ReferenceOverlayData['chunks'][number];faces:number[]};
/** One mesh per bounded chunk; triangles are never exposed as editable CAD objects. */
export class ReferenceOverlay {
  readonly group=new THREE.Group();private chunks=new Map<number,ChunkMesh>();private identity='';private style='';private mapping:ReferenceOverlayData['mappingStates'];
  private markers=new THREE.Points(new THREE.BufferGeometry(),new THREE.PointsMaterial({color:0xef7c27,size:10,sizeAttenuation:false,depthTest:false}));private markerKey='';
  private data:ReferenceOverlayData|null=null;private occurrences=new Map<string,ReferenceOccurrence>();
  constructor(){this.group.name='Architectural reference (display only)';this.markers.name='Measurement vertices (display only)';this.markers.renderOrder=10;this.group.add(this.markers);}
  update(data:ReferenceOverlayData|null|undefined,display:ReferencePresentation,selected:string|null){
    const identity=data?`${data.assetId}:${data.manifest.revision_sha256}:${data.manifest.geometry_sha256}`:'';
    if(identity!==this.identity){this.clear();this.identity=identity;this.occurrences=new Map(data?.manifest.occurrences.map(o=>[o.id,o])??[]);}
    this.data=data??null;this.group.visible=Boolean(data&&display.visible);if(!data)return;
    const retained=new Set(data.chunks.map(c=>c.index));let added=false;
    for(const [index,entry] of this.chunks)if(!retained.has(index)){entry.mesh.geometry.dispose();entry.mesh.material.dispose();entry.mesh.removeFromParent();this.chunks.delete(index);}
    for(const source of data.chunks){
      if(this.chunks.get(source.index)?.source===source)continue;
      if(source.index>=data.manifest.chunks.length||source.positions.length!==source.occurrences.length*9||source.occurrences.length!==data.manifest.chunks[source.index].triangle_count||source.occurrences.some(i=>i>=data.manifest.occurrences.length))throw Error('Admitted reference display chunk does not match its manifest.');
      const old=this.chunks.get(source.index);if(old){old.mesh.geometry.dispose();old.mesh.material.dispose();old.mesh.removeFromParent();}
      const geometry=new THREE.BufferGeometry();geometry.setAttribute('position',new THREE.BufferAttribute(source.positions,3));geometry.setAttribute('color',new THREE.Float32BufferAttribute(new Float32Array(source.positions.length),3));geometry.computeVertexNormals();geometry.computeBoundingSphere();
      const mesh=new THREE.Mesh(geometry,new THREE.MeshStandardMaterial({vertexColors:true,side:THREE.DoubleSide,roughness:.95,transparent:true,depthWrite:false,polygonOffset:true,polygonOffsetFactor:1,polygonOffsetUnits:1}));
      mesh.position.fromArray(source.origin_m);mesh.name=`Reference chunk ${source.index}`;mesh.userData.referenceChunk=source.index;this.group.add(mesh);this.chunks.set(source.index,{source,mesh,faces:[]});added=true;
    }
    const style=JSON.stringify([display.visible,display.ghost,display.hidden,display.isolated,selected,data.mappingRelationship]);if(!added&&style===this.style&&this.mapping===data.mappingStates)return;this.style=style;this.mapping=data.mappingStates;
    const visible=occurrenceVisibility(this.occurrences,display),colors=new Map<number,THREE.Color>();
    data.manifest.occurrences.forEach((o,index)=>{const state=data.mappingRelationship==='current'?data.mappingStates?.[o.id]??'unresolved':'unresolved';colors.set(index,new THREE.Color(selected===o.id?0xd66d27:state==='mapped'?0x3c8d9b:state==='context_only'?0x8a819f:state==='excluded'?0x9d9d9d:0x859db0));});
    for(const entry of this.chunks.values()){
      const indices:number[]=[],faces:number[]=[],attribute=entry.mesh.geometry.getAttribute('color') as THREE.BufferAttribute;
      entry.source.occurrences.forEach((oi,t)=>{const color=colors.get(oi)!;for(let c=0;c<3;c++)attribute.setXYZ(t*3+c,color.r,color.g,color.b);if(visible(data.manifest.occurrences[oi].id)){indices.push(t*3,t*3+1,t*3+2);faces.push(t);}});
      entry.mesh.geometry.setIndex(indices);attribute.needsUpdate=true;entry.faces=faces;entry.mesh.material.opacity=display.ghost?.24:.82;entry.mesh.material.depthWrite=!display.ghost;entry.mesh.visible=faces.length>0;
    }
  }
  meshes():THREE.Object3D[]{return this.group.visible?[...this.chunks.values()].filter(e=>e.mesh.visible).map(e=>e.mesh):[];}
  updateMarkers(vertices:ReferenceVertexId[]){const key=JSON.stringify([this.identity,vertices]);if(key===this.markerKey)return;this.markerKey=key;const positions:number[]=[],origin=this.chunks.get(vertices[0]?.chunk)?.source.origin_m??[0,0,0];this.markers.position.fromArray(origin);for(const v of vertices.slice(0,2)){const source=this.chunks.get(v.chunk)?.source;if(!source||v.triangle>=source.occurrences.length||v.corner<0||v.corner>2)continue;for(let a=0;a<3;a++)positions.push(source.positions[v.triangle*9+v.corner*3+a]+source.origin_m[a]-origin[a]);}this.markers.geometry.dispose();this.markers.geometry=new THREE.BufferGeometry();this.markers.geometry.setAttribute('position',new THREE.Float32BufferAttribute(positions,3));this.markers.visible=positions.length>0;}
  pick(hit:THREE.Intersection,section?:ReferenceSection):{occurrenceId:string;vertex:ReferenceVertexId}|null {
    const entry=this.chunks.get(hit.object.userData.referenceChunk);if(!entry||hit.faceIndex===undefined||hit.faceIndex===null||!this.data)return null;
    const triangle=entry.faces[hit.faceIndex];if(triangle===undefined)return null;const occurrence=this.data.manifest.occurrences[entry.source.occurrences[triangle]];
    const local=hit.point.clone().sub(entry.mesh.position),position=entry.mesh.geometry.getAttribute('position');let corner=0,distance=Infinity;
    for(let c=0;c<3;c++){const point=new THREE.Vector3().fromBufferAttribute(position,triangle*3+c);if(section&&!pointWithinSection(point.clone().add(entry.mesh.position).toArray() as ReferencePoint,section))continue;const d=point.distanceToSquared(local);if(d<distance){corner=c;distance=d;}}if(!Number.isFinite(distance))return null;
    return{occurrenceId:occurrence.id,vertex:{chunk:entry.source.index,triangle,corner}};
  }
  fitPoints(display:ReferencePresentation,selected:string|null):ReferencePoint[]{
    if(!this.data)return[];const visible=occurrenceVisibility(this.occurrences,display);return this.data.manifest.occurrences.filter(o=>o.bounds&&visible(o.id)&&(!selected||o.id===selected||this.descendsFrom(o,selected))).flatMap(o=>[o.bounds!.min_m,o.bounds!.max_m]);
  }
  private descendsFrom(o:ReferenceOccurrence,id:string){let current=o,depth=0;while(current.parent_id&&depth++<=64){if(current.parent_id===id)return true;const parent=this.occurrences.get(current.parent_id);if(!parent)return false;current=parent;}return false;}
  clear(){for(const e of this.chunks.values()){e.mesh.geometry.dispose();e.mesh.material.dispose();e.mesh.removeFromParent();}this.chunks.clear();this.style='';this.markerKey='';this.markers.visible=false;this.data=null;}
  dispose(){this.clear();this.markers.geometry.dispose();this.markers.material.dispose();this.group.removeFromParent();}
}
