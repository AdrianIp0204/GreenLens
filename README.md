# GreenLens

Local-first building environmental simulation with Rust, a native Tauri desktop, and Radiance.

**Static daylight illuminance under a prescribed CIE standard overcast sky.**

Version 0.2 adds a bounded Rhino-inspired modeling workspace: four persistent views, semantic selection, constrained handles, numeric commands, snapping, layers and Rust-validated undo/redo. Document guards, complete-copy Save As and completed-evidence recovery protect local work. See the [workspace guide](docs/workspace-guide.md), [v0.2 acceptance plan](docs/v0.2-implementation-plan.md) and [progress record](docs/progress.md) for current verification status. Windows packages are unsigned; a separate clean-machine test has not been performed.

The completed v0.1 [implementation plan](docs/implementation-plan.md), [scientific record](docs/scientific-validation.md) and [native verification](docs/native-verification.md) remain historical evidence. They are not substitutes for fresh v0.2 acceptance.

The bounded workflow models one rectangular room, a rectangular window, an optional horizontal overhang and a detached opaque obstruction. Inputs and completed runs stay in portable local project directories. Normal calculation requires no internet, cloud account, paid service or Python runtime.

Start with the [installation and launch guide](docs/installation.md). Fresh v0.2 checks are recorded in [scientific and CLI verification](docs/v0.2-scientific-verification.md) and [installed workspace verification](docs/v0.2-native-verification.md). The [v0.1 package hashes](docs/release-artifacts.json) and [v0.1 CI record](docs/ci-evidence.json) describe the historical release only.

The repaired candidate's unsigned v0.2 installer and CLI paths and hashes are listed in [v0.2 artifact inventory](docs/v0.2-release-artifacts.json). The [preserved candidate 03 inventory](docs/v0.2-candidate03-artifacts.json) retains its saved example and evidence/report. Acceptance status remains explicit; a built candidate is not a completed release.

![Installed GreenLens four-view workspace with constrained window editing](docs/images/v0.2/repaired-workspace-candidate04.jpg)

[Native screenshot context and tested sizes](docs/images/v0.2/README.md).

## Native Windows development

Install Visual Studio C++ Build Tools, WebView2, Node.js 24 and the pinned Rust toolchain following [toolchain instructions](docs/toolchain.md). From this repository in PowerShell:

```powershell
. ./scripts/dev-env.ps1
./scripts/install-radiance.ps1
cargo test -p greenlens-core
cargo test -p greenlens-radiance --lib
cargo run -p greenlens-cli -- doctor
cargo run -p greenlens-cli -- validate examples/synthetic-a.json
cargo run -p greenlens-cli -- run examples/synthetic-a.json --project "$env:LOCALAPPDATA/GreenLens/examples/room" --alternative A
cargo run -p greenlens-cli -- run examples/synthetic-b.json --project "$env:LOCALAPPDATA/GreenLens/examples/room" --alternative B
cargo run -p greenlens-cli -- compare "$env:LOCALAPPDATA/GreenLens/examples/room"
cargo run -p greenlens-cli -- export "$env:LOCALAPPDATA/GreenLens/examples/room" "$env:LOCALAPPDATA/GreenLens/examples/evidence.zip"
cargo run -p greenlens-cli -- replay "$env:LOCALAPPDATA/GreenLens/examples/evidence.zip"
```

The engine acquisition script downloads the pinned upstream engine and verifies its SHA-256. The desktop installer does not bundle or automatically install Radiance. A custom engine location can be supplied with `--engine-dir <directory-containing-bin-and-lib>`. Engine selection/configuration stays on the machine and is excluded from portable provenance.

For the desktop, run `npm ci` followed by `npm run tauri -- dev` from `apps/desktop` in the configured shell. Production build and installation commands are in [installation instructions](docs/installation.md); executed native acceptance is recorded separately. Build caches and active job scratch stay under `%LOCALAPPDATA%/GreenLens`; source and `node_modules` stay in the checkout. `.gitignore` does not disable OneDrive synchronization.

## Interpretation

The sky has a prescribed exterior illuminance and no date, weather sequence or annual schedule. Neutral equivalent glass, diffuse surfaces and simplified geometry limit interpretation. A metric is not a criterion evaluation, and a criterion evaluation is not certification. Same-engine replay verifies integration; analytical fixtures test limited behaviors rather than arbitrary building accuracy.

Read [the v0.2 workspace specification](docs/v0.2-spec.md), [historical v0.1 scientific contract](docs/v0.1-spec.md), [architecture](docs/architecture.md), [model contract](docs/model-contract.md), [engine feasibility](docs/engine-feasibility.md) and [excluded roadmap](docs/roadmap.md).

Original code is licensed under Apache-2.0. Third-party components retain their own licenses; see `LICENSE`, `NOTICE`, `dependencies` and `third-party`.
