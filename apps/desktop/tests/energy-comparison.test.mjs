// Render the production workspace with synthetic read-only service replies.
// These checks cover presentation and non-mutation, not native acceptance.
import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import {transformWithOxc} from 'vite';
import {renderToStaticMarkup} from 'react-dom/server';

const states=[];let cursor=0,pair,doc,initialProject,mutations=0;
const host={
  useState(initial){const i=cursor++;if(!(i in states))states[i]=typeof initial==='function'?initial():initial;return[states[i],v=>{states[i]=typeof v==='function'?v(states[i]):v;}];},
  useRef(initial){const i=cursor++;return states[i]??(states[i]={current:initial});},
  useCallback(fn){cursor++;return fn;},
  useEffect(){},
  invoke(){throw new Error('No native command expected');},
  useEnergyRead(command,args){
    if(command==='energy_default_runs')return{value:{A:'retained-A',B:'retained-B'},loading:false,error:null};
    if(command==='energy_compare'&&args)return pair;
    return{value:null,loading:false,error:null};
  },
  Assessment:()=>null,Cell:()=>null,ThermalEditor:()=>null,GeometryAuthoring:()=>null,NewEnergyProject:()=>null,EvidenceSettings:()=>null,ThermalView:()=>null,EnergyInvestigation:()=>null,ThermalDifferences:()=>null,EnergyPreflight:()=>null,ReportWorkbench:()=>null,ThermalInspectorResizer:()=>null,EnergySensitivity:()=>null,EnergyCommandBar:()=>null,RunHistory:()=>null,EnergyDependencySettings:()=>null,
  DesignProposal:()=>{throw Error("Proposal panel is outside this read-only comparison test");},DesignReports:()=>{throw Error("Report panel is outside this read-only comparison test");},DesignCopy:()=>null,useEnergyDependencyPaths:()=>({paths:{engineDir:'',typstDir:''},warning:'',setPath(){},reset(){}}),
  ScopeRevisions:()=>null,ReferenceWorkbench:()=>null,ProjectCapacity:()=>null,useReferenceModel:()=>({overlay:null,context:null,assessment:null,error:''}),
  readable:s=>s.charAt(0).toUpperCase()+s.slice(1).replaceAll('_',' '),
};
globalThis.__energyComparisonHost=host;
const sourceUrl=new URL('../src/energy/EnergyWorkspace.tsx',import.meta.url);
const {code}=await transformWithOxc(fs.readFileSync(sourceUrl,'utf8'),sourceUrl.pathname,{jsx:{runtime:'automatic'}});
const inspectorUrl=new URL('../src/energy/thermal-inspector-target.ts',import.meta.url);
const inspector=await transformWithOxc(fs.readFileSync(inspectorUrl,'utf8'),inspectorUrl.pathname);
const inspectorModule=`data:text/javascript;base64,${Buffer.from(inspector.code).toString('base64')}`;
const referenceDraftUrl=new URL('../src/reference/reference-drafts.ts',import.meta.url);
const referenceDraft=await transformWithOxc(fs.readFileSync(referenceDraftUrl,'utf8'),referenceDraftUrl.pathname);
const referenceDraftModule=`data:text/javascript;base64,${Buffer.from(referenceDraft.code).toString('base64')}`;
// Load the actual shared display contract and its pure dependencies. Presentation
// writes are local navigation, not project mutations or scientific transactions.
async function displayModule(url){
  const {code}=await transformWithOxc(fs.readFileSync(url,'utf8'),url.pathname);let linked=code;
  for(const match of code.matchAll(/import\s+([^;]+?)\s+from\s+(['"])([^'"]+)\2;/g)){
    const specifier=match[3],target=specifier.startsWith('.')?await displayModule(new URL(`${specifier}.ts`,url)):import.meta.resolve(specifier);
    linked=linked.replace(match[0],`import ${match[1]} from ${JSON.stringify(target)};`);
  }
  return `data:text/javascript;base64,${Buffer.from(linked).toString('base64')}`;
}
const presentationModule=await displayModule(new URL('../src/energy/thermal-presentation.ts',import.meta.url));
const referenceSelectionModule=await displayModule(new URL('../src/reference/selection.ts',import.meta.url));
const scopeReviewDraftModule=await displayModule(new URL('../src/reference/scope-review-drafts.ts',import.meta.url));
const inspectionSource=new URL('../src/energy/EvidenceInspection.tsx',import.meta.url);
const inspectionCode=await transformWithOxc(fs.readFileSync(inspectionSource,'utf8'),inspectionSource.pathname,{jsx:{runtime:'automatic'}});
const inspectionModule='data:text/javascript;base64,'+Buffer.from(inspectionCode.code.replace(/from (['"])react\/jsx-runtime\1/g,`from ${JSON.stringify(import.meta.resolve('react/jsx-runtime'))}`)).toString('base64');
const {ThermalPresentationStore}=await import(presentationModule);
// This older read-only comparison projection test excludes unrelated panels.
// Real editing, metadata, reference and viewport interactions have dedicated DOM suites.
const mocked=new Set(['react','@tauri-apps/api/core','./useEnergyRead','./EnergyAssessment','./ThermalEditor','./GeometryAuthoring','./NewEnergyProject','./EvidenceSettings','./ThermalView','./EnergyInvestigation','./ThermalDifferences','./EnergyPreflight','./ProjectCapacity','./ReportWorkbench','./ThermalInspectorResizer','./EnergySensitivity','./EnergyCommandBar','./RunHistory','./EnergyDependencySettings','./energy-dependency-settings','./DesignCopy','./DesignProposal','./DesignReports','../reference/ScopeRevisions','../reference/ReferenceWorkbench','../reference/useReferenceModel']);
const linked=code.replace(/import\s+(['"])\.\/energy\.css\1;/g,'').replace(/import\s+([^;]+?)\s+from\s+(['"])([^'"]+)\2;/g,(_,binding,_q,specifier)=>{
  if(mocked.has(specifier)){
    const parts=[],named=binding.match(/\{([^}]+)\}/)?.[1];
    if(named)parts.push(`const {${named.replace(/\bas\b/g,':')}}=globalThis.__energyComparisonHost;`);
    const defaultName=binding.split(',')[0].trim();
    if(!defaultName.startsWith('{'))parts.push(`const ${defaultName}=globalThis.__energyComparisonHost.${defaultName};`);
    return parts.join('\n');
  }
  return `import ${binding} from ${JSON.stringify(specifier==='./EvidenceInspection'?inspectionModule:specifier==='./thermal-presentation'?presentationModule:specifier==='./thermal-inspector-target'?inspectorModule:specifier==='../reference/reference-drafts'?referenceDraftModule:specifier==='../reference/selection'?referenceSelectionModule:specifier==='../reference/scope-review-drafts'?scopeReviewDraftModule:specifier.startsWith('.')?new URL(specifier,sourceUrl).href:import.meta.resolve(specifier))};`;
});
const {default:EnergyWorkspace}=await import(`data:text/javascript;base64,${Buffer.from(linked).toString('base64')}`);
const nodes=t=>!t||typeof t!=='object'?[]:Array.isArray(t)?t.flatMap(nodes):[t,...nodes(t.props?.children)];
const draw=()=>{cursor=0;return EnergyWorkspace({doc});};
function setup(differences){
  states.length=0;mutations=0;
  const design={model:{name:'Original synthetic zone',zones:[{id:'zone-1',name:'Zone 1',storey:0}]},study:null};
  doc={documentId:1,alternative:'A',path:'synthetic-project',project:{energy:{designs:{A:design,B:structuredClone(design)},runs:[{id:'retained-A',alternative_id:'A'},{id:'retained-B',alternative_id:'B'}]}},referenceDrafts:{draft:null},history:{past:[],future:[]},busy:false,running:false,pending:false,energyLocked:false,job:null,commitEnergy(){mutations++;},setAlternative(){mutations++;},setEnergyLocked(){mutations++;}};
  const presentationStore=new ThermalPresentationStore({getItem:()=>null,setItem(){}}),presentationKey='["synthetic-project","A"]';
  doc.thermalPresentation={key:presentationKey,get value(){return presentationStore.get(presentationKey);},update(change){presentationStore.set(presentationKey,change(presentationStore.get(presentationKey)));},resetLayout(){throw Error('No reset-layout action expected');}};
  initialProject=structuredClone(doc.project);
  const savings={absolute:{state:'value',value:1,display:'1',unit:'kwh'},percent:{state:'value',value:2,display:'2',unit:'percent'}};
  pair={value:{data:{comparison:{input_relation:{differences,service_warnings:[]},lighting_service_equivalence:'Missing service evidence'}},presentation:{facility_energy_savings:savings,facility_peak_savings:savings,end_uses:[]}},loading:false,error:null};
  const tree=draw(),compare=nodes(tree).find(n=>n.type==='button'&&n.props.children==='Compare');
  assert.ok(compare);compare.props.onClick();assert.equal(doc.thermalPresentation.value.workspace.panel,'compare');return draw();
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
