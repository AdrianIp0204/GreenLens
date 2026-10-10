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

Inspect the exact installed desktop copy as well, comparing its SHA-256 with the packaged build. Record all imported libraries; do not assume a successful developer-machine launch proves runtime portability. Dynamically loaded WebView2 components also require the runtime even if they are absent from this static import listing.

| Artifact | Evidence | Status |
| --- | --- | --- |
| Rust native standard-library smoke | `rustc --print cfg --target x86_64-pc-windows-msvc` contains no `crt-static`. `dumpbin /dependents` reports VCRUNTIME140.dll, six `api-ms-win-crt-*` libraries and Windows system libraries. | Verified 2026-10-11; toolchain evidence only. |
| Provisional desktop before explicit configuration | Imported UCRT and Windows libraries without VCRUNTIME140.dll. Inspection of pinned `tauri-build` 2.7.1 showed its default static VC support-library override. | Diagnostic only; prompted explicit `staticVCRuntime: false` and requires a new artifact check. |
| Release CLI | Import inspection requires the completed artifact. | Pending. |
| Installed desktop | Import inspection, copy hash and real native launch require the completed package. | Pending. |
| Clean machine without developer tools | Not tested on a separate clean Windows installation. | Unverified; installation route documented. |

The [Rust linkage reference](https://doc.rust-lang.org/reference/linkage.html#static-and-dynamic-c-runtimes) documents `crt-static` and recommends checking the resulting executable. [Tauri's Windows installer documentation](https://v2.tauri.app/distribute/windows-installer/#skipping-installation) documents the installed WebView2 requirement when acquisition is skipped. A future static-CRT change requires a fresh build, license review of linked runtime material and binary inspection; it is not enabled implicitly by this release.
