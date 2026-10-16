# Legacy issue regeneration — 7c7090e

The clean release CLI declared at source `7c7090edba87fcd6167a0cf4650315483d3aa485`, SHA-256 `8d993882643776d78115b203241a2bfa9f050f5f776ba3e427335640fcb5d3f0`, regenerated the two original synthetic renderer/1 issues once each. This used the two previously unspent legacy allowances; the separate stopped new-issue collector remains incomplete. No simulation was requested or observed.

| Issue | Deterministic products | Product bytes | Regeneration wall time | Sampled peak memory | Sampled new scratch |
|---|---:|---:|---:|---:|---:|
| R1 `31855516…c893ad` | 44/44 exact | 15,918,865 | 40.5940 s | 152,621,056 B | 52,294,463 B |
| R2 `4d53b3f7…ac6337` | 44/44 exact | 15,823,232 | 22.8686 s | 125,767,680 B | 52,191,695 B |

All **88 product files (31,742,097 bytes)** were rehashed after regeneration and matched their retained manifests and the [earlier exact visual receipt](../v0.7/v0.7-report-visual-coverage-5654b9e.md). Its **146 unique PDF pages and 16 performance PNG reviews** therefore apply through exact-byte reuse; this is not a claim of a new page review. The old renderer, template and font identities remain unchanged.

Inspection commands took 41.2795 s and 13.3096 s. All four commands exited zero. The maximum observed tree had four processes; no EnergyPlus child was observed. The pinned Typst logical and Windows-redirected physical paths were verified through held handles as the same file with the audited executable hash. In the second regeneration, one Typst child and two console hosts had already exited before image lookup; their image identities remain explicitly unobserved. Process and resource sampling is approximately 400 ms, not an absolute instrumented peak.

Final checks found **521 original-source members and 687 legacy-source members unchanged**. Each regenerated directory contains 44 deterministic products plus its runtime receipt and remains within the shared archive limits. The original failed collector's new-issue status was not changed to complete; its legacy allowance ledger now records both consumed attempts.

[Machine-readable receipt](legacy-regeneration-7c7090e.json) records exact issue/content/product identities, every PDF product digest, all command receipt digests, renderer/font identities and measured limits. This check establishes legacy deterministic regeneration and byte preservation only. New renderer/2 issuance, installed interaction, final-source CI, physical qualification, clean-machine installation and the blocked private benchmark have separate status.
