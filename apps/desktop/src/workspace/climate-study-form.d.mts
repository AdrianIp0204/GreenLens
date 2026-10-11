import type { NumericalProfile, Occupancy, Study, StudyMode, WeatherBinding, WeatherReference, WeatherIntervalView } from '../climate-types';
export interface StudyDraft {
  mode: StudyMode['mode']; weatherId: string; profile: NumericalProfile; interval: string;
  da: string; lower: string; upper: string; occupancyKind: Occupancy['kind']; start: string; end: string; mask: string;
}
export function minuteText(value: number): string;
export function parseMinute(text: string): number;
export function sameWeather(a: WeatherBinding | null | undefined, b: WeatherBinding | null | undefined): boolean;
export function draftFromStudy(study: Study, weather: WeatherReference[]): StudyDraft;
export function studyFromDraft(base: Study, draft: StudyDraft, weather: WeatherReference[]): Study;
export function fileLabel(path: string): string;
export function matchingInterval(view: WeatherIntervalView | null, weather: WeatherReference | undefined, indexText: string): WeatherIntervalView | null;
