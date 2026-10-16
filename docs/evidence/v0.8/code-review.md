# v0.8 bounded code review

Reviewed the working implementation on 2026-10-16. This is a source review of the named paths, not installed-app, engine, physical-validation or general security acceptance. Final source-bound test and delivery receipts remain separate.

## Findings and implemented remedies

- `persistence.rs` / `report_product_delivery.rs`: direct writing to a final copy name could leave partial bytes after failure and could report cancellation after a successful copy. Delivery now writes an owned sibling temporary in 64 KiB chunks, verifies actual bytes through the same open handle, and checks cancellation before no-replace publication. A second review rejected pathname promotion: another actor could rename/replace the temporary after hashing. Windows now promotes the verified handle with `FileRenameInfo`, replacement disabled, and cleans a failed temporary through that handle. Checked parent-directory handles remain retained. Existing destinations and viewer copies are not overwritten. No cancellation error is returned after successful publication. Injected-checkpoint regressions cover cancellation during writing and hashing, destination and temporary-replacement races, wrong digest, exact-byte publication and absence of a late cancellation checkpoint. Execution of those regressions is recorded in the final test receipt, not inferred from review. The handle operation follows the documented [Windows rename contract](https://learn.microsoft.com/en-us/windows/win32/api/winbase/ns-winbase-file_rename_info).
- `capacity.rs` / project publication: the effective manifest limit includes the existing 16 MiB scope-history bound. Resource inspection uses actual member sizes, is explicitly not content-integrity verification, and cannot replace existing byte/hash checks. Cancellation reaches the publication check and final manifest-replacement fence. Capacity failure preserves the previous manifest and staged completed evidence. Isolated regressions cover manifest, entry, total-byte and per-entry bounds and unchanged prior data; metadata-only sparse fixtures are never accepted engine evidence.
- `study_revision.rs`: v2 typed provenance rejects aggregate, nonexistent, calendar-alias and study-schema-version pointers. The version string is not an applied scientific field. Legacy v1 interpretation and identity dispatch remain separate. Candidate design-scope changes require explicit review; they cannot silently reuse decisions that omit changed metadata.

## Confirmed boundaries

- Report delivery accepts only enumerated, verified retained PDF/PNG bytes. Save Copy requires a new destination. Windows receives a path, not an imported command recipe. A successful handoff is labeled as a request, with viewer display unobserved. Inline preview has separate 8 MiB/page/pixel bounds and retains the original full product for Save Copy.
- The viewer cache reuses only matching actual bytes, counts retained files and total bytes, and preserves existing copies at its limit. It does not evict files while an external viewer may depend on them.
- Capacity estimates expose uncertainty and exclude archive-envelope overhead from their start prediction. Exact final writer/reader limits remain authoritative; there is no automatic history pruning or limit increase.
- Scope/revision interpretation and report rendering stay versioned. Candidate cancellation or validation failure cannot authorize intermediate unresolved revisions, and incomplete provenance does not prohibit otherwise valid Unassessed execution.

## Outstanding observations

Native dialogs, external association behavior, visible PDF display, installed cancellation, and cold report regeneration require their own identified installed-candidate observations. No such acceptance follows from this source review. Private-source decoding and stopped scientific protocols are unchanged.
