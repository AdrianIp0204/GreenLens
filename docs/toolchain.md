# Native Windows toolchain

The primary target is `x86_64-pc-windows-msvc`. No WSL or Python runtime is needed for the application. Source stays in its chosen checkout, including a OneDrive checkout. Do not relocate it or create cache junctions.

## Prerequisites and pinned Rust installation

Install Visual Studio Build Tools with **Desktop development with C++**, including an x64 compiler and Windows SDK. Install Node.js 24 LTS and npm. The desktop requires Microsoft Edge WebView2 Runtime. These platform components have their own Microsoft/Node licenses.

GreenLens pins Rust **1.99.0**, released 2026-10-01, in `rust-toolchain.toml`. For a new per-user installation, download the official versioned rustup bootstrapper, verify its SHA-256 before execution, then install without changing persistent PATH:

```powershell
$download = Join-Path $env:LOCALAPPDATA 'GreenLens\downloads'
New-Item -ItemType Directory -Force -Path $download | Out-Null
$installer = Join-Path $download 'rustup-init-1.29.1.exe'
Invoke-WebRequest 'https://static.rust-lang.org/rustup/archive/1.29.1/x86_64-pc-windows-msvc/rustup-init.exe' -OutFile $installer
$expected = '6f4bef66261261fcb43131be8720bab817d403a09edec7455c371974b90bdb7e'
if ((Get-FileHash $installer -Algorithm SHA256).Hash.ToLowerInvariant() -ne $expected) { throw 'rustup checksum mismatch' }
& $installer -y --no-modify-path --profile minimal --default-host x86_64-pc-windows-msvc --default-toolchain 1.99.0 --component rustfmt,clippy
if ($LASTEXITCODE -ne 0) { throw 'Rust installation failed' }
```

This initializes a new per-user Rust installation. For an existing installation, preserve its settings and run `rustup toolchain install 1.99.0 --profile minimal --component rustfmt,clippy --target x86_64-pc-windows-msvc`; the repository's toolchain file selects the version only in this checkout.

Official references: [Rust installation](https://rust-lang.org/tools/install/), [rustup Windows installation](https://rust-lang.github.io/rustup/installation/windows.html), [versioned installer digest](https://static.rust-lang.org/rustup/archive/1.29.1/x86_64-pc-windows-msvc/rustup-init.exe.sha256), [Rust 1.99.0 manifest](https://static.rust-lang.org/dist/channel-rust-1.99.0.toml).

## Each development shell

From the repository root:

```powershell
. ./scripts/dev-env.ps1
rustc --version --verbose
cargo --version
cargo test --workspace
```

The script locates C++ Build Tools using Microsoft's `vswhere`, imports the native x64 developer environment, prepends the existing per-user Cargo binaries, sets `CARGO_TARGET_DIR` to `%LOCALAPPDATA%\GreenLens\build\<workspace-id>` and sets npm's download cache to `%LOCALAPPDATA%\GreenLens\npm-cache`. All settings apply only to the current process and children. Different checkout paths receive distinct workspace IDs. No persistent environment or unrelated global configuration is changed.

`node_modules` remains inside the source checkout so npm, Vite and Tauri resolve packages normally. **`.gitignore` does not disable OneDrive synchronization**; dependency installation may therefore create synchronization overhead. The source is not silently relocated.

## Verification record

On 2026-10-11 (Hong Kong user-facing date), inspection found native Visual Studio Build Tools 2026 18.3.2, Node.js 24.14.0, npm 11.9.0 and WebView2 154.0.4258.62. The official stable manifest and versioned rustup SHA-256 were checked before installation. Native compilation and execution of a Rust program succeeded in a local scratch directory containing spaces and Chinese characters. Verified compiler: `rustc 1.99.0 (b940084d7 2026-09-28)`, host `x86_64-pc-windows-msvc`, LLVM 23.1.1; Cargo: `1.99.0 (5f94df478 2026-08-27)`. These are tested workstation versions, not a clean-machine certification.

Toolchain verification is distinct from native Radiance and packaged desktop acceptance; consult the gate records for those checks.

The initial concurrent bootstrap and compilation attempts left inconsistent rustup component metadata. Compilation was paused, only the newly acquired 1.99.0 toolchain was uninstalled, and the pinned minimal toolchain was installed once without competing rustup processes. After repair, a dependency-free Cargo application using the standard library compiled and ran through the normal Cargo shim under a path containing spaces and Chinese characters. `cargo fmt` and `cargo clippy --offline -- -D warnings` also passed. `rustup component list --installed` confirmed cargo, rustc, rust-std, rustfmt and clippy for the native target. This full check supersedes version-only evidence.

When a Git commit exists, the development script sets process-local `GREENLENS_SOURCE_COMMIT` from the checkout HEAD unless the build environment already supplied it. It also sets `GREENLENS_SOURCE_DIRTY` from tracked and non-ignored untracked changes unless explicitly supplied. Compiled provenance can therefore distinguish a clean source revision from a development build with uncommitted changes.

The repository does not add `crt-static` Rust flags. The desktop explicitly disables Tauri's default static VC support-library override using `build.windows.staticVCRuntime: false`, so it and the CLI use the dynamic Microsoft C runtime. Build Tools are development prerequisites; executable recipients instead need the separately acquired Microsoft Visual C++ v14 x64 Redistributable. See `windows-runtime.md` for binary inspection and final-release verification status.

## Dependency acquisition recovery

The initial native dependency acquisition encountered slow, intermittent official sparse-index and archive transfers. TLS and DNS checks succeeded. One 2.29 MB sparse-index transfer took approximately 95 seconds. Process-local `CARGO_HTTP_TIMEOUT=300` allowed version resolution to finish; no global Cargo configuration or registry mirror was introduced.

After the resolved lockfile existed, three remaining native Windows archives were acquired directly from `https://static.crates.io/crates/<name>/<name>-<version>.crate` with Windows `curl.exe --compressed`, bounded connect/total timeouts and normal certificate validation. Each complete archive was checked against its SHA-256 in `Cargo.lock`. With Cargo fetching paused, a uniquely named temporary file was copied into the existing official registry cache, rechecked, and atomically renamed only if the destination did not exist. Existing conflicting files would have been preserved and reported. No downloaded archive belongs in the source repository.

| Archive | Bytes | Observed acquisition | SHA-256 from locked source |
| --- | ---: | ---: | --- |
| windows 0.62.2 | 9,360,572 | 2.28 s | `527fadee13e0c05939a6a05d5bd6eec6cd2e3dbd648b9f8e447c6518133d8580` |
| webview2-com-sys 0.39.1 | 5,768,930 | 2.23 s | `b3a07132775117d6065853d9d1178157b8c90e228de47129d6bce2c7edebedfb` |
| window-vibrancy 0.8.1 | 1,416,463 | 0.80 s | `111e51caca442cafd9bab396628ac4d814880eaea4e63dfd76a12e9f342b5580` |

The alternative official `/crates/windows/0.62.2/download` endpoint returned the same bytes and digest, without a speed advantage. Throughput varied by request; the evidence does not establish an HTTP protocol bug. Partial diagnostic downloads were not promoted. After these three verified promotions, `cargo metadata --locked --offline --filter-platform x86_64-pc-windows-msvc --format-version 1` succeeded. This closes native dependency acquisition; dependencies for unused platforms were not downloaded to satisfy the release.
