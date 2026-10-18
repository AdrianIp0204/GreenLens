# RC2 stopped native reporting observation — 2026-10-18

RC2 is **consumed and failed**. The installed application reached the armed report stage and launched the pinned Typst executable, but its observed physical image path was a package-virtualized alias outside the prospectively declared exact path policy. The observer stopped the owned process tree. No R2 was accepted and the conditional cold regeneration was not attempted. This is an observer identity-policy failure, not evidence that Typst bytes were altered, that the report renderer completed, or that the report was physically validated.

## Exact identities and prerequisites

| Item | Identity |
|---|---|
| Application source | `f2197f39eb845515fb48d5f5e1616ea529e4f031` |
| Observer source | `0df5254dd44a124b62f4fc2125f4f4cf304dc93d` |
| Local unsigned installer SHA-256 | `5cb262035975b0f792d38fc56627f94ca5fe14c10eb833b580cf7c7953e88d42` |
| Installed application SHA-256 | `c1b430ad4d60abfcb5406bde3db95548aee0935eab175918038325a0786441c8` |
| Installed application bytes | 33,374,720 |
| Pinned and observed Typst SHA-256 | `081217a463adb006f8894b44227fe4b9c9e91fc85f5463d5948e8370db9bb31e` |
| Prospective declaration SHA-256 | `a49d8502e8debc5c4ddb15acdd1d4c23a540ee88453934863fc12ed839e38c04` |
| Runtime allowlist SHA-256 | `ada6d02d29e5f64eadd2b6a36a9fb9a13c1056d46bff37fe4f47601899e2213c` |
| Observer protocol SHA-256 | `fd0ea536993f2faa43a9554bf4d571aa1012c7224f6e78002ad0955fddaa6d9b` |
| Complete observer receipt SHA-256 | `8a6826521141ef01dce1ab00ada618cc80aafb7a5c698d61eadfd6b5ef01f16c` |
| Post-failure preservation audit SHA-256 | `458aab4179b88dd39a9cde3f2685a5e11acba655943d459dda748b0322b713ba` |

The local installation completed with exit code zero on the existing workstation. This was not a clean-machine installation. The installed/unbundled byte comparison recorded only the expected NSIS bundle-marker change; local installed identities above are distinct from hosted artifact hashes.

Both required hosted prerequisites passed before launch: [application CI 39104602643](https://github.com/AdrianIp0204/GreenLens/actions/runs/39104602643) on `f2197f39` passed build, energy and static compatibility; [observer CI 39108158299](https://github.com/AdrianIp0204/GreenLens/actions/runs/39108158299) on `0df5254d` passed its helper, harmless-process, collector-source and documentation checks. The [final CI receipt](f2197f3-ci-final.json) retains exact jobs, test/ignore disclosures and artifact identities. Hosted success is not installed reporting acceptance, and observer CI did not run GreenLens, Typst or EnergyPlus workflows.

## Observed failure and resource scope

The observation was consumed by one launch at 07:57:42 UTC. Rendering was armed at 08:06:18.8719457 UTC. A held process handle identified the Typst child, including kernel image path and SHA-256, at 08:08:00.2494081 UTC. Its digest matched the pinned executable exactly. The prospectively declared observer location and the observed package-virtualized physical location differed; the latter had not been admitted by the exact prospective policy. This does not establish the app's effective saved/default setting. The observer did not substitute a configured path, waive the mismatch, or retrospectively accept the child.

The stopped receipt completed at 08:08:01.3644028 UTC. Its 617.1219286 seconds include startup, navigation and preparation; this is not render duration. The armed-to-stop interval was approximately 102.492457 seconds. The first rejected Typst process did not establish successful rendering or publication.

| Measurement or limit | Result |
|---|---:|
| Completed observation samples | 2,813 |
| Raw owned-Job committed-memory peak | 1,032,003,584 bytes |
| Sampled aggregate private-memory maximum | 995,708,928 bytes |
| Maximum observed active processes | 8 |
| Fixed kernel Job guard | 1,073,741,824 bytes |
| Fixed raw aggregate committed-memory acceptance ceiling | 2,147,483,648 bytes |
| Declared render deadline | 600 seconds per render |
| Declared process maximum | 16 |
| Observed EnergyPlus execution | None |

These are measurements of the stopped RC2 observation only. They do not prove that a complete render/regeneration fits either memory limit, establish general process-memory containment, or close the exhausted live EnergyPlus cancellation requirement. Sampled private memory is not substituted for raw Job commitment.

Owned-Job forced termination completed and the Job was empty. All eight retained member handles were closed; zero remained. Standard-output and standard-error capture contained zero bytes. Cleanup targeted the owned Job; this is not an independent census of unrelated processes.

## Immutable evidence preserved

The read-only post-stop audit matched all **580 original members and 225,550,938 bytes**, with no changed or missing original member. The manifest SHA-256 remains `0704b72c5153be55fba1e5c05a519494191bde0371dbb9f68eec34de45451201`. R1's 52 members and all four scope-history members remain intact. There is still exactly one accepted issue; no R2 directory or accepted issue was added. The only new project member is a zero-byte runtime lock, excluded from immutable/export inventories.

The selected retained revision remains `e275d8664b1d222803272a9e8f0d9f5c9b942513ceacae016fce445934948e3a`, using scope-revision v3/history-storage v2. The selected genuine A/B record digests remain `85f2beded5c6278cf3e783eca939c930b2e4a6348c3c1ef253c82b9899dfac44` and `5c1f9539145886f041ed5eef18e1c62bda37cd26435361a19afdea833fa7481b`. No simulation was requested by RC2.

Incomplete staging remains diagnostics only: 41 observed files totaling 13,204,986 bytes. Presence and lengths do not establish product integrity, successful rendering, a complete issue or permission to promote these files. Ordinary and virtualized location views are not claimed to be separate physical copies. The pre-guard draft backup remains byte-identical to its receipt; it is not claimed to be a transactionally consistent live database or an accepted revision recovery.

## Remaining boundary

R2 issuance, changed-page/figure review, complete portable Save As, new-issue cold inspection and exact cold regeneration remain unverified for this observation. RC2 cannot be retried automatically, and its failed outcome is not erased by a later source or configuration repair.

The smallest prospective correction is to bind the actual held physical Typst image location and its pinned hash before any separately authorized observation, retaining exact policy checks and unchanged limits. At this checkpoint VD2 is a separate unspent allowance with that prospective path correction under review; no VD2 result or RC2 retry is asserted here. CT1 remains stopped, and the independent OpenSKP matrix and private probe remain unused. Raw receipts, location details and diagnostics are retained externally; this public record contains no machine paths or private-case content.
