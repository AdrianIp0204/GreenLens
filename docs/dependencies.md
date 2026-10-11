# Dependencies, notices and redistribution

Original GreenLens code and synthetic examples are Apache-2.0. See `LICENSE` and `NOTICE`. Third-party terms continue to apply to their own code and assets. The project does not treat a subprocess boundary as a licensing exemption.

## Reproducible inventory

After the native toolchain is established and both lockfiles are current:

```powershell
. ./scripts/dev-env.ps1
cargo fetch --locked
Push-Location apps/desktop
npm ci
Pop-Location
node scripts/license-inventory.mjs --strict
```

The generator reads Cargo's resolved Windows dependency graph and npm's committed lockfile, then inspects the installed package metadata and actual license/notice files. It writes:

- `dependencies/resolved-inventory.json`: exact versions, source URLs, registry archive checksums/integrity, license declarations, evidence-file hashes, distribution scope, modifications and review gaps.
- `third-party/generated-notices.txt`: retained license and attribution text from the resolved packages, deduplicated by the hash of the original file bytes.

No user names, absolute cache paths, credentials or environment blocks are written into these artifacts. An inventory generated with `--npm-only` is explicitly partial and fails `--strict`. A missing license declaration or retained notice is a review gap, never an assumed permissive grant. Platform-specific optional npm packages absent on Windows are listed as uninstalled and are not represented as shipped binaries. Rust graph entries include build/test tools; the inventory deliberately covers more than one final executable.

Some published Rust subcrate archives omit the workspace license text. `dependencies/license-supplements.json` records the exact source commit from each archive's `.cargo_vcs_info.json`, the upstream notice URL, retained text digest and reviewed license choice. These texts are retained under `third-party/upstream` and merged into the generated notices. The generator verifies the recorded source commit and notice hashes rather than silently inferring a license. For `selectors`, published source files carry the MPL-2.0 notice; the complete canonical Mozilla license is retained separately. The declared CC0-1.0 alternative is selected for `dunce`, and the MIT alternative for `defmt-parser`.

The linked Rust standard library is outside `Cargo.lock`. Its exact compiler release, selected native library archive hashes and complete release `COPYRIGHT-library.html` are included separately in the generated inventory and notices. That upstream document contains its own incorporated-dependency details. Two native npm build bindings publish only a binary; for the Windows Tauri CLI and Rolldown bindings, the generator verifies the exact-version parent package's optional-dependency mapping and matching license declaration, then retains that companion package's license and bundled third-party notice text. The source package is identified explicitly for that evidence.

The vendored Tauri 2.12.1 NSIS template is the documented exception to unmodified dependency sources: its current-user install destination is adjusted and a provenance header is added. The original source commit/digest, modified file digest, selected MIT license notice and modification description are recorded separately. See `docs/windows-installer.md`. The generator fails if the reviewed template digest changes before its review is updated.

Regenerate after changing either lockfile or build features. Compare the recorded lockfile SHA-256 values with the final release checkout. Retain `LICENSE`, `NOTICE`, the dependency inventory and third-party notices with released desktop/CLI artifacts. The package configuration and release script must include those files; a source-only inventory does not by itself retain notices in an installer.

After the first NSIS bundle build downloads its tool cache, also verify the actual compiler/stub/plugin bytes before assembling final artifacts:

```powershell
node scripts/license-inventory.mjs --strict --installer-cache "$env:LOCALAPPDATA/tauri/NSIS"
```

This comparison uses the SHA-256 values of the reviewed Tauri-pinned NSIS 3.11 archive and `nsis-tauri-utils` 0.5.3 release binary. The inventory distinguishes a reviewed upstream archive from a cache that has actually been verified on the packaging machine.

Use `--strict --check` in CI and before building from a committed checkout. It verifies the resolved inventory and exact notice text without modifying either file. The comparison excludes only generation time and installer cache observation flags; package versions, lockfile hashes, notices, license choices and reviewed binary hashes must match. Add `--installer-cache <directory>` after bundling to verify all nine actual compiler/stub/plugin hashes without making the checkout dirty. Normal generation without `--check` records the current timestamp and cache observations for a reviewed source update.

## Conditions tracked by the review

The exact retained license text controls. The inventory keeps AND/OR expressions intact, records whether upstream sources were modified, and records exact package-source download URLs. It flags expressions outside the reviewed policy for additional inspection.

MIT, BSD and ISC notices are retained with the distribution. Apache-2.0 dependencies also require preservation of applicable NOTICE material and identification of modifications. Apache-2.0 provides no general trademark permission. See the authoritative [Apache License 2.0 redistribution conditions](https://www.apache.org/licenses/LICENSE-2.0).

Where the resolved graph includes MPL-2.0 code, its covered source must remain available under MPL with a source-access notice to executable recipients. The inventory identifies exact unmodified source archives and their integrity digests; distribute that inventory and the full license with the artifact. Re-audit if covered files are modified. See [Mozilla Public License 2.0](https://www.mozilla.org/en-US/MPL/2.0/), especially sections 3.1 and 3.2.

Package metadata alone cannot establish that all embedded material has the same license. The script retains bundled license directories and notices in addition to top-level files; any unexplained nested third-party material requires review before release. Code license grants do not grant Tauri, Three.js or other project branding rights. GreenLens uses its own name and visual identity and does not imply endorsement.

## Scientific engine and native runtime

`dependencies/radiance.json` separately pins the official Radiance 6.0.2 Windows asset, source tag, archive digest, executable hashes and selected library/runtime review. `third-party/Radiance-LICENSE.txt` preserves the source-tag license. The upstream ZIP lacks a complete bundled-library notice set. GreenLens therefore acquires the entire unchanged engine archive separately from upstream and does not copy engine binaries or its Qt, GCC, Perl or Microsoft runtime components into the application installer or evidence export. Do not reinterpret acquisition from upstream as permission to rebundle its contents.

The actual workflow uses `gensky`, `oconv`, `rtrace`, their required calculation libraries and native runtime dependencies. The engine manifest distinguishes the required components from unused archive contents and records unresolved packaging grants. Any future engine bundling needs a new review of each included library and its notices.

WebView2 is an external Microsoft runtime prerequisite and is not an Apache-2.0 GreenLens component. The release must state whether a bootstrapper is used or an installed runtime is required; follow the applicable Microsoft terms for that acquisition route. Build-only tools such as Visual Studio, Rust, Node.js, npm and NSIS have their own licenses and installation terms. The recorded build-tool versions and native installation route belong in `docs/toolchain.md` and release verification evidence; they are not simulation dependencies during ordinary calculation.

The Windows MSVC build also uses the Microsoft C/C++ runtime dynamically. The compatible Microsoft Visual C++ v14 x64 Redistributable is acquired separately from Microsoft under its own terms; GreenLens does not redistribute its DLLs. The installed runtime must be at least as recent as the release build's toolset. Native smoke-build imports include VCRUNTIME140.dll and Universal CRT API sets; final CLI/desktop import inspection is recorded separately in `docs/windows-runtime.md`. An installed development toolchain is not evidence that a clean Windows machine already has these runtime components.

The WebView2 **loader** is a separate component from the external browser runtime. `webview2-com-sys` 0.39.1 includes Microsoft.Web.WebView2 SDK 1.0.3800.47 and statically links its x64 loader into the MSVC desktop executable. Its three x64 loader artifacts were compared byte-for-byte by SHA-256 against the [official versioned NuGet package](https://www.nuget.org/packages/Microsoft.Web.WebView2/1.0.3800.47). That SDK's actual license is BSD-3-Clause; its complete LICENSE and NOTICE (including the SDK's ANTLR/StringTemplate notices) are retained. The inventory records the package digest and loader digests separately from the MIT Rust wrapper. See the [versioned SDK license](https://www.nuget.org/packages/Microsoft.Web.WebView2/1.0.3800.47/License) and Microsoft's [loader distribution guidance](https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/distribution#files-to-ship-with-the-app). No SDK binaries are checked into GreenLens source.

The installer contains NSIS 3.11 stubs, UI material and selected plugins. Its complete COPYING and Modern UI/NSISdl notices are retained, including the LZMA module's CPL-1.0 license and explicit linking exception. GreenLens has not modified those components. Unmodified NSIS/LZMA source code is available from the [NSIS 3.11 source archive](https://sourceforge.net/projects/nsis/files/NSIS%203/3.11/nsis-3.11-src.tar.bz2/download); it remains under the licenses in COPYING. The linking exception permits separately licensed application code and does not change the CPL terms for the LZMA files. All upstream disclaimers remain in the retained text; GreenLens offers no separate warranties on behalf of those contributors.

The additional `nsis-tauri-utils` 0.5.3 helper is retained under its MIT alternative. Its release binary digest and [exact source commit](https://github.com/tauri-apps/nsis-tauri-utils/tree/13d9edd27b69310e108d6fbd49f90992f8a05390) are recorded. The helper uses `no_std` Rust; upstream does not publish an exact compiler/lockfile record for that binary, so this inventory does not claim a byte-identical source rebuild of the helper. It remains a pinned upstream installer component.

The 11 October 2026 full native Windows inventory check completed with 357 entries, 195 distinct retained notice texts and zero automated review gaps. All nine actual NSIS cache compiler/stub/plugin hashes matched the reviewed components. A subsequent `--strict --check --installer-cache` pass left the inventory and notice bytes unchanged. Confirming that final installed/distributed artifacts retain these documents remains a separate packaging check.

The v0.2 workspace adds the Windows-only `rfd` 0.17.2 native dialog wrapper. Its actual registry package retains the MIT license and copyright notice for Bartłomiej Maryńczak; GreenLens has not modified it. The refreshed native Windows inventory contains 358 packages and 196 distinct retained notice texts, with zero automated review gaps and the same nine verified installer-component hashes. The new package and notice are included in the generated inventory and notices; final v0.2 package retention remains a separate release check.

## Data and excluded packages

The two bundled room models and analytical fixtures are original synthetic data. No weather file, proprietary assessment manual, copyrighted benchmark model, personal model or regional certification dataset is included. The release requires no Honeybee, IfcOpenShell, EnergyPlus or OpenStudio runtime. Future adapters or assessment modules require their own source, data and redistribution review.

The automated inventory reports unresolved facts explicitly. Completion requires review of those gaps and confirmation that the final distributed package retains the required texts; running the generator is not a claim that every legal obligation has already been satisfied.
