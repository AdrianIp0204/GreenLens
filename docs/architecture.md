# Architecture

The Rust core defines versioned inputs, authoritative geometry, sensor generation, scientific conversion, hashes, statistics and compatibility. The Radiance adapter produces bounded engine inputs and invokes the pinned executables. The runner/application service coordinates one job, process supervision, persistence, comparison, evidence export and replay. CLI and Tauri commands call the same service; React/Three.js only renders validated geometry/results and issues typed bounded requests.

Simulation adapters produce evidence. Data providers supply validated inputs with source rights. Assessment modules can consume evidence but do not define simulation physics. Only the Radiance adapter and synthetic input data exist in v0.1; the latter two categories are architectural boundaries, not a marketplace or dynamic ABI.

Durable project data is relative, JSON/text and checksummed. Active jobs and octrees live in per-user local cache. Finalization validates before promotion and manifest update. Historical runs preserve their input snapshots; current inputs determine staleness. Packages never supply executable commands. Native cancellation/shutdown supervises the entire process tree.
