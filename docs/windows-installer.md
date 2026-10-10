# Windows installer behavior

GreenLens uses the pinned `@tauri-apps/cli` 2.12.1 NSIS implementation with a retained template in `apps/desktop/src-tauri/installer.nsi`. The template has one functional modification: a new current-user installation defaults to `%LOCALAPPDATA%\Programs\GreenLens`. This separates installed program files from `%LOCALAPPDATA%\GreenLens` engines, builds and active jobs. The ordinary directory selector, explicit `/D=...` option and previous-install location restoration remain upstream behavior.

The source is [Tauri's installer template at commit 30da1fd6e17de6107ecc850c95dfb16b5729f2dd](https://github.com/tauri-apps/tauri/blob/30da1fd6e17de6107ecc850c95dfb16b5729f2dd/crates/tauri-bundler/src/bundle/windows/nsis/installer.nsi), matching the npm package's `gitHead` and `tauri-cli-v2.12.1` tag. Original upstream SHA-256: `dabed59013b1d78b879a1a85bc7f2eed2993b33a9a90cdabe5946de3d3950597`. The MIT permission/copyright notice is retained in `third-party/tauri-template-license.txt`; upstream offers Apache-2.0 OR MIT.

The reviewed uninstaller deletes named application binaries and bundled resource files, then attempts nonrecursive removal of their directories and the installation directory. It does not recursively remove the installation directory. The only recursive data removals target the Tauri bundle identifier's directories (`org.greenlens.desktop`) under roaming/local AppData when the user chooses application-data removal; these differ from GreenLens engines/jobs and user-selected portable project directories. Saved evidence is not installed as an application resource.

Modified template SHA-256 (including the added provenance header): `044b64e5095444eba1b09a4fd870a088f0ffa87217ed533439b26c6c486d7539`.

Template upgrades require review of directory selection, resource paths and uninstall cleanup against the newly pinned upstream source. Compilation and installed-app acceptance are recorded separately in `native-verification.md`; source inspection alone is not an install/uninstall execution test.

The package is unsigned. WebView2, the Microsoft Visual C++ v14 x64 Redistributable and Radiance must already be installed as described in the installation guide. The installer does not acquire these prerequisites. See `windows-runtime.md` for runtime-linkage evidence and the distinction between toolchain checks and final-package inspection.
