import { useEffect, useRef, useState } from 'react';
import * as THREE from 'three';
import { OrbitControls } from 'three/addons/controls/OrbitControls.js';
import type { ValidatedThermalModel } from './types';

/** Geometry is Rust-validated. This component only draws the returned vertices. */
export default function ThermalView({validated,selected,onSelect}:{validated:ValidatedThermalModel|null;selected:string;onSelect:(id:string)=>void}){
  const host=useRef<HTMLDivElement>(null), selection=useRef(onSelect);selection.current=onSelect;
  const sceneState=useRef<{scene:THREE.Scene;group:THREE.Group;camera:THREE.PerspectiveCamera;controls:OrbitControls}|undefined>(undefined);
  const fitted=useRef(false);
  const fit=()=>{const st=sceneState.current;if(!st||!st.group.children.length)return;const box=new THREE.Box3().setFromObject(st.group),center=box.getCenter(new THREE.Vector3()),size=box.getSize(new THREE.Vector3());const distance=Math.max(2,size.length()*1.25);st.controls.target.copy(center);st.camera.position.copy(center).add(new THREE.Vector3(1,-1,.8).normalize().multiplyScalar(distance));st.camera.far=Math.max(2000,distance*20);st.camera.updateProjectionMatrix();st.controls.update();};
  const [error,setError]=useState('');
  useEffect(()=>{const node=host.current;if(!node)return;let renderer:THREE.WebGLRenderer;
    try{renderer=new THREE.WebGLRenderer({antialias:true});}catch(e){setError(`WebGL2 unavailable: ${String(e)}`);return;}
    renderer.setPixelRatio(Math.min(devicePixelRatio,2));node.appendChild(renderer.domElement);
    const scene=new THREE.Scene();scene.background=new THREE.Color('#edf2ef');scene.add(new THREE.AmbientLight('#ffffff',2));const light=new THREE.DirectionalLight('#ffffff',3);light.position.set(10,-10,20);scene.add(light);
    const group=new THREE.Group();scene.add(group);const camera=new THREE.PerspectiveCamera(40,1,.1,2000);camera.up.set(0,0,1);camera.position.set(27,-27,22);
    const controls=new OrbitControls(camera,renderer.domElement);controls.target.set(8,3,3);controls.enableDamping=true;controls.dampingFactor=.12;
    const grid=new THREE.GridHelper(60,30,0xa8bbb0,0xd0dcd5);grid.rotation.x=Math.PI/2;scene.add(grid);scene.add(new THREE.AxesHelper(3));
    sceneState.current={scene,group,camera,controls};const resize=new ResizeObserver(()=>{const w=node.clientWidth,h=node.clientHeight;if(w&&h){camera.aspect=w/h;camera.updateProjectionMatrix();renderer.setSize(w,h);}});resize.observe(node);
    const ray=new THREE.Raycaster();let down:[number,number]|null=null;const press=(e:PointerEvent)=>{down=[e.clientX,e.clientY];};const pick=(e:PointerEvent)=>{if(!down||Math.hypot(e.clientX-down[0],e.clientY-down[1])>4)return;const b=node.getBoundingClientRect();ray.setFromCamera(new THREE.Vector2(2*(e.clientX-b.left)/b.width-1,1-2*(e.clientY-b.top)/b.height),camera);const hit=ray.intersectObjects(group.children).find(h=>h.object.userData.zone);if(hit)selection.current(hit.object.userData.zone);};renderer.domElement.addEventListener('pointerdown',press);renderer.domElement.addEventListener('pointerup',pick);
    renderer.setAnimationLoop(()=>{controls.update();renderer.render(scene,camera);});return()=>{resize.disconnect();renderer.setAnimationLoop(null);controls.dispose();scene.traverse(o=>{const mesh=o as THREE.Mesh;mesh.geometry?.dispose();if(mesh.material)(Array.isArray(mesh.material)?mesh.material:[mesh.material]).forEach(m=>m.dispose());});renderer.dispose();renderer.domElement.remove();sceneState.current=undefined;};
  },[]);
  useEffect(()=>{const state=sceneState.current;if(!state)return;for(const child of [...state.group.children]){state.group.remove(child);const mesh=child as THREE.Mesh;mesh.geometry?.dispose();if(mesh.material)(Array.isArray(mesh.material)?mesh.material:[mesh.material]).forEach(m=>m.dispose());}if(!validated)return;
    const polygon=(points:number[][],color:number,zone:string,opacity:number)=>{const flat=[...points[0],...points[1],...points[2],...points[0],...points[2],...points[3]];const geometry=new THREE.BufferGeometry();geometry.setAttribute('position',new THREE.Float32BufferAttribute(flat,3));geometry.computeVertexNormals();const mesh=new THREE.Mesh(geometry,new THREE.MeshStandardMaterial({color,transparent:true,opacity,side:THREE.DoubleSide,depthWrite:false}));mesh.userData.zone=zone;state.group.add(mesh);const edges=new THREE.LineSegments(new THREE.EdgesGeometry(geometry),new THREE.LineBasicMaterial({color:zone===selected?0x277358:0x859f91}));state.group.add(edges);};
    validated.surfaces.forEach(s=>polygon(s.vertices_m,s.zone_id===selected?0x77b99e:0xc1d4c6,s.zone_id,s.face==='roof'?.12:.42));
    const model=validated.model,angle=-model.north_rotation_deg*Math.PI/180,c=Math.cos(angle),s=Math.sin(angle),rotate=(p:number[])=>[p[0]*c-p[1]*s,p[0]*s+p[1]*c,p[2]];
    for(const zone of model.zones)for(const window of zone.windows){const face=zone.surfaces.find(s=>s.id===window.surface_id)!.face;const axis=face==='west'||face==='east'?0:face==='south'||face==='north'?1:2;const uv=axis===0?[1,2]:axis===1?[0,2]:[0,1];const p=[...zone.origin_m];if(['east','north','roof'].includes(face))p[axis]+=zone.size_m[axis];p[uv[0]]+=window.offset_u_m;p[uv[1]]+=window.offset_v_m;const q=[p.slice(),p.slice(),p.slice(),p.slice()];q[1][uv[0]]+=window.width_m;q[2][uv[0]]+=window.width_m;q[2][uv[1]]+=window.height_m;q[3][uv[1]]+=window.height_m;polygon(q.map(rotate),0x459eb0,zone.id,.82);
      if(window.overhang){const o=window.overhang,base=p.slice();base[uv[0]]-=o.side_extension_m;base[2]+=window.height_m+o.gap_above_m;const shade=[base.slice(),base.slice(),base.slice(),base.slice()];shade[1][uv[0]]+=window.width_m+2*o.side_extension_m;shade[2][uv[0]]+=window.width_m+2*o.side_extension_m;const outward=['east','north'].includes(face)?1:-1;shade[2][axis]+=outward*o.projection_m;shade[3][axis]+=outward*o.projection_m;polygon(shade.map(rotate),0x9f9a7c,zone.id,.9);}}
    for(const shade of model.detached_shading){const points=Array.from({length:8},(_,i)=>rotate(shade.origin_m.map((v,a)=>v+((i>>a)&1)*shade.size_m[a])));for(const face of [[0,1,3,2],[4,6,7,5],[0,4,5,1],[2,3,7,6],[0,2,6,4],[1,5,7,3]])polygon(face.map(i=>points[i]),0x8a8f94,'',.65);}
    if(!fitted.current){fit();fitted.current=true;}
  },[validated,selected]);
  return <div className="thermal-view"><div ref={host} className="thermal-canvas" aria-label="Thermal model three dimensional view"/>{error&&<p role="alert">{error}</p>}<span className="thermal-view-caption">Validated geometry · drag to orbit · right drag to pan · click a zone</span><button className="thermal-extents" onClick={fit}>Fit model</button></div>;
}
