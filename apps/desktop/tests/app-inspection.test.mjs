// Exercise the production App render/handlers with a synchronous hook host.
// Native services and child renderers are explicit mocks; these tests establish
// routing and non-mutation only, not React effect timing or desktop acceptance.
import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import { transformWithOxc } from 'vite';

// Reference guard behavior is exercised with the real hook/forms in
// reference-drafts.test.mjs; this file only isolates top-level inspection routing.
const componentNames=['ScopeReviewDraftGuard','ReferenceDraftGuard','EnergyDraftGuard','EnergyWorkspace','SelectionBar','PropertiesPanel','ParameterField','AnalysisContent','Stats','Legend','ClimateStudyPanel','ClimateResultControls','ClimateResultsContent','ExportRunDialog'];
const components=Object.fromEntries(componentNames.map(name=>[name,Object.defineProperty(()=>null,'name',{value:name})]));
const fixture=JSON.parse(fs.readFileSync(new URL('./viewport-fixture.json',import.meta.url)));
const staticRun={id:'static-A',alternative_id:'A',model:fixture.model,validated:fixture,lux:fixture.sensor_grid.sensors.map(()=>100),statistics:{min:100,mean:100,max:100}};
const climateRun={id:'historical-method2-A',alternative_id:'A',study:{calculation:{mode:'annual'}},series:{sensor_ids:fixture.sensor_grid.sensors.map(s=>s.id),interval_count:8760}};
const states=[];let cursor=0,doc,climate,mutations,keyboardHandler;
globalThis.window={addEventListener:(name,fn)=>{if(name==='keydown')keyboardHandler=fn;},removeEventListener(){}};
const host={...components,
  useState(initial){const index=cursor++;if(!(index in states))states[index]=typeof initial==='function'?initial():initial;return [states[index],value=>{states[index]=typeof value==='function'?value(states[index]):value;}];},
  useRef(initial){const index=cursor++;return states[index]??(states[index]={current:initial});},
  useMemo:fn=>fn(),useEffect(fn){if(fn.toString().includes('addEventListener("keydown"'))fn();},lazy:()=>Object.defineProperty(()=>null,'name',{value:'SceneView'}),Suspense:'suspense',
  useDocument:()=>doc,useClimateResults:()=>climate,
  useHistoricalComparison:()=>({a:staticRun,b:null,comparison:null,comparisonError:'',comparing:false,selectRun(){}}),
  invoke:async()=>{throw new Error('Unexpected native service request from inspection');},isTauri:()=>false,getCurrentWindow(){throw new Error('Unexpected window operation');},
  semanticObjects:()=>[],objectLabel:id=>id,objectVisibility:()=>({visible:true,locked:false}),selectableIds:ids=>ids,
  qualificationLabel:()=> 'Scientific qualification unassessed',errorText:String,fmt:String,METHOD:'Static daylight illuminance under a prescribed CIE standard overcast sky.',
};
globalThis.__greenlensAppInspectionHost=host;
const mocked=new Set(['./reference/ScopeReviewDraftGuard','./reference/ReferenceDraftGuard','./energy/EnergyDraftGuard','./energy/EnergyWorkspace','react','@tauri-apps/api/core','@tauri-apps/api/window','./viewport/entities','./workspace/useDocument','./workspace/useHistoricalComparison','./workspace/PropertiesPanel','./workspace/SelectionBar','./workspace/AnalysisContent','./workspace/ClimateStudyPanel','./workspace/useClimateResults','./workspace/ClimateResultsContent','./workspace/QualificationStatus']);
const source=fs.readFileSync(new URL('../src/App.tsx',import.meta.url),'utf8');
const {code}=await transformWithOxc(source,'App.tsx',{jsx:{runtime:'automatic'}});
const linked=code.replace(/import\s+([^;]+?)\s+from\s+(['"])([^'"]+)\2;/g,(_,binding,_quote,specifier)=>{
  if(mocked.has(specifier)){
    const parts=[];const named=binding.match(/\{([^}]+)\}/)?.[1];
    if(named)parts.push(`const {${named.replace(/\bas\b/g,':')}}=globalThis.__greenlensAppInspectionHost;`);
    const defaultName=binding.split(',')[0].trim();
    if(!defaultName.startsWith('{'))parts.push(`const ${defaultName}=globalThis.__greenlensAppInspectionHost.${defaultName};`);
    return parts.join('\n');
  }
  const target=specifier.startsWith('.')?new URL(specifier,new URL('../src/App.tsx',import.meta.url)).href:import.meta.resolve(specifier);
  return `import ${binding} from ${JSON.stringify(target)};`;
});
const {default:App}=await import(`data:text/javascript;base64,${Buffer.from(linked).toString('base64')}`);
function nodes(tree){return !tree||typeof tree!=='object'?[]:Array.isArray(tree)?tree.flatMap(nodes):[tree,...nodes(tree.props?.children)];}
const render=()=>{cursor=0;return App();};
const find=(tree,predicate)=>nodes(tree).find(predicate);
const byLabel=(tree,label)=>find(tree,n=>n.props?.['aria-label']===label);
const byComponent=(tree,name)=>find(tree,n=>n.type===components[name]);
function clickText(tree,text){const node=find(tree,n=>n.type==='button'&&n.props.children===text);assert.ok(node,`button ${text}`);node.props.onClick();return render();}
function setup(mode){
  states.length=0;mutations=0;
  const study={calculation:{mode}};
  doc={documentId:1,alternative:'A',project:{id:'doc',name:'Fixture',alternatives:[{id:'A',model:fixture.model}],climate:{studies:{A:study},runs:[],weather:[]}},study,
    validated:fixture,validating:false,model:fixture.model,results:[staticRun],climateResults:[climateRun],currentClimate:null,display:{mode:'analysis',cutaway:true,layers:{},objects:{}},
    path:'mock-project',history:{past:[],future:[]},job:null,error:'',message:'',setError(){},commitStudy(){mutations++;},setDisplay(){mutations++;},commit(){mutations++;}};
  climate={run:climateRun,values:{values:[900],units:'lx'},canOverlay:false,scale:900,a:null,b:null};
  return render();
}
function chooseFamily(tree,family){const choice=byLabel(tree,'Result inspection family');assert.ok(choice,'Independent result family selector is available');choice.props.onChange({target:{value:family}});return render();}

test('App exposes historical climate results while accepted study stays static',()=>{
  let tree=setup('static_overcast');const original=structuredClone(doc.project);
  tree=chooseFamily(tree,'climate');tree=clickText(tree,'Analysis');
  assert.ok(byComponent(tree,'ClimateResultControls'));
  tree=clickText(tree,'Sensor table');assert.ok(byComponent(tree,'ClimateResultsContent'));
  assert.equal(byComponent(tree,'ClimateResultsContent').props.tab,'table');
  assert.equal(find(tree,n=>n.type?.name==='SceneView').props.lux,null);
  assert.deepEqual(doc.project,original);assert.equal(mutations,0);
  tree=clickText(tree,'A/B compare');assert.equal(byComponent(tree,'ClimateResultsContent').props.tab,'compare');
  tree=clickText(tree,'Time series');assert.equal(byComponent(tree,'ClimateResultsContent').props.tab,'series');
});

test('App exposes explicit static snapshots under an annual study without a historical overlay',()=>{
  let tree=setup('annual');const original=structuredClone(doc.project);
  tree=chooseFamily(tree,'static');tree=clickText(tree,'Analysis');
  const snapshots=byLabel(tree,'Static result snapshot');assert.ok(snapshots);
  snapshots.props.onChange({target:{value:staticRun.id}});tree=render();
  tree=clickText(tree,'Sensor table');const table=byComponent(tree,'AnalysisContent');
  assert.equal(table.props.current.id,staticRun.id);assert.equal(table.props.validated,staticRun.validated);
  assert.match(table.props.snapshotRelationText,/CURRENT-STUDY MISMATCH/);
  assert.equal(table.props.canSelectSensors,false);
  assert.equal(find(tree,n=>n.type?.name==='SceneView').props.lux,null);
  assert.ok(nodes(tree).some(n=>n.type==='p'&&typeof n.props.children==='string'&&n.props.children.includes('CURRENT-STUDY MISMATCH')));
  tree=clickText(tree,'A/B compare');assert.equal(byComponent(tree,'AnalysisContent').props.tab,'compare');
  assert.equal(byComponent(tree,'AnalysisContent').props.currentStudyModes.A,'annual');
  assert.deepEqual(doc.project,original);assert.equal(mutations,0);
});

test('explicit inspector survives study edits; follow-study and document reset remain available',()=>{
  let tree=setup('annual');tree=chooseFamily(tree,'climate');
  doc.study={calculation:{mode:'static_overcast'}};doc.project.climate.studies.A=doc.study;tree=render();
  assert.equal(byLabel(tree,'Result inspection family').props.value,'climate');
  tree=chooseFamily(tree,'study');assert.equal(byLabel(tree,'Result inspection family').props.value,'study');
  assert.equal(find(tree,n=>n.type==='button'&&n.props.children==='Time series'),undefined);
  tree=chooseFamily(tree,'climate');doc.documentId=2;tree=render();
  assert.equal(byLabel(tree,'Result inspection family').props.value,'study');assert.equal(mutations,0);
});

test('changing inspector from climate time series to static history routes to a sensor table',()=>{
  let tree=setup('annual');tree=chooseFamily(tree,'climate');tree=clickText(tree,'Time series');
  assert.equal(byComponent(tree,'ClimateResultsContent').props.tab,'series');
  tree=chooseFamily(tree,'static');assert.equal(byComponent(tree,'AnalysisContent').props.tab,'table');
  assert.equal(doc.study.calculation.mode,'annual');assert.equal(mutations,0);
});

test('follow-study routes an open series drawer to the static table after a scientific study edit',()=>{
  let tree=setup('annual');tree=clickText(tree,'Time series');
  assert.equal(byComponent(tree,'ClimateResultsContent').props.tab,'series');
  doc.study={calculation:{mode:'static_overcast'}};doc.project.climate.studies.A=doc.study;tree=render();
  assert.equal(byComponent(tree,'AnalysisContent').props.tab,'table');
  assert.equal(byLabel(tree,'table').props.className,'analysis-drawer');
  assert.equal(mutations,0);
});

test('App labels pending hourly reads as loading without claiming a scientific mismatch or showing old values',()=>{
  setup('annual');climate={...climate,values:null,canOverlay:false,matchesCurrentGeometry:true,checkingRelationship:false,loading:true};
  let tree=render(),scene=find(tree,n=>n.type?.name==='SceneView');
  assert.equal(scene.props.lux,null);assert.equal(scene.props.resultLoading,true);assert.equal(scene.props.validated,fixture);
  assert.equal(scene.props.display,doc.display);
  assert.equal(find(tree,n=>n.props?.className==='stale-banner'),undefined);
  climate={...climate,loading:false,matchesCurrentGeometry:false};tree=render();
  assert.ok(find(tree,n=>n.props?.className==='stale-banner'),'a completed Rust mismatch still warns');
  assert.equal(find(tree,n=>n.type?.name==='SceneView').props.resultLoading,false);
});

test('App distinguishes a pending current lookup from a completed absence of matching evidence',()=>{
  setup('annual');climate={...climate,run:null,values:null,canOverlay:false,matchesCurrentGeometry:false,checkingRelationship:true,loading:false};
  let tree=render();assert.equal(find(tree,n=>n.props?.className==='stale-banner'),undefined);
  assert.equal(find(tree,n=>n.type?.name==='SceneView').props.resultLoading,true);
  climate={...climate,checkingRelationship:false};tree=render();assert.ok(find(tree,n=>n.props?.className==='stale-banner'));
});

test('energy discipline hides legacy command surfaces and preserves mounted daylight viewport',()=>{let tree=setup('annual');const before=structuredClone(doc.project);tree=clickText(tree,'Energy');assert.ok(byComponent(tree,'EnergyWorkspace'));assert.equal(find(tree,n=>n.props?.className==='daylight-container').props.hidden,true);assert.equal(byLabel(tree,'Application menu').props.className,'energy-active-hidden');const footer=find(tree,n=>n.type==='footer');assert.equal(nodes(footer).filter(n=>n.type==='button').length,0);assert.ok(find(tree,n=>n.type?.name==='SceneView'));tree=clickText(tree,'Daylight');assert.equal(find(tree,n=>n.props?.className==='daylight-container').props.hidden,false);assert.equal(byComponent(tree,'EnergyWorkspace'),undefined);assert.deepEqual(doc.project,before);assert.equal(mutations,0);});

test('actual App energy keyboard routing keeps text native and never dispatches a daylight repeat',()=>{let tree=setup('annual');const actions=[];Object.assign(doc,{replace:k=>actions.push(k),save:()=>actions.push('save'),saveAs:()=>actions.push('save_as'),traverse:k=>actions.push(k)});tree=clickText(tree,'Energy');assert.equal(typeof keyboardHandler,'function');let prevented=0;const key=(key,extra={})=>keyboardHandler({key,preventDefault(){prevented++;},target:{tagName:'DIV'},...extra});key('n',{ctrlKey:true});key('s',{ctrlKey:true,shiftKey:true});key('z',{ctrlKey:true});for(const k of ['Enter','Delete','F8','F9','r'])key(k);assert.deepEqual(actions,['New Energy','save_as','undo']);assert.equal(prevented,3);key('z',{ctrlKey:true,target:{tagName:'TEXTAREA'}});assert.equal(actions.length,3);doc.busy=true;tree=render();assert.equal(find(tree,n=>n.type==='button'&&n.props.children==='Daylight').props.disabled,true);});
