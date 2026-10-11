import test from 'node:test';
import assert from 'node:assert/strict';
import { runStudyTask } from '../src/workspace/climate-study-task.mjs';
const deferred = () => { let resolve; const promise = new Promise(done => { resolve = done; }); return {promise,resolve}; };
const fixture = () => {
  const context={current:1},inFlight={current:false},disabled={current:false},events=[];
  return {context,inFlight,disabled,events,handlers:{start:()=>events.push('start'),failure:e=>events.push(String(e)),finish:()=>events.push('finish')}};
};

test('successful import completion closes its wizard even while its parent busy render remains true', async()=>{
  const f=fixture(),done=deferred();let open=true,status='';
  const pending=runStudyTask(f.context,f.inFlight,f.disabled,async(current,canContinue)=>{
    assert.equal(canContinue(),true);
    // The shared document callback makes the parent busy until React renders
    // again after resolving. This is a UI state fixture, not an engine mock.
    f.disabled.current=true;
    const accepted=await done.promise;
    assert.equal(canContinue(),false,'no second document action may start while busy');
    if(current()){if(accepted)open=false;status=accepted?'retained':'draft retained';}
  },f.handlers);
  done.resolve(true);await pending;
  assert.equal(open,false);assert.equal(status,'retained');assert.equal(f.inFlight.current,false);
  assert.deepEqual(f.events,['start','finish']);
});

test('failed import retains declaration and later context replacement suppresses completion', async()=>{
  for(const replaced of [false,true]){
    const f=fixture(),done=deferred();let open=true,status='';
    const pending=runStudyTask(f.context,f.inFlight,f.disabled,async current=>{
      const accepted=await done.promise;
      if(current()){if(accepted)open=false;status=accepted?'retained':'draft retained';}
    },f.handlers);
    if(replaced)f.context.current++;
    done.resolve(replaced);await pending;
    assert.equal(open,true);assert.equal(status,replaced?'':'draft retained');
    assert.deepEqual(f.events,replaced?['start']:['start','finish']);
  }
});

test('task gate blocks overlapping/disabled starts and suppresses obsolete failures',async()=>{
  const f=fixture(),done=deferred();
  const pending=runStudyTask(f.context,f.inFlight,f.disabled,async()=>{await done.promise;throw new Error('old context failure');},f.handlers);
  assert.equal(await runStudyTask(f.context,f.inFlight,f.disabled,async()=>assert.fail('overlap'),f.handlers),false);
  f.context.current++;done.resolve();await pending;
  assert.deepEqual(f.events,['start']);
  f.inFlight.current=false;f.disabled.current=true;
  assert.equal(await runStudyTask(f.context,f.inFlight,f.disabled,async()=>assert.fail('disabled'),f.handlers),false);
});
