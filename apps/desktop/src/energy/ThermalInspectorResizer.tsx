import { useEffect, useRef } from 'react';
import type { useThermalPresentation } from './thermal-presentation';
import './thermal-view.css';

/** Right-side Properties width, local to this document/design; never a model edit. */
export default function ThermalInspectorResizer({presentation}:{presentation:ReturnType<typeof useThermalPresentation>}){
  const live=useRef(presentation);live.current=presentation;
  const stop=useRef<()=>void>(()=>{});
  useEffect(()=>()=>stop.current(),[]);
  useEffect(()=>{stop.current();},[presentation.key]);
  const width=(value:number)=>live.current.update(p=>({...p,inspector_width:Math.max(260,Math.min(520,value))}));
  return <div className="thermal-inspector-resizer" role="separator" tabIndex={0} aria-label="Properties panel width" aria-orientation="vertical" aria-valuemin={260} aria-valuemax={520} aria-valuenow={presentation.value.inspector_width}
    onKeyDown={event=>{if(!['ArrowLeft','ArrowRight','Home','End'].includes(event.key))return;event.preventDefault();width(event.key==='Home'?260:event.key==='End'?520:live.current.value.inspector_width+(event.key==='ArrowLeft'?20:-20));}}
    onPointerDown={event=>{if(event.button!==0)return;event.preventDefault();stop.current();const start=event.clientX,initial=live.current.value.inspector_width,id=event.pointerId;const move=(e:PointerEvent)=>{if(e.pointerId===id)width(initial+start-e.clientX);};const end=(e:PointerEvent)=>{if(e.pointerId===id)stop.current();};stop.current=()=>{window.removeEventListener('pointermove',move);window.removeEventListener('pointerup',end);window.removeEventListener('pointercancel',end);};window.addEventListener('pointermove',move);window.addEventListener('pointerup',end);window.addEventListener('pointercancel',end);}}/>;
}
