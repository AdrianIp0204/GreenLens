import {useEffect,useMemo,useState} from 'react';
import {invoke} from '@tauri-apps/api/core';
import type {ClimateProjectDocument} from '../climate-types';
import type {ReferenceModel,ReferenceTriangle,ScopeAssessment,ScopeContext} from './types';
import {compactReferenceChunk,type ReferenceMappingRelationship,type ReferenceOverlayData} from '../energy/reference-presentation';

function checkedAssessment(value:ScopeAssessment):ScopeAssessment {
  const hash=(s:unknown)=>typeof s==='string'&&/^[a-f0-9]{64}$/.test(s);
  if(!value||value.schema_version!=='greenlens.architectural-scope-assessment.v1'||!hash(value.scope_sha256)||!hash(value.report_binding_sha256)||!['current','stale'].includes(value.relationship)||!Array.isArray(value.stale_reasons)||value.stale_reasons.some(r=>!r||typeof r.field!=='string'||typeof r.message!=='string')||value.relationship==='current'&&value.stale_reasons.length!==0)throw Error('Scope assessment response is invalid; mapping relationship remains unassessed.');
  return value;
}

/** Only admitted bounded chunks cross IPC; annual evidence is not reopened by a
 * reference panel. A replacement invalidates all late responses. */
export function useReferenceModel(path:string,project:ClimateProjectDocument|null){
  const asset=project?.architecture?.assets.find(a=>a.id===project.architecture?.active_asset_id);
  const key=JSON.stringify([path,project?.id,asset]);
  const [snapshot,setSnapshot]=useState<{key:string;overlay:ReferenceOverlayData|null;error:string}>({key:'',overlay:null,error:''});
  const [context,setContext]=useState<{key:string;value:ScopeContext|null;error:string}>({key:'',value:null,error:''});
  const [assessment,setAssessment]=useState<{key:string;value:ScopeAssessment|null;error:string}>({key:'',value:null,error:''});
  const modelsKey=JSON.stringify([key,project?.energy?.designs,project?.energy?.workbench?.calendars]);
  const scopeKey=JSON.stringify([modelsKey,project?.architecture?.scope]);
  useEffect(()=>{
    let alive=true;if(!asset||!path||!project){setSnapshot({key,overlay:null,error:''});return;}
    setSnapshot({key,overlay:null,error:''});
    void(async()=>{
      const manifest=await invoke<ReferenceModel>('reference_manifest',{path,project,assetId:asset.id});
      if(!alive)return;
      const chunks:ReferenceOverlayData['chunks']=[];
      const publish=(loading:boolean)=>setSnapshot({key,error:'',overlay:{assetId:asset.id,manifest,chunks:[...chunks],loading,status:loading?`Loading admitted geometry ${chunks.length}/${manifest.chunks.length}`:'Admitted geometry loaded; display-only overlay'}});
      publish(true);
      for(let index=0;index<manifest.chunks.length;index++){
        const triangles=await invoke<ReferenceTriangle[]>('reference_chunk',{path,project,assetId:asset.id,index});
        if(!alive)return;
        chunks.push(compactReferenceChunk(index,triangles,manifest.bounds.min_m));publish(index+1<manifest.chunks.length);
        await new Promise(resolve=>setTimeout(resolve,0));
      }
    })().catch(e=>{if(alive)setSnapshot({key,overlay:null,error:String(e)});});
    return()=>{alive=false;};
  },[key]);
  useEffect(()=>{
    let alive=true;if(!asset||!project||!path)return;
    invoke<ScopeContext>('reference_scope_context',{path,project}).then(value=>{if(alive)setContext({key:modelsKey,value,error:''});}).catch(e=>{if(alive)setContext({key:modelsKey,value:null,error:String(e)});});
    return()=>{alive=false;};
  },[modelsKey]);
  useEffect(()=>{
    let alive=true;const scope=project?.architecture?.scope;if(!scope||!asset||!path)return;
    setAssessment({key:scopeKey,value:null,error:''});
    invoke<ScopeAssessment>('reference_scope_assess',{path,project,scope}).then(checkedAssessment).then(value=>{if(alive)setAssessment({key:scopeKey,value,error:''});}).catch(e=>{if(alive)setAssessment({key:scopeKey,value:null,error:String(e)});});
    return()=>{alive=false;};
  },[scopeKey]);
  const overlay=snapshot.key===key?snapshot.overlay:null;
  const scope=project?.architecture?.scope;
  const applicableAssessment=scope&&asset&&path&&assessment.key===scopeKey?assessment.value:null;
  const assessmentError=assessment.key===scopeKey?assessment.error:'';
  const mappingRelationship:ReferenceMappingRelationship=!scope||!asset||!path||assessmentError?'unassessed':applicableAssessment?.relationship??'pending';
  const declaredMappingStates=useMemo(()=>Object.fromEntries((scope?.mappings??[]).map(m=>[m.occurrence_id,m.status])),[scope]);
  const mappingStates=mappingRelationship==='current'?declaredMappingStates:undefined;
  const mappingDisclosure=mappingRelationship==='current'?'Mappings are current for the accepted inputs assessed by Rust; this does not establish architectural equivalence or scientific qualification.':mappingRelationship==='stale'?`Mappings are historical: the accepted scope is stale for current scientific inputs. ${applicableAssessment?.stale_reasons.map(r=>r.message).join(' ')??''} Current mapping colors are withheld.`:mappingRelationship==='pending'?'Checking mapping relationship against current scientific inputs. Current mapping colors are withheld pending Rust assessment.':`Mapping relationship is unassessed. ${assessmentError||(!scope?'No scope is accepted.':'No applicable assessment is available.')} Current mapping colors are withheld.`;
  return {overlay:overlay?{...overlay,mappingStates,declaredMappingStates,mappingRelationship,mappingDisclosure}:null,context:context.key===modelsKey?context.value:null,assessment:applicableAssessment,error:[snapshot.key===key?snapshot.error:'',context.key===modelsKey?context.error:'',assessmentError].filter(Boolean).join('\n')};
}
