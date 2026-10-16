import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import {fileURLToPath} from 'node:url';
import {transformWithOxc} from 'vite';
const file=fileURLToPath(new URL('../src/energy/bounded-read.ts',import.meta.url));
const {code}=await transformWithOxc(fs.readFileSync(file,'utf8'),file);
const {BoundedReadQueue,MAX_QUEUED_LARGE_READS}=await import('data:text/javascript;base64,'+Buffer.from(code).toString('base64'));
const deferred=()=>{let resolve,reject;const promise=new Promise((yes,no)=>{resolve=yes;reject=no;});return{promise,resolve,reject};};
const turn=()=>new Promise(resolve=>setImmediate(resolve));

test('large reads are FIFO with one unsettled invocation, including heterogeneous reference/energy work',async()=>{
  const queue=new BoundedReadQueue(),gates=[deferred(),deferred(),deferred()],seen=[];let active=0,peak=0;
  const requests=['manifest','scope context','energy defaults'].map((name,i)=>queue.enqueue(async()=>{seen.push(name);active++;peak=Math.max(peak,active);try{return await gates[i].promise;}finally{active--;}}));
  await turn();assert.deepEqual(seen,['manifest']);gates[0].resolve('m');assert.equal(await requests[0].promise,'m');await turn();assert.deepEqual(seen,['manifest','scope context']);
  gates[1].resolve('c');assert.equal(await requests[1].promise,'c');await turn();gates[2].resolve('e');assert.equal(await requests[2].promise,'e');assert.equal(peak,1);assert.equal(active,0);
});

test('cancelled queued work never invokes; active cancellation keeps the lane until the actual promise settles',async()=>{
  const queue=new BoundedReadQueue(),gate=deferred(),seen=[];
  const active=queue.enqueue(()=>{seen.push('active');return gate.promise;}),discarded=queue.enqueue(async()=>{seen.push('discarded');return 2;}),last=queue.enqueue(async()=>{seen.push('last');return 3;});
  const activeRejected=assert.rejects(active.promise,/cancelled/),discardedRejected=assert.rejects(discarded.promise,/cancelled/);
  await turn();discarded.cancel();active.cancel();active.cancel();await discardedRejected;await turn();assert.deepEqual(seen,['active']);
  gate.resolve(1);await activeRejected;assert.equal(await last.promise,3);assert.deepEqual(seen,['active','last']);
});

test('cancel before dispatch invokes nothing and releases the selected lane',async()=>{
  const queue=new BoundedReadQueue();let count=0;
  const first=queue.enqueue(async()=>++count),rejected=assert.rejects(first.promise,/cancelled/);first.cancel();await rejected;
  assert.equal(count,0);assert.equal(await queue.enqueue(async()=>++count).promise,1);
});

test('queue capacity fails explicitly without pruning, retry or parallel fallback',async()=>{
  const queue=new BoundedReadQueue(),gate=deferred(),seen=[];
  const active=queue.enqueue(()=>gate.promise),waiting=Array.from({length:MAX_QUEUED_LARGE_READS},(_,i)=>queue.enqueue(async()=>{seen.push(i);return i;}));
  const overflow=queue.enqueue(async()=>{throw Error('must never run');});await assert.rejects(overflow.promise,/queue is full/);assert.deepEqual(seen,[]);
  gate.resolve('done');assert.equal(await active.promise,'done');assert.deepEqual(await Promise.all(waiting.map(task=>task.promise)),Array.from({length:16},(_,i)=>i));assert.deepEqual(seen,Array.from({length:16},(_,i)=>i));
});

test('rejection including null/undefined and synchronous failure releases the lane exactly once',async()=>{
  const queue=new BoundedReadQueue();
  for(const reason of [null,undefined,Error('native failure')]){
    const failed=queue.enqueue(()=>Promise.reject(reason));let rejected=false;await failed.promise.then(()=>assert.fail('must reject'),value=>{rejected=true;assert.equal(value,reason);});assert.equal(rejected,true);
    assert.equal(await queue.enqueue(async()=>7).promise,7);
  }
  await assert.rejects(queue.enqueue(()=>{throw Error('synchronous');}).promise,/synchronous/);
  assert.equal(await queue.enqueue(async()=>9).promise,9);
});
