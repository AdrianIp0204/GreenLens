/** Presentation identity only. Occurrence names are never identity, and an
 * inspection of an old mapping is independent of the active viewport pick. */
export type ReferenceOccurrenceSelection = { revision_sha256: string; occurrence_id: string };

export function sameReferenceSelection(a: ReferenceOccurrenceSelection | null | undefined, b: ReferenceOccurrenceSelection | null | undefined) {
  return Boolean(a && b && a.revision_sha256 === b.revision_sha256 && a.occurrence_id === b.occurrence_id);
}
