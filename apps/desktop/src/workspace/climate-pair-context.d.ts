import type { ClimateProjectDocument, ClimateRunSummary, CurrentClimateResult, ClimateRunRecord, ClimateValues, ResultField } from '../climate-types';
export function selectedPairProject(project: ClimateProjectDocument | null, a: ClimateRunSummary | null, b: ClimateRunSummary | null): ClimateProjectDocument | null;
export function selectedPairRelation(summary: ClimateRunSummary | null, condition: CurrentClimateResult | undefined, error?: string): string;
export function snapshotDisplayContext(run: ClimateRunRecord | null, request: {field: ResultField; interval: number}): {annual: boolean; field: ResultField; interval: number};
export function snapshotDisplayScale(field: ResultField, values: ClimateValues | null): number;
