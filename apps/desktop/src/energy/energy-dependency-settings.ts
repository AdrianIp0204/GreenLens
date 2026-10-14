import {useCallback,useState} from 'react';

export type EnergyDependencyPaths={engineDir:string;typstDir:string};
type StoragePort=Pick<Storage,'getItem'|'setItem'>;
const key='greenlens.energy-dependency-paths.v1';
const defaults=():EnergyDependencyPaths=>({engineDir:'',typstDir:''});
const bytes=(text:string)=>new TextEncoder().encode(text).byteLength;
export function validDependencyPaths(value:unknown):value is EnergyDependencyPaths {
  if(!value||typeof value!=='object'||Array.isArray(value))return false;
  const p=value as EnergyDependencyPaths;
  return Object.keys(p).length===2&&['engineDir','typstDir'].every(k=>{
    const path=p[k as keyof EnergyDependencyPaths];
    return typeof path==='string'&&path.length<=4096&&!/[\u0000-\u001f\u007f]/.test(path);
  });
}
/** Machine-local configuration only. Never included in Project or report evidence. */
export class EnergyDependencyPathStore {
  paths=defaults();warning='';
  constructor(private storage:StoragePort|null){
    if(!storage){this.warning='Local settings are unavailable; installation paths remain only in this window.';return;}
    try{
      const raw=storage.getItem(key);if(raw===null)return;
      if(raw.length>32768||bytes(raw)>32768)throw Error();
      const data=JSON.parse(raw);
      if(data?.schema_version!=='greenlens.energy-dependency-paths.v1'||!validDependencyPaths(data.paths))throw Error();
      this.paths={...data.paths};
    }catch{this.warning='Saved installation settings could not be read. Default discovery is selected; check local installations before use.';}
  }
  set(paths:EnergyDependencyPaths){
    if(!validDependencyPaths(paths)){this.warning='Use directory paths of at most 4096 characters without control characters. The previous settings remain.';return false;}
    this.paths={...paths};
    try{
      if(!this.storage)throw Error();
      this.storage.setItem(key,JSON.stringify({schema_version:'greenlens.energy-dependency-paths.v1',paths}));
      this.warning='';
    }catch{this.warning='Installation paths are retained only in this window because local settings could not be saved.';}
    return true;
  }
}
export function useEnergyDependencyPaths(){
  const [store]=useState(()=>{try{return new EnergyDependencyPathStore(window.localStorage);}catch{return new EnergyDependencyPathStore(null);}});
  const [,refresh]=useState(0);
  const setPath=useCallback((field:keyof EnergyDependencyPaths,value:string)=>{store.set({...store.paths,[field]:value});refresh(n=>n+1);},[store]);
  const reset=useCallback(()=>{store.set(defaults());refresh(n=>n+1);},[store]);
  return {paths:store.paths,warning:store.warning,setPath,reset};
}
