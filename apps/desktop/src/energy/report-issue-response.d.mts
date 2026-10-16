import type {ReportIssueInspection,ReportIssueSummary} from './report-issues';
export function validIssueSummary(value:unknown):value is ReportIssueSummary;
export function assertIssueInspection(request:Record<string,unknown>,value:unknown):ReportIssueInspection;
