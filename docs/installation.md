# Installation and launch

For the active **0.4.0 energy candidate**, use the [energy status and exact source commands](v0.4-energy-status.md). EnergyPlus and Typst/fonts are acquired separately with `scripts/install-energyplus.ps1` and `scripts/install-typst.ps1`; neither engine is bundled. Saved energy inspection needs no simulator, and PDF generation needs only the report renderer. The 0.4 installed workflow remains unverified until its acceptance record is published. Historical installation outcomes below describe their named versions.

The historical v0.1 package was exercised on native Windows with no development server. Historical v0.2 package and interaction outcomes remain in [v0.2 native verification](v0.2-native-verification.md). Historical 0.3.0 adds climate studies whose scientific qualification remains failed; that bounded investigation is stopped. Consult [v0.3 native verification](v0.3-native-verification.md) and [release progress](progress.md) for its separate incomplete acceptance. The following clean-machine installation route is documented; a separate clean Windows installation has not been tested.

## Windows users

1. Use a supported x64 Windows installation with the [Microsoft Edge WebView2 Runtime](https://developer.microsoft.com/en-us/microsoft-edge/webview2/) and [Microsoft Visual C++ v14 x64 Redistributable](https://learn.microsoft.com/en-us/cpp/windows/latest-supported-vc-redist). Use the latest supported redistributable, at least as recent as the release's MSVC toolset. GreenLens uses the installed Evergreen WebView2 Runtime. Its installer does not silently acquire either Microsoft prerequisite. See [runtime requirements and observed imports](windows-runtime.md).
2. Obtain the unsigned GreenLens per-user NSIS setup package from the project build/release artifacts. Signing and SmartScreen reputation are not claimed. The application installs for the current user.
3. Acquire the separately licensed pinned Radiance engine. In PowerShell 7.2 or newer, from a source checkout, run `./scripts/install-radiance.ps1`. It verifies the official release asset and SHA-256 before extraction into `%LOCALAPPDATA%/GreenLens/engines/radiance-6.0.2/package`. There is no engine binary inside the GreenLens evidence ZIP or desktop installer.
4. Launch GreenLens from its installed shortcut. The synthetic A/B document opens in the four-view workspace with the static prescribed-overcast study. Use **Save** and select a project folder in the native dialog, then **Run**. Switch designs, run the second alternative, use **A/B compare**, then **Export** to a new ZIP filename. To calculate a weather-selected hour or a full year, import retained weather and explicitly apply that study using the [climate guide](v0.3-usage.md). See the [workspace guide](workspace-guide.md) for editing and document protection.

For a default installation, the exact PowerShell launch command is:

```powershell
& "$env:LOCALAPPDATA/Programs/GreenLens/greenlens-desktop.exe"
```

The CLI ZIP includes `greenlens.exe`, synthetic room inputs, the reviewed weather example, documentation and retained notices. Extract the entire directory before use. The following commands exercise **static prescribed overcast**, from the extracted `GreenLens-CLI-0.3.0-windows-x64` directory:

```powershell
./greenlens.exe doctor
./greenlens.exe validate ./examples/synthetic-a.json
./greenlens.exe run ./examples/synthetic-a.json --project "$env:LOCALAPPDATA/GreenLens/projects/example" --alternative A
./greenlens.exe run ./examples/synthetic-b.json --project "$env:LOCALAPPDATA/GreenLens/projects/example" --alternative B
./greenlens.exe compare "$env:LOCALAPPDATA/GreenLens/projects/example"
./greenlens.exe export "$env:LOCALAPPDATA/GreenLens/projects/example" "$env:LOCALAPPDATA/GreenLens/projects/example-evidence.zip"
./greenlens.exe replay "$env:LOCALAPPDATA/GreenLens/projects/example-evidence.zip"
```

For an engine installed elsewhere, enter the directory containing `bin` and `lib` under **Analysis → Project and engine**. The CLI `doctor`, `run`, `study run` and `replay` commands accept `--engine-dir <directory>`. Initial acquisition needs internet; ordinary calculation, reopening, report viewing and exporting operate locally without it.

The pinned Windows Radiance package requires an **ASCII-only engine installation path**, including its parent directories, for CAL-library lookup. Spaces are supported. A non-ASCII engine path can fail with `cannot find function file "rayinit.cal": Invalid argument` even when the file exists. Choose a separate local ASCII path with `./scripts/install-radiance.ps1 -InstallDirectory <path>` and configure its `package` subdirectory in GreenLens. This limitation concerns the engine and its libraries; tested project and working directories contain spaces and Chinese characters. If the Windows user profile itself has non-ASCII characters, select a writable ASCII engine location explicitly.

For historical climate studies, follow the [weather import, annual desktop and CLI guide](v0.3-usage.md). The real-weather example retains PVGIS/ERA5 attribution and source-specific time conventions. Climate verification scripts require PowerShell 7.5 or newer. The failed numerical qualification and outstanding native observations remain visible; energy verification does not qualify those daylight results.

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
$env:CARGO_BUILD_JOBS = '1' # Bound compiler memory on a shared desktop.
cargo build -p greenlens-cli --release
Push-Location apps/desktop
npm run tauri -- build --ci
Pop-Location
Get-ChildItem "$env:CARGO_TARGET_DIR/release/bundle/nsis"
node scripts/license-inventory.mjs --strict --check --installer-cache "$env:LOCALAPPDATA/tauri/NSIS"
./scripts/package-artifacts.ps1
```

The CLI is `$env:CARGO_TARGET_DIR/release/greenlens.exe`. The installer is beneath `$env:CARGO_TARGET_DIR/release/bundle/nsis`. The production app embeds the frontend and runs without a Vite development server. Preserve the version-matched source documentation and third-party notices alongside distributed artifacts.

## Storage and synchronization

Source stays in the checkout. Build products, engine installation, disposable octrees and active jobs are outside OneDrive under local application data. `node_modules` remains in the checkout; `.gitignore` does not stop OneDrive from synchronizing it. No junctions or silent source relocation are used.

A project contains `project.json`, relative `runs/<id>` evidence directories and, for retained climate weather, `weather/<id>` assets. Copy the whole directory after saving. Historical run inputs are immutable; edited geometry, weather or numerical settings can make a physical result stale, while occupancy/threshold edits change the derived metric identity. External edits and concurrent writers are rejected rather than overwritten. Simultaneous editing across devices is unsupported. Climate exports select immutable run IDs explicitly; see the mode-specific formats and tolerances in [replay](replay.md). Export to a new ZIP filename; existing exports are not replaced.

For a lost lock after an application crash, ordinary OS locks release automatically; the small `.greenlens.lock` file may remain and is harmless. Do not remove a file to try to bypass a live writer.

The v0.4 offline documentation index is version 2 with a bounded 16 MiB total and 2 MiB per document, sufficient to retain historical evidence and added reviewed runtime notices. Simulation evidence-package limits are unchanged. See [dependency review](dependencies.md).
