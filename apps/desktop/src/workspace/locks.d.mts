import type { Model, ValidatedModel } from '../types';
import type { ViewportDisplayState } from '../viewport/types';
export function editLockReason(before:Model,after:Model,display:ViewportDisplayState):string|null;
export function validatedEditLockReason(before:ValidatedModel,after:ValidatedModel,display:ViewportDisplayState):string|null;
