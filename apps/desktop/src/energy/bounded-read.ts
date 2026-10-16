/** Participating background energy/reference reads share one transport lane.
 * Explicit authoring/review actions are outside this queue. Scheduling changes
 * no Rust validation or actual-byte check, and caches no result or identity. */
export const MAX_QUEUED_LARGE_READS=16;
export type BoundedRead<T>={promise:Promise<T>;cancel:()=>void};
type Entry={phase:'queued'|'active'|'settled';cancelled:boolean;run:(()=>Promise<unknown>)|null;resolve:(value:unknown)=>void;reject:(reason:unknown)=>void};
const cancelled=()=>new Error('Evidence read cancelled; its response is not available.');

export class BoundedReadQueue {
  private active:Entry|null=null;
  private waiting:Entry[]=[];
  constructor(private readonly maximumWaiting=MAX_QUEUED_LARGE_READS){
    if(!Number.isSafeInteger(maximumWaiting)||maximumWaiting<1||maximumWaiting>64)throw Error('Read queue capacity must be between 1 and 64.');
  }
  enqueue<T>(run:()=>Promise<T>):BoundedRead<T>{
    if(this.active&&this.waiting.length>=this.maximumWaiting)return{
      promise:Promise.reject(new Error(`Evidence read queue is full (${this.maximumWaiting} waiting). Finish or cancel an existing read, then explicitly retry.`)),cancel:()=>{},
    };
    let resolve!:(value:T|PromiseLike<T>)=>void,reject!:(reason:unknown)=>void;
    const promise=new Promise<T>((yes,no)=>{resolve=yes;reject=no;});
    const entry:Entry={phase:'queued',cancelled:false,run,resolve:value=>resolve(value as T),reject};
    this.waiting.push(entry);this.startNext();
    return{promise,cancel:()=>{
      if(entry.phase==='settled'||entry.cancelled)return;
      entry.cancelled=true;
      if(entry.phase==='queued'){
        this.waiting=this.waiting.filter(item=>item!==entry);
        entry.phase='settled';entry.run=null;entry.reject(cancelled());
      }
      // An already-started native operation still owns this lane until its
      // actual promise settles. Its caller requests native cancellation and
      // hides late replies; cancellation is not permission to overlap work.
    }};
  }
  private startNext(){
    if(this.active)return;
    const entry=this.waiting.shift();if(!entry)return;
    this.active=entry;entry.phase='active';
    void Promise.resolve().then(()=>{
      if(entry.cancelled)throw cancelled();
      return entry.run!();
    }).then(value=>this.finish(entry,true,value),error=>this.finish(entry,false,error));
  }
  private finish(entry:Entry,succeeded:boolean,value:unknown){
    entry.phase='settled';entry.run=null;
    if(this.active===entry)this.active=null;
    if(!succeeded)entry.reject(value);else if(entry.cancelled)entry.reject(cancelled());else entry.resolve(value);
    this.startNext();
  }
}

/** Shared by Project-bearing useEnergyRead queries and background reference
 * manifest/context/assessment/chunk reads, not all desktop commands. */
export const largeReadQueue=new BoundedReadQueue();
