export function energyShortcut(event:{key:string;altKey?:boolean;ctrlKey?:boolean;metaKey?:boolean;shiftKey?:boolean}):'new'|'open'|'save'|'save_as'|'undo'|'redo'|null;
export function selectedEnergyRun(runs:{id:string;alternative_id:string}[],explicit:string,defaults:Record<string,string>|null,alternative:string):string;
export function energyRowStart(text:string,intervalCount:number):number;
