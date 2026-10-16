import type { ReferenceOccurrence } from './types';
export const OUTLINER_PAGE_SIZE: 100;
export type OccurrenceIndex = { byId: Map<string, ReferenceOccurrence>; children: Map<string, string[]>; roots: string[]; meshUses: Map<number, number> };
export function indexOccurrences(occurrences: ReferenceOccurrence[]): OccurrenceIndex;
export function occurrenceAncestors(index: OccurrenceIndex, id: string): ReferenceOccurrence[];
export function outlinerRows(index: OccurrenceIndex, query: string, collapsed: Set<string>): { occurrence: ReferenceOccurrence; depth: number; hasChildren: boolean }[];
