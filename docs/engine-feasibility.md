# Native Radiance feasibility

Gate 0 engine execution passed on 11 October 2026 (Hong Kong date; retained machine timestamps are UTC). See [machine-readable evidence](gate-0-evidence.json). This is actual native Windows execution, not WSL, emulation or mocked output.

## Acquisition and identity

The official [6.0.2 release](https://github.com/LBNL-ETA/Radiance/releases/tag/rad6R0P2), tag `rad6R0P2`, resolves to commit `c1700d56ccb9e34e56b3512c6cce3f579430497a`. GitHub's official release API reported `Radiance_c1700d56_Windows.zip`, 33,584,750 bytes, SHA-256 `c4f02efcb4a33f4a43b7ce0ba58100544abc317a1be61f7b0425a64a66eb8a09`. The downloaded ZIP matched. `rtrace -version` returned `RADIANCE 6.0.2 2026-02-02 LBNL (6.0.c1700d56cc)`.

Run `pwsh -NoProfile -File scripts/install-radiance.ps1`. The installer verifies upstream metadata and downloaded bytes, extracts into `%LOCALAPPDATA%/GreenLens/engines/radiance-6.0.2/package`, and checks the three required executable hashes and two critical CAL library hashes. It changes no global PATH or environment setting. The engine remains a separate upstream installation.

## Execution and path checks

Run `pwsh -NoProfile -File scripts/engine-feasibility.ps1`. Its default working directory `%LOCALAPPDATA%/GreenLens/verification/Gate 0 空間` contains spaces and non-ASCII characters. Each process receives an argument array through .NET ProcessStartInfo. Binary stdout from `oconv` goes directly to a file stream; no shell redirection or text transcoding is involved. Scene and octree arguments are relative ASCII filenames inside that working directory.

The child-only environment is `RAYPATH=.;<engine>/lib`. An initial direct `rtrace -version` without RAYPATH failed to find `rayinit.cal`; setting the child RAYPATH resolved it. No system environment edits or DLL installation were required. Microsoft dumpbin inspection found the required three executables import `VCRUNTIME140.dll`, Windows Universal CRT API sets and `KERNEL32.dll`. The selected archive supplies the MSVC runtime DLL.

The first Rust adapter run exposed another native path issue: Rust's Windows `canonicalize` returned an extended `\\?\` path, which Radiance rejected in `RAYPATH` with exit 2 and `cannot find function file "rayinit.cal": Invalid argument`. The adapter now keeps the resolved path but converts extended drive/UNC spelling to conventional Win32/UNC spelling before launching the engine. The full B–E analytical fixture run and the runner's native cancellation test subsequently passed. This repair changes no global environment setting or engine bytes.

The exact command arguments and process exits are in the evidence JSON. The calculation generated an overcast sky, explicitly added upper-sky/lower-ground luminous hemispheres, compiled an octree and evaluated an unobstructed upward sensor. All required processes exited 0, with empty stderr. RGB irradiance was `(55.77737, 55.77737, 55.77737)`, converting to **9,984.14923 lx** for a prescribed 10,000 lx (absolute relative error **0.158508%**). This establishes native execution and the exterior normalization check for these settings; it does not independently validate arbitrary building simulations.

## Pinned source review

- [gensky.c](https://github.com/LBNL-ETA/Radiance/blob/rad6R0P2/src/gen/gensky.c): `-c` selects the overcast distribution, `-B` accepts diffuse horizontal irradiance and converts with the overcast normalization factor approximately 7/9. Explicit brightness makes the supplied `-ang 45 0` a convention, not a simulated date or sun condition. The output is a brightness function, requiring explicit luminous sources.
- [color.h](https://github.com/LBNL-ETA/Radiance/blob/rad6R0P2/src/common/color.h): the default RGB photopic weights are 0.265074126, 0.670114631 and 0.064811243, and equal-energy-white efficacy is 179. These are Radiance's RGB convention, not a conversion for arbitrary broadband solar energy.
- [glass.c](https://github.com/LBNL-ETA/Radiance/blob/rad6R0P2/src/rt/glass.c): the documented transmittance-to-transmissivity formula matches the release contract, with default refractive index 1.52 and a single thin surface. It is not a multilayer glazing model.
- [rtrace manual](https://github.com/LBNL-ETA/Radiance/blob/rad6R0P2/doc/man/man1/rtrace.1): `-I+` computes irradiance at supplied positions/orientations, `-faa` makes input/output ASCII and `-ov` requests values. `-n 1` uses one process. No ambient file is supplied or reused.

The packaged `skybright.cal` blends sky and ground close to the horizon. Its upper overcast expression is proportional to `(1+2*Dz)/3`, but the blending means it is not a mathematically abrupt hemisphere boundary. Numerical analytical fixtures must account for this limited difference using the unchanged acceptance tolerances.

## License and redistribution decision

The selected ZIP contains `bin/` and `lib/`; it contains no license files. The exact-tag [Radiance Software License Version 2.0](../third-party/Radiance-LICENSE.txt) is retained unmodified. Its copyright line says v6.1 even though the pinned release/binary says 6.0.2; this is upstream text, not an inferred engine version. Radiance redistribution allows source/binary distribution subject to retained notices, non-endorsement and the enhancement grant terms.

The ZIP also includes Microsoft runtime DLLs (14.44.35211.0), Qt 5.15.2, GCC runtime, mingw-w64 winpthreads and Perl 5.32-named files. Those additional tools are unused by the required three executables. Their exact packaged licenses/versions are not completely established by the ZIP, so **GreenLens does not redistribute the ZIP, engine or these libraries**. The versioned [inventory](../dependencies/radiance.json) distinguishes observed versions from unresolved package grants. Any future bundling requires a separate complete license/notice review; a subprocess boundary is not a licensing exemption.
