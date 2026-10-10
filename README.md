# GreenLens

Local-first building environmental simulation with Rust, a native Tauri desktop, and Radiance.

**Static daylight illuminance under a prescribed CIE standard overcast sky.**

Version 0.1 is under implementation. Gate 0 has executed the native engine; the remaining release acceptance is tracked in [the implementation plan](docs/implementation-plan.md) and [progress record](docs/progress.md). Do not interpret unfinished checks as a validated release.

The bounded workflow models one rectangular room, a rectangular window, an optional horizontal overhang and a detached opaque obstruction. Inputs and completed runs stay in portable local project directories. Normal calculation requires no internet, cloud account, paid service or Python runtime.

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

Read [the release specification](docs/v0.1-spec.md), [architecture](docs/architecture.md), [model contract](docs/model-contract.md), [engine feasibility](docs/engine-feasibility.md) and [excluded roadmap](docs/roadmap.md).

Original code is licensed under Apache-2.0. Third-party components retain their own licenses; see `LICENSE`, `NOTICE`, `dependencies` and `third-party`.
