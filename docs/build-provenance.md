# Build identity

GreenLens v0.2 stamps application provenance when Cargo builds the shared Rust runner. `scripts/dev-env.ps1` configures process-local tools and caches; it no longer caches Git identity. This fixes the v0.1 failure where a reused shell could keep an earlier commit or a false clean flag after editing or committing source.

`crates/greenlens-runner/build.rs` uses the dependency-free `scripts/build-provenance.rs` helper. Git is queried relative to the crate, using argument arrays, without optional index writes. The helper removes inherited Git directory/index overrides for those child commands, reads the actual HEAD, and checks tracked and untracked changes. Ignored caches do not count as source changes. Git is needed only at build time. If Git identity is unavailable, no identity is stamped; runtime provenance remains unknown rather than claiming a clean build.

The helper emits `GREENLENS_BUILD_SOURCE_COMMIT` and `GREENLENS_BUILD_SOURCE_DIRTY` to the compiler. Legacy shell variables `GREENLENS_SOURCE_COMMIT` and `GREENLENS_SOURCE_DIRTY` have no authority over these fields and are cleared by the development setup script.

The v0.3 repair also protects source archives against caller-supplied internal `GREENLENS_BUILD_SOURCE_*` variables. With verified Git, actual checkout identity replaces those values. Without Git identity, either inherited internal variable causes an actionable build error: remove both and rebuild to obtain unknown provenance. Explicit CI assertions remain the only supported caller assertions and must be checked against Git. Internal output variables are never accepted as archive identity.

## Refresh and CI assertions

A deliberately nonexistent `rerun-if-changed` input under Cargo's `OUT_DIR` makes Cargo rerun the small helper on every invocation. HEAD/index tracking alone misses newly created untracked files. Recursive repository tracking would repeatedly scan `node_modules`; the explicit refresh avoids that cost. This can recompile the small runner after an otherwise unchanged invocation, a deliberate correctness tradeoff. It does not fetch dependencies, mutate Git history or require a new shell.

CI may explicitly assert its expected identity using both variables:

```powershell
$env:GREENLENS_EXPECT_SOURCE_COMMIT = (git rev-parse HEAD).Trim()
$env:GREENLENS_EXPECT_SOURCE_DIRTY = 'false'
cargo build --locked -p greenlens-cli --release
```

These are checked assertions, not unchecked substitutes for the checkout. The full 40-character SHA and `true`/`false` state must match actual Git results; unavailable Git, mismatches, partial assertion pairs and malformed values fail the build. The workflow uses its checkout SHA and an explicit clean assertion. Both variables have Cargo environment-change tracking.

## Regression evidence

Run in a native development shell:

```powershell
. ./scripts/dev-env.ps1
./scripts/test-build-provenance.ps1
```

The test creates a fresh synthetic repository and Cargo probe under the local GreenLens cache. It compiles and runs the actual helper repeatedly using the same shell and target cache. It verifies clean state, tracked edits, restoring clean state, untracked files, a newly committed revision, stale legacy environment variables, matching CI assertions, wrong commit/dirty assertions, partial assertions and unknown identity for a source archive without Git metadata. Temporary commits use command-local synthetic author settings; the project history and global Git settings are untouched. A dated JSON summary is retained outside the source tree.

All eleven native compiled checks passed; the sanitized record is [v0.2-build-provenance-evidence.json](v0.2-build-provenance-evidence.json).

The later [v0.3 regression record](evidence/v0.3-integration/build-provenance-worktree.json) preserves a genuine failing archive probe followed by 20 passing compiled checks. These add linked-worktree HEAD/dirty identity, inherited Git directory/index overrides, verified Git replacing poisoned internal identity, each no-Git inherited-variable rejection and recovery to unknown after clearing them. The disposable repositories, source commits and test failure remain separate from real project history; all 11 touched process-environment variables are restored.

Application/package version `0.2.0` is separate from scientific model/project/run contracts. Existing v0.1 schemas, engine adapter protocol and historical immutable bytes remain readable and are not rewritten by version stamping.
