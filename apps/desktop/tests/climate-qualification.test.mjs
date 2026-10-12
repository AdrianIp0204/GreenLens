// Production React markup with explicit synthetic service replies. These checks
// establish presentation, never native interaction or physical qualification.
import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import { transformWithOxc } from 'vite';
import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';

async function load(relative) {
  const url=new URL(relative,import.meta.url);
  const {code}=await transformWithOxc(fs.readFileSync(url,'utf8'),url.pathname,{jsx:{runtime:'automatic'}});
  const linked=code.replace(/import\s+([^;]+?)\s+from\s+(['"])([^'"]+)\2;/g,(_,binding,_q,specifier)=>{
    if(specifier==='./AnalysisContent')return 'const fmt=String;';
    if(specifier==='./WeatherCalendarPicker')return 'const WeatherCalendarPicker=()=>null;';
    if(specifier==='./QualificationStatus')return 'const QualificationStatus=globalThis.__qualificationComponent;';
    const target=specifier.startsWith('.')?new URL(specifier,url).href:import.meta.resolve(specifier);
    return `import ${binding} from ${JSON.stringify(target)};`;
  }).replace(/import\s+['"][^'"]+\.css['"];?/g,'');
  return import(`data:text/javascript;base64,${Buffer.from(linked).toString('base64')}`);
}
const {QualificationStatus,qualificationLabel}=await load('../src/workspace/QualificationStatus.tsx');
globalThis.__qualificationComponent=QualificationStatus;
const {ClimateResultControls,ClimateResultsContent}=await load('../src/workspace/ClimateResultsContent.tsx');
const assessment=status=>({
  schema_version:'synthetic.assessment',run_id:'run-A',record_sha256:'record-seal',
  process_completion:'retained_successful_exit_records',evidence_integrity:'sealed_metadata_checked',
  current_input_relation:{physical:'matches',metrics:'differs'},
  scientific_qualification:{status,registry_version:'synthetic.v1',registry_sha256:'registry-seal',
    identity:{method_version:'synthetic-method/5',adapter_version:'synthetic/5',profile_sha256:'profile-seal',engine_sha256:'engine-seal',scope:'synthetic fixture'},
    profile_display_name:'Candidate profile',reason:status==='failed'?'Six numerical checks failed in the declared study.':'Scope-specific assessment only.',
    evidence:[{id:'synthetic-evidence-1',sha256:'evidence-seal',tested_scope:'Original fixture only',outcome:'failed',reason:'Frozen limit exceeded.'}],
    limitations:['Not a statement about every building.']},
});
const markup=(component,props)=>renderToStaticMarkup(createElement(component,props));

test('failed qualification remains visible next to successful execution and matching inputs',()=>{
  const html=markup(QualificationStatus,{assessment:assessment('failed')});
  assert.match(html,/Numerical acceptance failed/);assert.match(html,/Six numerical checks failed/);
  assert.match(html,/process exits succeeded/);assert.match(html,/Match this snapshot/);
  assert.match(html,/Sealed metadata checked/);assert.match(html,/did not reread the full time series/);
  assert.match(html,/Not current for the present physical inputs and\/or metric settings/);assert.match(html,/synthetic-evidence-1/);
  assert.match(html,/Original fixture only/);assert.match(html,/profile-seal/);assert.match(html,/engine-seal/);
});
test('geometry-only noncurrency does not claim saved metric settings changed',()=>{
  const value=assessment('failed');
  value.current_input_relation={physical:'differs',metrics:'differs'};
  const html=markup(QualificationStatus,{assessment:value});
  assert.match(html,/Retained metric results/);
  assert.match(html,/Not current for the present physical inputs and\/or metric settings/);
  assert.doesNotMatch(html,/Saved metric settings|Differ from current settings/);
  value.current_input_relation={physical:'matches',metrics:'matches'};
  assert.match(markup(QualificationStatus,{assessment:value}),/Current for these physical inputs and metric settings/);
});
test('unassessed, missing and unfamiliar statuses never become qualified',()=>{
  for(const value of [null,assessment('unassessed'),assessment('future_status')]){
    assert.equal(qualificationLabel(value),'Scientific qualification unassessed');
    assert.match(markup(QualificationStatus,{assessment:value}),/Scientific qualification unassessed/);
  }
  const error=markup(QualificationStatus,{assessment:null,error:'Assessment seal mismatch'});
  assert.match(error,/Assessment seal mismatch/);assert.match(error,/does not establish numerical accuracy/);
});
test('limited qualification states scope and never claims release acceptance',()=>{
  const html=markup(QualificationStatus,{assessment:assessment('limited_qualified')});
  assert.match(html,/Qualified only within the stated scope/);assert.match(html,/Original fixture only/);
  assert.doesNotMatch(html,/Release accepted|certified|verified results/i);
});
const run={id:'run-A',alternative_id:'A',study:{calculation:{mode:'annual'},metrics:{}},series:{interval_count:8760}};
const base={run,selectedRunId:'current',field:'da',annual:true,selectRun(){},setField(){},qualification:{assessment:assessment('failed'),error:''},values:null,loading:false,error:'',selectedRelation:'Current physical inputs match.'};
test('main result controls expose qualification before any values arrive',()=>{
  const html=markup(ClimateResultControls,{controller:base,records:[run],alternative:'A'});
  assert.match(html,/Numerical acceptance failed/);assert.match(html,/Six numerical checks failed/);
  assert.match(html,/Result scientific qualification/);
});
test('both comparison snapshots expose separate qualification even when pair is incompatible',()=>{
  const controller={...base,a:{id:'run-A'},b:{id:'run-B'},pairTime:{field:'da',annual:true},pairRelation:()=> 'Current inputs differ',
    qualificationA:{assessment:assessment('failed'),error:''},qualificationB:{assessment:assessment('unassessed'),error:''},
    selectPair(){},setPairField(){},comparing:false,comparison:null,comparisonError:'Incompatible methods'};
  const html=markup(ClimateResultsContent,{tab:'compare',controller,records:[run,{...run,id:'run-B',alternative_id:'B'}],selection:[],onSelect(){}});
  assert.match(html,/A: Numerical acceptance failed/);assert.match(html,/B: Scientific qualification unassessed/);
  assert.match(html,/Incompatible methods/);assert.equal((html.match(/scientific qualification"/g)??[]).length,2);
});
