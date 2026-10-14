import {useState} from 'react';
export type EnergyCommand={name:string;description:string;disabled?:string;run:()=>void|Promise<unknown>};
/** A discoverable registry of existing actions. No expression or shell parser. */
export default function EnergyCommandBar({commands}:{commands:EnergyCommand[]}){
  const [text,setText]=useState(''),[history,setHistory]=useState<string[]>([]),[expanded,setExpanded]=useState(false),[error,setError]=useState('');
  const matches=commands.filter(c=>`${c.name} ${c.description}`.toLowerCase().includes(text.trim().toLowerCase()));
  const execute=async(name:string)=>{const command=commands.find(c=>c.name.toLowerCase()===name.trim().toLowerCase());
    if(!command){setError('Choose an exact supported command from the list.');return;}if(command.disabled){setError(command.disabled);return;}
    setError('');try{await command.run();setHistory(h=>[...h.slice(-39),command.name]);setText('');}catch(e){setError(String(e));}
  };
  return <section className="energy-command" aria-label="Thermal commands"><form onSubmit={e=>{e.preventDefault();if(text.trim())void execute(text);else setExpanded(true);}}><label>Command<input value={text} autoComplete="off" list="thermal-command-list" placeholder="NewEnergy, Geometry, FitSelected, Run…" onChange={e=>{setText(e.target.value);setError('');}} onKeyDown={e=>{if(e.key==='Escape'){e.stopPropagation();setText('');setExpanded(false);setError('');}}}/></label><datalist id="thermal-command-list">{matches.map(c=><option key={c.name} value={c.name}>{c.description}</option>)}</datalist><button type="submit">Execute</button><button type="button" aria-expanded={expanded} onClick={()=>setExpanded(!expanded)}>Commands / history</button></form>{error&&<p role="alert">{error}</p>}{expanded&&<div className="energy-command-list"><table><caption>Bounded application actions; exact numeric inputs open the corresponding form</caption><tbody>{matches.map(c=><tr key={c.name}><th><button disabled={Boolean(c.disabled)} title={c.disabled??c.description} onClick={()=>void execute(c.name)}>{c.name}</button></th><td>{c.disabled??c.description}</td></tr>)}</tbody></table><details><summary>Session command history</summary><ol>{history.map((h,i)=><li key={i}>{h}</li>)}</ol></details></div>}</section>;
}
