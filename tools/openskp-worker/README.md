# Optional OpenSKP development worker

**Not adopted, built, installed, or exposed by GreenLens by this source addition.** This is a bounded development candidate for the predeclared matrix in `docs/v0.8-skp-decision.md`. Successful exit means an *unaccepted candidate*, never accepted project geometry, complete source fidelity or scientific qualification. No private input is authorized by this recipe.

Source pin: OpenSKP `dceb9fff0efc9668961530b535d5375c001343dc`; miniz `77d0dce8627735138c51770d1799a1ef48f2117d`; TinyGLTF `488a70a3df62a4df1a736e9e56fb8836580c4888` including nlohmann JSON 3.10.4. Exact notices are retained in `licenses/`. No SketchUp SDK, FlatBuffers/Fragments, image decoder, interpreter, online converter or application runtime dependency is introduced. MSVC runtime distribution remains separately reviewed if anything is later packaged.

`prepare-source.mjs` checks those external Git pins/clean tracked content, copies only the selected source closure to a **new** external directory and applies unique-anchor strict patches. It leaves failed preparations inspectable. It never downloads, builds, parses a model or mutates an upstream checkout. Required Node.js is development tooling only. CMake checks every prepared source hash, has no acquisition or install target, and builds only the static parser/geometry/instanced GLB closure plus the Windows worker. Final linked closure still needs the build owner's check.

The build owner supplies paths; no local settings belong in Git:

```powershell
node tools/openskp-worker/prepare-source.mjs $OpenSkpCheckout $MinizCheckout $TinyGltfCheckout $NewExternalPreparedTree
cmake -S tools/openskp-worker -B $NewExternalBuild -G Ninja "-DGREENLENS_PREPARED_SOURCE=$NewExternalPreparedTree" -DCMAKE_BUILD_TYPE=Release
cmake --build $NewExternalBuild --parallel 2
```

Run **only through the owner's supervised process service**, never an unconstrained shell invocation. Fixed argument array: `[input_skp, new_empty_stage, expected_lowercase_source_sha256]`. Worker verifies a containing Windows Job Object with kill-on-close and a hard aggregate committed-memory limit of at most 1 GiB. Parent must also enforce the 120-second deadline, one-worker process tree, 256 MiB scratch and bounded pipes, kill/reap on cancellation, retain failure evidence and independently fence source/output bytes. The worker's cooperative checks do not replace the hard job/deadline.

Outputs are only `candidate.glb`, `metadata.json`, and last-written `completion.json`. All are new files in a retained empty stage. No completion file means incomplete. Original input is held read-only with write/delete sharing denied and hashed before/after. Output contains no source path or command. It includes model names/metadata and therefore stays private for private input. No accepted manifest or project is written. Independent Rust GLB admission plus the exact fixture oracle are required after successful exit.

The candidate recognizes only actual encoded `{13.0.1}` and `{26.2.243}` for the declared independently authored matrix. It preserves transforms and all hidden geometry, returns occurrence identity as source-hash-bound preorder index/path, and separately records source hidden/tag state. Bounds are measured from every transformed exported vertex, not upstream transformed AABB corners. Source unit display may be unknown; storage inches and explicit `(X,Z,-Y)` metre conversion are recorded. Unknown north is not inferred.

Strict patches turn detected malformed ranges, incomplete list/prefix fallbacks, missing references, empty/stalled triangulation and overlapping-hole approximation into fatal failures. Image pixels are skipped before extraction/allocation. Raw validation rejects missing/degenerate topology, missing transforms, cycles, billboard/image definitions, loose edges and detected unsupported drawing primitives. A conservative two-sided triangle upper bound is checked before scene building; exact expanded count is checked after. This deliberately bounded surface subset may reject otherwise valid files. **Do not relax it after a failed independent fixture.** Unknown source structures are not proved complete merely by these checks; no production or private adoption follows until the independent matrix and containment gates pass.

Metadata keeps all source face loop/visibility inventory and node paths within 4 MiB. Texture/watermark pixels, application saved views/styles and arbitrary property dictionaries are explicitly omitted. Imported visual materials never imply thermal properties. The source parser is not a complete sandbox or CAD kernel. All prior failed parser/application/science protocols remain retained.

Source-only preparation regressions (no converter/model execution):

```powershell
node --test tools/openskp-worker/patches.test.mjs
```

The required audited source must exist; tests fail rather than skip when absent. These tests establish only deterministic strict patch application and static guard presence. They do not establish native compilation, resource containment, independent geometry fidelity or private model recovery.

`matrix-oracle.mjs` is a separate source-only collector. It never starts a worker.
Its `prepare SIMPLE_DIR COMPLEX_DIR NEW_EXTERNAL_DIR` command would retain the
fixed 20 inputs and their original-byte hashes; `inspect MATRIX_JSON SLOT STAGE
OBSERVED_EXIT` would inspect retained worker output. Neither command establishes
process supervision or replaces independent Rust admission. The simple native
receipt establishes only one face, four edges and 2 m²; its missing coordinate
oracle is disclosed. The complete original authoring fixture supplies the
independent geometry/placement/hole/visibility checks. All source expectations
must be declared before any candidate parser execution.

```powershell
node --test tools/openskp-worker/matrix-oracle.test.mjs
```

These tests use original in-memory bytes, not an SKP parser. The native strict
containment peak criterion has failed in the independent discriminator; **do not
execute preparation, worker builds or conversions as an implicit continuation**.
See the retained stop in `docs/v0.8-skp-decision.md`. The collector and worker
remain unexecuted development candidates; the private probe remains unused.
