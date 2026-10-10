# Installation and launch

Release status: implementation and verification are in progress. Package paths and executed acceptance evidence will be recorded in `docs/progress.md`; the following is the intended reproducible route, not a claim that a clean machine has been tested.

## Windows users

1. Use a supported x64 Windows installation with the [Microsoft Edge WebView2 Runtime](https://developer.microsoft.com/en-us/microsoft-edge/webview2/) and [Microsoft Visual C++ v14 x64 Redistributable](https://learn.microsoft.com/en-us/cpp/windows/latest-supported-vc-redist). Use the latest supported redistributable, at least as recent as the release's MSVC toolset. GreenLens uses the installed Evergreen WebView2 Runtime. Its installer does not silently acquire either Microsoft prerequisite. See [runtime requirements and observed imports](windows-runtime.md).
2. Obtain the unsigned GreenLens per-user NSIS setup package from the project build/release artifacts. Signing and SmartScreen reputation are not claimed. The application installs for the current user.
3. Acquire the separately licensed pinned Radiance engine. In PowerShell 7.2 or newer, from a source checkout, run `./scripts/install-radiance.ps1`. It verifies the official release asset and SHA-256 before extraction into `%LOCALAPPDATA%/GreenLens/engines/radiance-6.0.2/package`. There is no engine binary inside the GreenLens evidence ZIP or desktop installer.
4. Launch GreenLens from its installed shortcut. Use the synthetic A/B example, enter a project directory and run the calculation. Save the completed result, run the second alternative, compare, then export to a new ZIP filename.

For an engine installed elsewhere, select the directory containing `bin` and `lib` in the app. CLI commands accept `--engine-dir <directory>`. Initial acquisition needs internet; ordinary calculation, reopening, report viewing and exporting operate locally without it.

## Build from source

Follow `docs/toolchain.md`, then run from the checkout:

```powershell
. ./scripts/dev-env.ps1
./scripts/install-radiance.ps1
Push-Location apps/desktop
npm ci
npm run build
npm run tauri -- dev
Pop-Location
```

For a production package and CLI:

```powershell
. ./scripts/dev-env.ps1
cargo build -p greenlens-cli --release
Push-Location apps/desktop
npm run tauri -- build --ci
Pop-Location
Get-ChildItem "$env:CARGO_TARGET_DIR/release/bundle/nsis"
```

The CLI is `$env:CARGO_TARGET_DIR/release/greenlens.exe`. The installer is beneath `$env:CARGO_TARGET_DIR/release/bundle/nsis`. The production app embeds the frontend and runs without a Vite development server. Preserve the version-matched source documentation and third-party notices alongside distributed artifacts.

## Storage and synchronization

Source stays in the checkout. Build products, engine installation, disposable octrees and active jobs are outside OneDrive under local application data. `node_modules` remains in the checkout; `.gitignore` does not stop OneDrive from synchronizing it. No junctions or silent source relocation are used.

A project contains `project.json` and relative `runs/<id>` evidence directories. Copy the whole directory after saving. Historical run inputs are immutable; edited alternatives can make a run stale. External edits and concurrent writers are rejected rather than overwritten. Simultaneous editing across devices is unsupported. Export to a new ZIP filename; existing exports are not replaced.

For a lost lock after an application crash, ordinary OS locks release automatically; the small `.greenlens.lock` file may remain and is harmless. Do not remove a file to try to bypass a live writer.
