# Windows runtime requirements

The Windows x64 build uses Rust's native MSVC target and dynamic Microsoft C runtime linkage. The desktop explicitly sets `build.windows.staticVCRuntime: false` in `tauri.conf.json`; this disables Tauri's default static VC support-library override so its runtime acquisition route agrees with the CLI. GreenLens does not copy Microsoft runtime DLLs out of Visual Studio or the Radiance archive. Install the [Microsoft Visual C++ v14 x64 Redistributable](https://learn.microsoft.com/en-us/cpp/windows/latest-supported-vc-redist) separately from Microsoft, alongside the [Microsoft Edge WebView2 Evergreen Runtime](https://developer.microsoft.com/en-us/microsoft-edge/webview2/). These components retain their own Microsoft terms and are not GreenLens Apache-2.0 components. The GreenLens per-user installer does not install either prerequisite.

Microsoft's supported rule is that the installed redistributable must be at least as recent as the MSVC Build Tools used to produce an application. Use the latest supported v14 x64 package on a supported Windows release. The inspected development toolset is 14.50.35725.0. The installed workstation runtime was VCRUNTIME140.dll 14.50.35719.0 and UCRT 10.0.26100.8875; successful tests on that workstation establish only observed behavior, not a general minimum redistributable version or clean-machine acceptance. Windows supplies the UCRT system component on supported Windows 10/11 installations.

The CLI does not require WebView2. The desktop does; configuring Tauri to skip WebView2 installation does not remove this requirement. Radiance is acquired separately, and its adjacent bundled runtimes apply to engine executables, not to the GreenLens application installed in another directory.

The WebView2 SDK loader is a separate component from the WebView2 browser runtime. The pinned `webview2-com-sys` crate links the reviewed Microsoft WebView2 SDK loader into the desktop. Its BSD-3-Clause license and notices are retained as documented in `dependencies.md`. The statement about separately acquired runtimes does not mean that no Microsoft-authored loader code is linked.

## Inspection method and evidence

Run this from the checkout after producing both release executables:

```powershell
. ./scripts/dev-env.ps1
dumpbin /dependents "$env:CARGO_TARGET_DIR/release/greenlens.exe"
dumpbin /dependents "$env:CARGO_TARGET_DIR/release/greenlens-desktop.exe"
```

Inspect the exact installed desktop copy as well, recording its SHA-256 and comparing its bytes with the packaged build. Tauri deliberately changes its bundle-type marker from `__TAURI_BUNDLE_TYPE_VAR_UNK` to `__TAURI_BUNDLE_TYPE_VAR_NSS` for NSIS, so the installed executable and unbundled executable have different hashes. Confirm that any difference is explained, rather than silently assuming equality. Record imported libraries; do not assume a successful developer-machine launch proves runtime portability. Dynamically loaded WebView2 components also require the runtime even if they are absent from this static import listing.

| Artifact | Evidence | Status |
| --- | --- | --- |
| Rust native standard-library smoke | `rustc --print cfg --target x86_64-pc-windows-msvc` contains no `crt-static`. `dumpbin /dependents` reports VCRUNTIME140.dll, six `api-ms-win-crt-*` libraries and Windows system libraries. | Verified 2026-10-11; toolchain evidence only. |
| Provisional desktop before explicit configuration | Imported UCRT and Windows libraries without VCRUNTIME140.dll. Inspection of pinned `tauri-build` 2.7.1 showed its default static VC support-library override. | Diagnostic only; prompted explicit `staticVCRuntime: false` and requires a new artifact check. |
| Release CLI | `dumpbin /dependents` confirms VCRUNTIME140.dll and Windows/UCRT imports. SHA-256 `cc9d7a228cfe4cfd7f386d9f8f66346edea4e7d68b9f08b54ad717795b826dc8`. | Verified 2026-10-11. |
| Installed desktop | `dumpbin /dependents` confirms VCRUNTIME140.dll, VCRUNTIME140_1.dll and Windows/UCRT imports. Installed SHA-256 `66196ab72212617a2232043b8dd71e2cacd4ef3dc4d357d46385c41f9fc7986d`. Native application launch succeeded using installed WebView2. | Verified 2026-10-11; workflow evidence is recorded separately in `native-verification.md`. |
| Clean machine without developer tools | Not tested on a separate clean Windows installation. | Unverified; installation route documented. |

The [Rust linkage reference](https://doc.rust-lang.org/reference/linkage.html#static-and-dynamic-c-runtimes) documents `crt-static` and recommends checking the resulting executable. [Tauri's Windows installer documentation](https://v2.tauri.app/distribute/windows-installer/#skipping-installation) documents the installed WebView2 requirement when acquisition is skipped. A future static-CRT change requires a fresh build, license review of linked runtime material and binary inspection; it is not enabled implicitly by this release.

The verified desktop source revision is `2c3de39e05271353a3818a2fd10e81c423c27b78`. Its unbundled desktop SHA-256 is `befaa5e113618d8139948f3c83314935e3a4043a14f4050cac50feb4a60c3fa6`. The installed file has exactly three different bytes, at offsets 8,580,464–8,580,466: the documented `UNK` to `NSS` bundle marker. Pinned `tauri-utils` source (`src/platform.rs`, bundle-type definition and parser) confirms this packaging behavior. No other byte differed.

## v0.2 installed candidate

The historical table above remains the v0.1 record. On 2026-10-11, `dumpbin /dependents` was repeated against the installed v0.2 desktop and frozen release CLI from clean source `bb0456c218eb9146035a0c580deb4e546c7b9c21`. The desktop imports VCRUNTIME140.dll, VCRUNTIME140_1.dll and Windows/UCRT libraries; the CLI imports VCRUNTIME140.dll and Windows/UCRT libraries. Their SHA-256 values are respectively `3160a0f1cf1f6c7c15f53cd14babf94aa8da48cd382b06ca5d8c7b638da2771c` and `d2f4431131744206833cf858cc425ac255a5f3ef980cba91a8ff9f72b581e8eb`. The per-user installed application ran actual Radiance calculations using WebView2 154.0.4258.62 on Windows 11 build 26200.9168. Both remain unsigned; a separate clean-machine test remains unexecuted. See `v0.2-native-verification.md` for the distinct workflow and coverage record.

That v0.2 candidate 03 was subsequently superseded after user-reported viewport gesture failures. Candidate 04, built with clean-source assertions for `c6feb7bd09663ca38c3e59ad0f890bce1e91f9c5`, was installed on 2026-10-11 and its imports inspected again. Installed desktop SHA-256 `8ccbc2ad518f991e8ba7a6cd7db8cfed5fe0aacf2fb7ef9e12127e05ecdaa657` and CLI `02ea24b42311b7967afe26b6606cedd77a6840deace99406e40267723228595e` retain the same respective VC/Windows/UCRT imports. The repaired package launches through installed WebView2; gesture retest and fresh calculation acceptance are tracked separately, and are not inferred from these import checks.
