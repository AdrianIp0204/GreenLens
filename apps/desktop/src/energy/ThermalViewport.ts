import * as THREE from 'three';
import { OrbitControls } from 'three/addons/controls/OrbitControls.js';
import type { CameraState, ViewId } from '../viewport/types';
import { THERMAL_VIEWS, type ThermalPresentation, type ThermalScene, type ThermalSceneObject } from './thermal-presentation';

type Camera=THREE.PerspectiveCamera|THREE.OrthographicCamera;
type Renderer=Pick<THREE.WebGLRenderer,'domElement'|'setPixelRatio'|'setScissorTest'|'setSize'|'setViewport'|'setScissor'|'render'|'setAnimationLoop'|'dispose'>;
export type ThermalRendererFactory=()=>Renderer;
type View={camera:Camera;controls:OrbitControls;element:HTMLDivElement};
export interface ThermalViewportProps {identity:string;scene:ThermalScene|null;display:ThermalPresentation;onDisplay:(change:(p:ThermalPresentation)=>ThermalPresentation)=>void;onSelect:(id:string|null)=>void;disabled:boolean}
const directions:Record<ViewId,number[]>={perspective:[1,-1,.8],top:[0,0,1],front:[0,-1,0],right:[1,0,0]};
const release=(object:THREE.Object3D)=>{object.traverse(child=>{const mesh=child as THREE.Mesh;mesh.geometry?.dispose();if(mesh.material)(Array.isArray(mesh.material)?mesh.material:[mesh.material]).forEach(m=>m.dispose());});object.removeFromParent();};
export function objectVisible(object:ThermalSceneObject,display:ThermalPresentation,objects:ThermalSceneObject[]):boolean {
  const zone=objects.find(o=>o.kind==='zone'&&o.zone_id===object.zone_id);
  if(display.hidden.includes(object.id)||zone&&display.hidden.includes(zone.id)||object.kind!=='zone'&&!display.layers[object.kind])return false;
  return display.isolated===null||display.isolated.includes(object.id)||Boolean(zone&&display.isolated.includes(zone.id));
}

/** Rendering only: every model polygon and semantic identity comes from Rust. */
export class ThermalViewport {
  private renderer:Renderer;private scene=new THREE.Scene();private group=new THREE.Group();private grid:THREE.GridHelper;
  private views=new Map<ViewId,View>();private objects=new Map<string,THREE.Group>();private observer:ResizeObserver;
  private props:ThermalViewportProps;private identity='';private geometry='';private restoring=false;private disposers:(()=>void)[]=[];
  constructor(private host:HTMLDivElement,elements:Record<ViewId,HTMLDivElement>,props:ThermalViewportProps,factory:ThermalRendererFactory=()=>new THREE.WebGLRenderer({antialias:true})){
    this.props=props;this.renderer=factory();this.renderer.setPixelRatio(Math.min(globalThis.devicePixelRatio||1,2));this.renderer.setScissorTest(true);
    this.renderer.domElement.setAttribute('aria-hidden','true');Object.assign(this.renderer.domElement.style,{position:'absolute',inset:'0',width:'100%',height:'100%',pointerEvents:'none'});host.prepend(this.renderer.domElement);
    this.scene.background=new THREE.Color('#edf2ef');this.scene.add(new THREE.AmbientLight(0xffffff,2));const light=new THREE.DirectionalLight(0xffffff,3);light.position.set(10,-10,20);this.scene.add(light,this.group);
    this.grid=new THREE.GridHelper(100,50,0xa8bbb0,0xd0dcd5);this.grid.rotation.x=Math.PI/2;this.scene.add(this.grid);
    for(const id of THERMAL_VIEWS){
      const camera:Camera=id==='perspective'?new THREE.PerspectiveCamera(40,1,.01,1e7):new THREE.OrthographicCamera(-8,8,8,-8,-1e6,1e6);
      camera.up.set(...(id==='top'?[0,1,0]:[0,0,1]) as [number,number,number]);camera.position.fromArray(directions[id]).multiplyScalar(25);
      const element=elements[id],controls=new OrbitControls(camera,element);controls.enableDamping=false;controls.enableRotate=id==='perspective';controls.screenSpacePanning=true;controls.minDistance=.01;controls.maxDistance=1e8;controls.minZoom=.0001;controls.maxZoom=10000;
      controls.mouseButtons.LEFT=null;controls.mouseButtons.MIDDLE=THREE.MOUSE.PAN;controls.mouseButtons.RIGHT=id==='perspective'?THREE.MOUSE.ROTATE:THREE.MOUSE.PAN;
      // OrbitControls swaps rotate/pan on modifiers. Resolve its action before its
      // pointer listener so orthographic and middle drags remain pan with Shift.
      const navigation=(e:PointerEvent)=>{const modified=e.shiftKey||e.ctrlKey||e.metaKey;controls.mouseButtons.MIDDLE=modified?THREE.MOUSE.ROTATE:THREE.MOUSE.PAN;controls.mouseButtons.RIGHT=id==='perspective'?THREE.MOUSE.ROTATE:modified?THREE.MOUSE.ROTATE:THREE.MOUSE.PAN;};
      element.addEventListener('pointerdown',navigation,true);
      const changed=()=>{if(!this.restoring)this.saveCameras();};controls.addEventListener('change',changed);
      this.views.set(id,{camera,controls,element});
      let down:{x:number;y:number}|null=null;
      const press=(e:PointerEvent)=>{if(this.props.disabled)return;element.focus();this.props.onDisplay(p=>({...p,active_view:id}));down=e.button===0?{x:e.clientX,y:e.clientY}:null;};
      const pick=(e:PointerEvent)=>{const start=down;down=null;if(this.props.disabled||!start||Math.hypot(e.clientX-start.x,e.clientY-start.y)>4)return;const r=element.getBoundingClientRect();if(!r.width||!r.height)return;this.scene.updateMatrixWorld(true);camera.updateMatrixWorld();const ray=new THREE.Raycaster();ray.setFromCamera(new THREE.Vector2(2*(e.clientX-r.left)/r.width-1,1-2*(e.clientY-r.top)/r.height),camera);const visible=[...this.objects.values()].filter(o=>o.visible).flatMap(o=>o.children.filter(c=>c instanceof THREE.Mesh));const hit=ray.intersectObjects(visible).find(h=>h.object.userData.objectId);this.props.onSelect(hit?.object.userData.objectId??null);};
      const cancel=()=>{down=null;};
      const keyboard=(e:KeyboardEvent)=>{if(e.target!==element||this.props.disabled||e.ctrlKey||e.metaKey||e.altKey)return;const step:Record<string,number[]>={ArrowLeft:[-1,0],ArrowRight:[1,0],ArrowUp:[0,1],ArrowDown:[0,-1]};if(step[e.key]){e.preventDefault();e.stopPropagation();const [x,y]=step[e.key],scale=camera instanceof THREE.OrthographicCamera?1/camera.zoom:camera.position.distanceTo(controls.target)/12;const move=new THREE.Vector3().setFromMatrixColumn(camera.matrix,0).multiplyScalar(x*scale).add(new THREE.Vector3().setFromMatrixColumn(camera.matrix,1).multiplyScalar(y*scale));camera.position.add(move);controls.target.add(move);controls.update();this.saveCameras();}else if(['+','=','-','_'].includes(e.key)){e.preventDefault();e.stopPropagation();const factor=['-','_'].includes(e.key)?1.15:1/1.15;if(camera instanceof THREE.OrthographicCamera)camera.zoom=THREE.MathUtils.clamp(camera.zoom/factor,.0001,10000);else camera.position.sub(controls.target).multiplyScalar(factor).add(controls.target);camera.updateProjectionMatrix();controls.update();this.saveCameras();}else if(e.key.toLowerCase()==='f'){e.preventDefault();e.stopPropagation();this.fit(id,Boolean(this.props.display.selected));}else if(e.key==='Escape'){e.stopPropagation();this.props.onSelect(null);}};
      element.addEventListener('pointerdown',press);element.addEventListener('pointerup',pick);element.addEventListener('pointercancel',cancel);element.addEventListener('lostpointercapture',cancel);element.addEventListener('keydown',keyboard);
      this.disposers.push(()=>{controls.removeEventListener('change',changed);controls.dispose();element.removeEventListener('pointerdown',navigation,true);element.removeEventListener('pointerdown',press);element.removeEventListener('pointerup',pick);element.removeEventListener('pointercancel',cancel);element.removeEventListener('lostpointercapture',cancel);element.removeEventListener('keydown',keyboard);});
    }
    this.observer=new ResizeObserver(()=>this.resize());this.observer.observe(host);this.update(props);this.resize();this.renderer.setAnimationLoop(()=>this.render());
  }
  cameraState(id:ViewId):CameraState {const v=this.views.get(id)!;return{position:v.camera.position.toArray() as [number,number,number],target:v.controls.target.toArray() as [number,number,number],up:v.camera.up.toArray() as [number,number,number],zoom:v.camera.zoom};}
  private saveCameras(){const cameras=Object.fromEntries(THERMAL_VIEWS.map(id=>[id,this.cameraState(id)]));this.props.onDisplay(p=>JSON.stringify(p.cameras)===JSON.stringify(cameras)?p:{...p,cameras});}
  update(props:ThermalViewportProps){
    this.props=props;const changedIdentity=this.identity!==props.identity;this.identity=props.identity;
    this.restoring=true;for(const [id,v] of this.views){v.controls.enabled=!props.disabled;const c=props.display.cameras[id];if(c&&JSON.stringify(c)!==JSON.stringify(this.cameraState(id))){v.camera.position.fromArray(c.position);v.camera.up.fromArray(c.up);v.controls.target.fromArray(c.target);v.camera.zoom=c.zoom;v.camera.updateProjectionMatrix();v.controls.update();}}
    const geometry=props.scene?JSON.stringify([props.scene.model_sha256,props.scene.objects]):'';
    if(geometry!==this.geometry){this.geometry=geometry;for(const child of [...this.group.children])release(child);this.objects.clear();for(const object of props.scene?.objects??[]){if(!object.polygons_m.length)continue;const group=new THREE.Group();for(const polygon of object.polygons_m){const vertices:number[]=[];for(let i=1;i+1<polygon.length;i++)vertices.push(...polygon[0],...polygon[i],...polygon[i+1]);const g=new THREE.BufferGeometry();g.setAttribute('position',new THREE.Float32BufferAttribute(vertices,3));g.computeVertexNormals();const mesh=new THREE.Mesh(g,new THREE.MeshStandardMaterial({color:object.kind==='window'?0x78aeb1:object.kind==='surface'?0xc1d4c6:0x9daba4,side:THREE.DoubleSide,transparent:true,opacity:object.kind==='surface'?.35:.85,depthWrite:false}));mesh.userData.objectId=object.id;const edge=new THREE.LineSegments(new THREE.EdgesGeometry(g),new THREE.LineBasicMaterial({color:0x859f91}));group.add(mesh,edge);}this.objects.set(object.id,group);this.group.add(group);}}
    for(const object of props.scene?.objects??[]){const node=this.objects.get(object.id);if(!node)continue;node.visible=objectVisible(object,props.display,props.scene!.objects);const zone=props.scene!.objects.find(o=>o.kind==='zone'&&o.zone_id===object.zone_id);const selected=props.display.selected===object.id||zone?.id===props.display.selected;node.traverse(child=>{if(child instanceof THREE.LineSegments)(child.material as THREE.LineBasicMaterial).color.setHex(selected?0x146c4b:0x859f91);});}
    this.grid.visible=props.display.grid;
    if(props.scene?.objects.length)for(const id of THERMAL_VIEWS)if(!props.display.cameras[id])this.fit(id,false,false);
    this.restoring=false;if(props.scene?.objects.length&&(changedIdentity||Object.keys(props.display.cameras).length<4))this.saveCameras();this.resize();
  }
  fit(id:ViewId,selected=false,persist=true){
    const scene=this.props.scene;if(!scene)return;const selectedObject=scene.objects.find(o=>o.id===this.props.display.selected);const points=scene.objects.filter(o=>objectVisible(o,this.props.display,scene.objects)&&(!selected||o.id===selectedObject?.id||selectedObject?.kind==='zone'&&o.zone_id===selectedObject.zone_id)).flatMap(o=>o.polygons_m.flat());if(!points.length)return;
    const bounds=new THREE.Box3().setFromPoints(points.map(p=>new THREE.Vector3(...p))),center=bounds.getCenter(new THREE.Vector3()),radius=Math.max(1,bounds.getSize(new THREE.Vector3()).length()/2),v=this.views.get(id)!;
    const wasRestoring=this.restoring;this.restoring=true;v.controls.target.copy(center);v.camera.position.copy(center).add(new THREE.Vector3().fromArray(directions[id]).normalize().multiplyScalar(radius*3.4));v.camera.up.set(...(id==='top'?[0,1,0]:[0,0,1]) as [number,number,number]);if(v.camera instanceof THREE.OrthographicCamera)v.camera.zoom=8/(radius*1.2);v.camera.lookAt(center);v.camera.updateProjectionMatrix();v.controls.update();this.restoring=wasRestoring;if(persist)this.saveCameras();
  }
  private resize(){const rect=this.host.getBoundingClientRect();if(!rect.width||!rect.height)return;this.renderer.setSize(rect.width,rect.height,false);for(const {camera,element} of this.views.values()){const r=element.getBoundingClientRect();if(!r.width||!r.height)continue;const aspect=r.width/r.height;if(camera instanceof THREE.PerspectiveCamera)camera.aspect=aspect;else{camera.left=-8*aspect;camera.right=8*aspect;camera.top=8;camera.bottom=-8;}camera.updateProjectionMatrix();}}
  private render(){const bounds=this.host.getBoundingClientRect();for(const [id,v] of this.views){if(this.props.display.maximized_view&&this.props.display.maximized_view!==id)continue;const r=v.element.getBoundingClientRect();if(!r.width||!r.height)continue;this.renderer.setViewport(r.left-bounds.left,bounds.bottom-r.bottom,r.width,r.height);this.renderer.setScissor(r.left-bounds.left,bounds.bottom-r.bottom,r.width,r.height);this.renderer.render(this.scene,v.camera);}}
  dispose(){this.renderer.setAnimationLoop(null);this.observer.disconnect();this.disposers.forEach(f=>f());release(this.scene);this.renderer.dispose();this.renderer.domElement.remove();}
}
