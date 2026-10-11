import type { Model } from '../types';
export const COMMANDS:[string,string][];
export function parseCommand(text:string):{name:string;args:string[]}|null;
export function suggestions(prefix:string):[string,string][];
export function repeatable(name:string):boolean;
export function parseQuantity(text:string|number,kind?:'length'|'angle'|'lux'|'number'):number;
export function setParameter(model:Model,path:string,value:number):Model;
export function modelCommand(model:Model,selection:string[],name:string,args:string[]):{model:Model;target:string;label:string;prompt?:never}|{prompt:string;model?:never;target?:never;label?:never};
