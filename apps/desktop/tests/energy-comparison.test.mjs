// Render the production workspace with synthetic read-only service replies.
// These checks cover presentation and non-mutation, not native acceptance.
import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import {transformWithOxc} from 'vite';
import {renderToStaticMarkup} from 'react-dom/server';

const states=[];let cursor=0,pair,doc,initialProject,mutations=0;
const host={
  useState(initial){const i=cursor++;if(!(i in states))states[i]=initial;return[states[i],v=>{states[i]=v;}];},
  useEffect(){},
  invoke(){throw new Error('No native command expected');},
  useEnergyRead(command,args){
    if(command==='energy_default_runs')return{value:{A:'retained-A',B:'retained-B'},loading:false,error:null};
    if(command==='energy_compare'&&args)return pair;
    return{value:null,loading:false,error:null};
  },
  Assessment:()=>null,Cell:()=>null,ThermalEditor:()=>null,EvidenceSettings:()=>null,ThermalView:()=>null,EnergySeries:()=>null,
  readable:s=>s.charAt(0).toUpperCase()+s.slice(1).replaceAll('_',' '),
};
globalThis.__energyComparisonHost=host;
const sourceUrl=new URL('../src/energy/EnergyWorkspace.tsx',import.meta.url);
const {code}=await transformWithOxc(fs.readFileSync(sourceUrl,'utf8'),sourceUrl.pathname,{jsx:{runtime:'automatic'}});
const mocked=new Set(['react','@tauri-apps/api/core','./useEnergyRead','./EnergyAssessment','./ThermalEditor','./EvidenceSettings','./ThermalView','./EnergySeries']);
const linked=code.replace(/import\s+(['"])\.\/energy\.css\1;/g,'').replace(/import\s+([^;]+?)\s+from\s+(['"])([^'"]+)\2;/g,(_,binding,_q,specifier)=>{
  if(mocked.has(specifier)){
    const parts=[],named=binding.match(/\{([^}]+)\}/)?.[1];
    if(named)parts.push(`const {${named.replace(/\bas\b/g,':')}}=globalThis.__energyComparisonHost;`);
    const defaultName=binding.split(',')[0].trim();
    if(!defaultName.startsWith('{'))parts.push(`const ${defaultName}=globalThis.__energyComparisonHost.${defaultName};`);
    return parts.join('\n');
  }
  return `import ${binding} from ${JSON.stringify(specifier.startsWith('.')?new URL(specifier,sourceUrl).href:import.meta.resolve(specifier))};`;
});
const {default:EnergyWorkspace}=await import(`data:text/javascript;base64,${Buffer.from(linked).toString('base64')}`);
const nodes=t=>!t||typeof t!=='object'?[]:Array.isArray(t)?t.flatMap(nodes):[t,...nodes(t.props?.children)];
const draw=()=>{cursor=0;return EnergyWorkspace({doc});};
function setup(differences){
  states.length=0;mutations=0;
  const design={model:{name:'Original synthetic zone',zones:[{id:'zone-1',name:'Zone 1',storey:0}]},study:null};
  doc={documentId:1,alternative:'A',path:'synthetic-project',project:{energy:{designs:{A:design,B:structuredClone(design)},runs:[{id:'retained-A',alternative_id:'A'},{id:'retained-B',alternative_id:'B'}]}},history:{past:[],future:[]},busy:false,running:false,pending:false,energyLocked:false,job:null,commitEnergy(){mutations++;},setAlternative(){mutations++;},setEnergyLocked(){mutations++;}};
  initialProject=structuredClone(doc.project);
  const savings={absolute:{state:'value',value:1,display:'1',unit:'kwh'},percent:{state:'value',value:2,display:'2',unit:'percent'}};
  pair={value:{data:{comparison:{input_relation:{differences,service_warnings:[]},lighting_service_equivalence:'Missing service evidence'}},presentation:{facility_energy_savings:savings,facility_peak_savings:savings,end_uses:[]}},loading:false,error:null};
  const tree=draw(),compare=nodes(tree).find(n=>n.type==='button'&&n.props.children==='Compare');
  assert.ok(compare);compare.props.onClick();return draw();
}
const differencesSection=t=>nodes(t).find(n=>n.props?.['aria-label']==='Baseline/proposed design differences');

test('Compare shows every Rust design difference in order as escaped text without editing inputs',()=>{
  const differences=Object.freeze(['Cooling COP differs: A 3, B 4','Fan efficiency differs: A 0.5, B 0.6','<script>not executable</script>','Repeated retained label','Repeated retained label']);
  const tree=setup(differences),section=differencesSection(tree);
  assert.ok(section,'difference inventory is visible in the actual Compare tab');
  assert.deepEqual(nodes(section).filter(n=>n.type==='li').map(n=>n.props.children),differences);
  const html=renderToStaticMarkup(section);
  assert.match(html,/Baseline\/proposed design differences/);
  assert.match(html,/Retained baseline A and proposed B snapshots/);
  assert.match(html,/&lt;script&gt;not executable&lt;\/script&gt;/);
  assert.doesNotMatch(html,/<script>/);
  draw();assert.deepEqual(doc.project,initialProject);assert.equal(mutations,0);
});

test('Compare gives an explicit empty inventory without claiming identical service or qualification',()=>{
  const section=differencesSection(setup([]));assert.ok(section);
  assert.equal(nodes(section).filter(n=>n.type==='li').length,0);
  const html=renderToStaticMarkup(section);
  assert.match(html,/No design differences were reported for the selected baseline\/proposed snapshots/);
  assert.doesNotMatch(html,/equivalent|qualified|identical inputs/i);
});

test('a pending or rejected comparison does not leave a stale difference inventory visible',()=>{
  assert.ok(differencesSection(setup(['Old retained difference'])));
  for(const state of [{value:null,loading:true,error:null},{value:null,loading:false,error:'Selected evidence is invalid'}]){
    pair=state;assert.equal(differencesSection(draw()),undefined);
  }
  assert.equal(mutations,0);
});
