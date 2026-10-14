#!/usr/bin/env node
// Produce a machine-path-free inventory from resolved packages and their actual
// local license/notice files. Run after cargo fetch/build and npm ci.
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { validateNpmSupplement } from './license-supplement-binding.mjs';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const argv = process.argv.slice(2);
const npmOnly = argv.includes('--npm-only');
const strict = argv.includes('--strict');
const check = argv.includes('--check');
const metadataIndex = argv.indexOf('--metadata');
const installerCacheIndex = argv.indexOf('--installer-cache');
if (installerCacheIndex >= 0 && !argv[installerCacheIndex + 1]) throw new Error('--installer-cache needs the NSIS cache directory');
const digest = data => createHash('sha256').update(data).digest('hex');
const slash = text => text.replaceAll('\\', '/');
const readJson = file => JSON.parse(fs.readFileSync(file, 'utf8'));
const packages = [];
const notices = new Map();
const gaps = [];
const supplements = readJson(path.join(root, 'dependencies', 'license-supplements.json'));
const cargoDirectories = new Map();

function retainedSupplement(record, files) {
  return files.map(item => {
    const resolved = path.resolve(root, item.path);
    if (!resolved.startsWith(`${root}${path.sep}`)) throw new Error('Supplemental notice path escapes repository');
    const bytes = fs.readFileSync(resolved);
    if (digest(bytes) !== item.sha256) throw new Error(`Supplemental notice changed: ${item.path}`);
    const text = new TextDecoder(item.encoding || 'utf-8', { fatal: true }).decode(bytes);
    if (!notices.has(item.sha256)) notices.set(item.sha256, { text, references: [] });
    notices.get(item.sha256).references.push(`${record.ecosystem}:${record.name}@${record.version} / ${item.path}`);
    return { ...item, bytes: bytes.length };
  });
}

function licenseFiles(directory, explicitFile) {
  if (!fs.existsSync(directory)) return [];
  const found = [];
  function visit(dir, depth, inLegalDirectory = false) {
    for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
      if (entry.isSymbolicLink()) continue;
      const file = path.join(dir, entry.name);
      if (entry.isDirectory() && depth < 3 && (/^(licenses?|legal|notices?)$/i.test(entry.name) || inLegalDirectory)) {
        visit(file, depth + 1, true);
      } else if (entry.isFile() && (inLegalDirectory || /^(licen[cs]e|copying|copyright|notice)([._-].*)?$/i.test(entry.name) || /^third[-_ ]party[-_ ].*(licen[cs]e|notice)|^third[-_ ]party[-_ ](licen[cs]e|notice)/i.test(entry.name))) {
        found.push(file);
      }
    }
  }
  visit(directory, 0);
  if (explicitFile) {
    const candidate = path.resolve(directory, explicitFile);
    if (candidate.startsWith(`${directory}${path.sep}`) && fs.existsSync(candidate) && !found.includes(candidate)) found.push(candidate);
  }
  return found.sort();
}

function evidence(record, directory, explicitFile) {
  const files = [];
  for (const file of licenseFiles(directory, explicitFile)) {
    const bytes = fs.readFileSync(file);
    if (bytes.length > 2 * 1024 * 1024) {
      gaps.push(`${record.ecosystem}:${record.name}@${record.version}: oversized license file requires review`);
      continue;
    }
    const hash = digest(bytes);
    const relative = slash(path.relative(directory, file));
    files.push({ path: relative, sha256: hash, bytes: bytes.length });
    if (!notices.has(hash)) notices.set(hash, { text: bytes.toString('utf8'), references: [] });
    notices.get(hash).references.push(`${record.ecosystem}:${record.name}@${record.version} / ${relative}`);
  }
  return files;
}

function obligations(expression) {
  if (!expression) return { review: 'missing_license_declaration', requirements: ['Establish the actual license before distribution.'] };
  // Preserve the expression, including AND/OR. This is a triage policy, not an
  // SPDX parser or a substitute for the retained complete license text.
  const known = new Set(['MIT', 'Apache-2.0', 'BSD-2-Clause', 'BSD-3-Clause', 'BSD-3-Clause-Clear', 'ISC', '0BSD', 'Unlicense', 'CC0-1.0', 'Zlib', 'BSL-1.0', 'Unicode-3.0', 'Unicode-DFS-2016', 'MPL-2.0', 'LLVM-exception']);
  const terms = expression.replace(/[()]/g, ' ').split(/\s+(?:AND|OR|WITH)\s+|\s*\/\s*/).map(s => s.trim()).filter(Boolean);
  const unknown = terms.filter(term => !known.has(term));
  const requirements = ['Retain applicable copyright, license and attribution notices; see exact retained text.'];
  if (expression.includes('Apache-2.0')) requirements.push('Preserve applicable NOTICE material; identify modifications; no trademark rights are granted.');
  if (expression.includes('MPL-2.0')) requirements.push('Make MPL-covered source and its license available to executable recipients; exact unmodified source download is recorded.');
  if (expression.includes('Zlib')) requirements.push('Do not misrepresent origin; mark altered versions and retain the notice.');
  if (expression.includes(' OR ')) requirements.push('Alternatives are preserved in the inventory; an applicable permitted alternative must be followed.');
  return { review: unknown.length ? 'manual_review_required' : 'recognized_declared_license', unrecognized_terms: unknown, requirements };
}

function addPackage(record, directory, explicitFile, companion) {
  record.license_evidence = evidence(record, directory, explicitFile);
  if (companion) {
    const companionEvidence = evidence(record, companion.directory);
    record.companion_notice_source = companion.source;
    record.license_evidence.push(...companionEvidence.map(item => ({ ...item, source_package: companion.source })));
  }
  const supplement = supplements.packages[`${record.ecosystem}:${record.name}@${record.version}`];
  if (supplement) {
    if (record.ecosystem === 'npm') {
      validateNpmSupplement(record, directory, supplement);
    } else {
      const sourceCommit = readJson(path.join(directory, '.cargo_vcs_info.json')).git.sha1;
      if (record.ecosystem !== 'cargo' || sourceCommit !== supplement.source_commit || record.declared_license !== supplement.declared_license) {
        throw new Error(`Supplemental license review no longer matches ${record.name}@${record.version}`);
      }
    }
    if (supplement.selected_license) {
      if (!record.declared_license.split(/\s+OR\s+/).includes(supplement.selected_license)) throw new Error('Selected license is not a declared alternative');
      record.selected_license = supplement.selected_license;
    }
    record.supplemental_license_review = { ...supplement, files: undefined };
    record.license_evidence.push(...retainedSupplement(record, supplement.files));
  }
  // A scoped npm review is not a global license-name allowlist. Its exact
  // package/source/notice bindings above must pass before applying obligations.
  record.redistribution = record.ecosystem === 'npm' && supplement?.redistribution
    ? supplement.redistribution : obligations(record.selected_license || record.declared_license);
  record.modifications = 'No upstream source modifications made by GreenLens.';
  if (record.installed && !record.license_evidence.length) gaps.push(`${record.ecosystem}:${record.name}@${record.version}: package has no retained license/notice file`);
  if (record.installed && !['recognized_declared_license', 'manual_reviewed'].includes(record.redistribution.review)) gaps.push(`${record.ecosystem}:${record.name}@${record.version}: ${record.redistribution.review}: ${record.declared_license}`);
  packages.push(record);
}

const cargoLock = fs.readFileSync(path.join(root, 'Cargo.lock'), 'utf8');
const lockedCrates = new Map();
for (const block of cargoLock.split('[[package]]').slice(1)) {
  const get = key => block.match(new RegExp(`^${key} = "([^"]+)"`, 'm'))?.[1];
  lockedCrates.set(`${get('name')}@${get('version')}`, { source: get('source'), checksum: get('checksum') });
}

if (!npmOnly) {
  let metadata;
  if (metadataIndex >= 0) {
    if (!argv[metadataIndex + 1]) throw new Error('--metadata needs a JSON file');
    metadata = readJson(path.resolve(argv[metadataIndex + 1]));
  } else {
    const cargo = process.env.GREENLENS_CARGO || 'cargo';
    const result = spawnSync(cargo, ['metadata', '--locked', '--offline', '--format-version', '1', '--filter-platform', 'x86_64-pc-windows-msvc'], { cwd: root, encoding: 'utf8', maxBuffer: 64 * 1024 * 1024, windowsHide: true });
    if (result.status !== 0) throw new Error(`cargo metadata failed; source scripts/dev-env.ps1 and finish dependency acquisition first.\n${result.stderr || result.error?.message || ''}`);
    metadata = JSON.parse(result.stdout);
  }
  const nodes = new Map(metadata.resolve.nodes.map(node => [node.id, node]));
  const reachable = new Set();
  function walk(id) {
    if (reachable.has(id)) return;
    reachable.add(id);
    for (const dep of nodes.get(id)?.deps || []) walk(dep.pkg);
  }
  metadata.workspace_members.forEach(walk);
  for (const item of metadata.packages.filter(item => reachable.has(item.id) && item.source)) {
    const lock = lockedCrates.get(`${item.name}@${item.version}`);
    if (!lock) throw new Error(`Missing Cargo lock identity for ${item.name}`);
    cargoDirectories.set(`${item.name}@${item.version}`, path.dirname(item.manifest_path));
    addPackage({
      ecosystem: 'cargo', name: item.name, version: item.version,
      source: item.source, repository: item.repository || null,
      source_download: item.source.startsWith('registry+') ? `https://crates.io/api/v1/crates/${item.name}/${item.version}/download` : item.source,
      integrity: lock.checksum ? `sha256:${lock.checksum}` : null,
      declared_license: item.license || null, installed: true,
      usage: 'Resolved Windows dependency graph, including build/test tooling; not every crate is linked into every artifact.',
    }, path.dirname(item.manifest_path), item.license_file);
  }
  // Rust's linked standard library is outside Cargo.lock. Retain its complete
  // release notice, including its own dependency inventory and source notices.
  const rustc = process.env.GREENLENS_RUSTC || 'rustc';
  const sysrootResult = spawnSync(rustc, ['--print', 'sysroot'], { cwd: root, encoding: 'utf8', windowsHide: true });
  const versionResult = spawnSync(rustc, ['--version'], { cwd: root, encoding: 'utf8', windowsHide: true });
  if (sysrootResult.status !== 0 || versionResult.status !== 0) throw new Error('Cannot establish the Rust standard-library version and notice location');
  const sysroot = sysrootResult.stdout.trim();
  const rustVersion = versionResult.stdout.trim().split(' ')[1];
  const standardNotice = fs.readFileSync(path.join(sysroot, 'share', 'doc', 'rust', 'COPYRIGHT-library.html'));
  const standardHash = digest(standardNotice);
  notices.set(standardHash, { text: standardNotice.toString('utf8'), references: [`rust-standard-library@${rustVersion} / COPYRIGHT-library.html`] });
  const nativeArchives = {};
  const nativeLibraryDirectory = path.join(sysroot, 'lib', 'rustlib', 'x86_64-pc-windows-msvc', 'lib');
  for (const entry of fs.readdirSync(nativeLibraryDirectory)) {
    if (/^lib(?:std-|core-|alloc-|panic_|unwind-|compiler_builtins-).*\.rlib$/.test(entry)) {
      nativeArchives[entry] = digest(fs.readFileSync(path.join(nativeLibraryDirectory, entry)));
    }
  }
  if (!Object.keys(nativeArchives).some(name => name.startsWith('libstd-'))) throw new Error('Native Windows standard-library archive is missing');
  packages.push({
    ecosystem: 'rust-standard-library', name: 'rust-standard-library', version: rustVersion,
    source: `https://github.com/rust-lang/rust/tree/${rustVersion}/library`,
    source_download: `https://github.com/rust-lang/rust/archive/refs/tags/${rustVersion}.tar.gz`,
    declared_license: 'MIT OR Apache-2.0',
    license_scope: 'Standard-library umbrella terms; detailed per-file and incorporated dependency terms are retained in the full release notice.',
    installed: true, usage: 'Native runtime code linked into Rust application artifacts.',
    modifications: 'No Rust standard-library source modifications made by GreenLens.',
    native_archive_sha256: nativeArchives,
    license_evidence: [{ path: 'COPYRIGHT-library.html', sha256: standardHash, bytes: standardNotice.length }],
    redistribution: obligations('MIT OR Apache-2.0'),
  });

  const sdk = supplements.webview2_sdk;
  const bindingsDirectory = cargoDirectories.get(sdk.parent_crate);
  if (!bindingsDirectory) throw new Error('Reviewed WebView2 SDK parent crate is absent; re-review the desktop dependency graph');
  for (const [file, expectedHash] of Object.entries(sdk.binary_sha256)) {
    if (digest(fs.readFileSync(path.join(bindingsDirectory, file))) !== expectedHash) {
      throw new Error(`Reviewed Microsoft WebView2 SDK loader changed: ${file}`);
    }
  }
  const sdkRecord = {
    ecosystem: 'embedded-native-sdk', name: sdk.name, version: sdk.version,
    source: sdk.source, source_download: sdk.source_download,
    integrity: `sha256:${sdk.archive_sha256}`, declared_license: sdk.declared_license,
    installed: true, parent_crate: sdk.parent_crate,
    usage: 'Microsoft WebView2LoaderStatic.lib is linked into the MSVC desktop executable; the browser runtime remains an external installation.',
    modifications: 'No SDK binary modifications. Exact x64 loader bytes were compared with the official NuGet SDK archive.',
    binary_sha256: sdk.binary_sha256, review: sdk.review, version_source: sdk.version_source,
    redistribution: obligations(sdk.declared_license),
  };
  sdkRecord.license_evidence = retainedSupplement(sdkRecord, sdk.files);
  packages.push(sdkRecord);
}

const npmRoot = path.join(root, 'apps', 'desktop');
const npmLock = readJson(path.join(npmRoot, 'package-lock.json'));
for (const [key, item] of Object.entries(npmLock.packages)) {
  if (!key) continue;
  const directory = path.join(npmRoot, key);
  const manifestFile = path.join(directory, 'package.json');
  const installed = fs.existsSync(manifestFile);
  const manifest = installed ? readJson(manifestFile) : {};
  const name = manifest.name || key.split('node_modules/').at(-1);
  if (installed && manifest.version !== item.version) throw new Error(`Installed npm package ${name} differs from lock; run npm ci`);
  const license = typeof manifest.license === 'string' ? manifest.license : typeof item.license === 'string' ? item.license : null;
  let companion;
  // These native build bindings deliberately contain only their binary. Their
  // exact-version parent package carries licensing material for the binding.
  const companionName = name === '@rolldown/binding-win32-x64-msvc' ? 'rolldown' : name === '@tauri-apps/cli-win32-x64-msvc' ? '@tauri-apps/cli' : null;
  if (installed && companionName) {
    const companionDirectory = path.join(npmRoot, 'node_modules', companionName);
    const parent = readJson(path.join(companionDirectory, 'package.json'));
    if (parent.version !== item.version || parent.optionalDependencies?.[name] !== item.version || parent.license !== license) {
      throw new Error(`Native build binding ${name} needs fresh companion-notice review`);
    }
    companion = { directory: companionDirectory, source: `npm:${companionName}@${parent.version}` };
  }
  addPackage({
    ecosystem: 'npm', name, version: item.version,
    source: item.resolved || null,
    repository: typeof manifest.repository === 'string' ? manifest.repository : manifest.repository?.url || null,
    source_download: item.resolved || null,
    integrity: item.integrity || null, declared_license: license, installed,
    usage: item.dev ? 'Development/build/test tooling; not intentionally shipped as an installed npm tree.' : 'Frontend dependency; reachable bundled modules may be distributed in desktop assets.',
    optional: Boolean(item.optional), platforms: item.os || null, cpus: item.cpu || null,
  }, directory, null, companion);
}

const installerTemplate = 'apps/desktop/src-tauri/installer.nsi';
const installerBytes = fs.readFileSync(path.join(root, installerTemplate));
const installerHash = digest(installerBytes);
if (installerHash !== '044b64e5095444eba1b09a4fd870a088f0ffa87217ed533439b26c6c486d7539') {
  throw new Error('The reviewed vendored NSIS template changed; update its source/modification review and recorded digest before regenerating the release inventory');
}
const installerLicensePath = 'third-party/tauri-template-license.txt';
const installerLicense = fs.readFileSync(path.join(root, installerLicensePath));
const installerLicenseHash = digest(installerLicense);
if (!notices.has(installerLicenseHash)) notices.set(installerLicenseHash, { text: installerLicense.toString('utf8'), references: [] });
notices.get(installerLicenseHash).references.push(`vendored-template:tauri-nsis-installer@2.12.1 / ${installerLicensePath}`);
packages.push({
  ecosystem: 'vendored-template', name: 'tauri-nsis-installer', version: '2.12.1',
  source: 'https://github.com/tauri-apps/tauri/blob/30da1fd6e17de6107ecc850c95dfb16b5729f2dd/crates/tauri-bundler/src/bundle/windows/nsis/installer.nsi',
  source_download: 'https://raw.githubusercontent.com/tauri-apps/tauri/30da1fd6e17de6107ecc850c95dfb16b5729f2dd/crates/tauri-bundler/src/bundle/windows/nsis/installer.nsi',
  source_commit: '30da1fd6e17de6107ecc850c95dfb16b5729f2dd',
  upstream_sha256: 'dabed59013b1d78b879a1a85bc7f2eed2993b33a9a90cdabe5946de3d3950597',
  file: installerTemplate, modified_sha256: installerHash,
  declared_license: 'Apache-2.0 OR MIT', selected_license: 'MIT', installed: true,
  usage: 'Template compiled into the Windows installer.',
  modifications: 'Added provenance header. Changed fresh current-user default installation directory to LocalAppData/Programs/GreenLens. Existing-location restoration and directory selection retain upstream behavior.',
  review_document: 'docs/windows-installer.md',
  license_evidence: [{ path: installerLicensePath, sha256: installerLicenseHash, bytes: installerLicense.length }],
  redistribution: obligations('MIT'),
});

for (const component of supplements.installer_components) {
  const record = { ...component, files: undefined, ecosystem: 'installer-component', installed: installerCacheIndex >= 0 };
  record.integrity = `sha256:${component.archive_sha256}`;
  record.redistribution = component.redistribution || obligations(component.selected_license || component.declared_license);
  record.license_evidence = retainedSupplement(record, component.files);
  if (installerCacheIndex >= 0) {
    const cacheDirectory = path.resolve(argv[installerCacheIndex + 1]);
    for (const [file, expectedHash] of Object.entries(component.binary_sha256)) {
      if (digest(fs.readFileSync(path.join(cacheDirectory, file))) !== expectedHash) throw new Error(`Installer cache differs from reviewed component: ${component.name}/${file}`);
    }
  }
  record.cache_verified = installerCacheIndex >= 0;
  packages.push(record);
}

packages.sort((a, b) => `${a.ecosystem}:${a.name}@${a.version}`.localeCompare(`${b.ecosystem}:${b.name}@${b.version}`, 'en'));
const licenseCounts = {};
for (const item of packages) licenseCounts[item.declared_license || 'UNKNOWN'] = (licenseCounts[item.declared_license || 'UNKNOWN'] || 0) + 1;
const inventory = {
  schema_version: 'greenlens.dependencies.v0.1',
  generated_utc: new Date().toISOString(),
  scope: npmOnly ? 'PARTIAL: npm only; no Rust graph was inspected' : 'Resolved native Windows Rust graph and locked npm graph',
  target: 'x86_64-pc-windows-msvc',
  lockfiles: { 'Cargo.lock': digest(cargoLock), 'apps/desktop/package-lock.json': digest(fs.readFileSync(path.join(npmRoot, 'package-lock.json'))) },
  scientific_engine_manifest: 'dependencies/radiance.json',
  supplemental_notice_manifest: 'dependencies/license-supplements.json',
  installer_cache_verified: installerCacheIndex >= 0,
  external_runtimes: [
    { name: 'Microsoft Edge WebView2 Evergreen Runtime', source: 'https://developer.microsoft.com/en-us/microsoft-edge/webview2/', bundled: false, license: 'Microsoft proprietary runtime terms; separate upstream acquisition', review_document: 'docs/installation.md' },
    { name: 'Microsoft Visual C++ v14 x64 Redistributable and Universal CRT', source: 'https://learn.microsoft.com/en-us/cpp/windows/latest-supported-vc-redist', bundled: false, license: 'Microsoft proprietary runtime terms; separate upstream acquisition', review_document: 'docs/windows-runtime.md' },
  ],
  datasets: [{ name: 'Synthetic A/B room models', source: 'examples/synthetic-a.json; examples/synthetic-b.json', license: 'Apache-2.0', external_data: false, modifications: 'Original GreenLens fixtures.' }],
  policy: 'Declarations are corroborated by the listed actual package license/notice files and their SHA-256 digests. This automated inventory flags gaps for review and does not infer grants from subprocess boundaries. Inapplicable optional npm platform packages are listed but not installed or bundled.',
  package_count: packages.length, license_counts: licenseCounts,
  review_gaps: [...new Set(gaps)].sort(),
  packages,
};
let text = 'GREENLENS THIRD-PARTY LICENSES AND NOTICES\n\n';
text += 'Generated from actual resolved package contents. Source versions, archive checksums and source download links are recorded in dependencies/resolved-inventory.json.\n';
text += 'GreenLens modifications, when present, are identified for each package in the inventory. The original license text below is retained; package references are inventory annotations.\n';
text += 'Some packages are development or platform-specific dependencies and are not present in every distributed artifact. Engine notices are retained separately in Radiance-LICENSE.txt; engine binaries are not bundled.\n\n';
for (const [hash, notice] of [...notices].sort((a, b) => a[0].localeCompare(b[0]))) {
  text += `${'='.repeat(78)}\nSHA-256: ${hash}\n${notice.references.sort().join('\n')}\n${'-'.repeat(78)}\n${notice.text}\n\n`;
}
const inventoryPath = path.join(root, 'dependencies', 'resolved-inventory.json');
const noticesPath = path.join(root, 'third-party', 'generated-notices.txt');
if (check) {
  const comparable = value => {
    const copy = structuredClone(value);
    delete copy.generated_utc;
    delete copy.installer_cache_verified;
    for (const item of copy.packages) {
      if (item.ecosystem === 'installer-component') {
        delete item.installed;
        delete item.cache_verified;
      }
    }
    return JSON.stringify(copy);
  };
  if (comparable(readJson(inventoryPath)) !== comparable(inventory)) throw new Error('Committed dependency inventory differs from resolved sources; regenerate and review it before building');
  if (fs.readFileSync(noticesPath, 'utf8') !== text) throw new Error('Committed dependency notices differ from actual source notices; regenerate and review them before building');
} else {
  fs.mkdirSync(path.dirname(inventoryPath), { recursive: true });
  fs.writeFileSync(inventoryPath, `${JSON.stringify(inventory, null, 2)}\n`);
  fs.mkdirSync(path.dirname(noticesPath), { recursive: true });
  fs.writeFileSync(noticesPath, text);
}
console.log(JSON.stringify({ packages: packages.length, notice_texts: notices.size, review_gaps: inventory.review_gaps, partial: npmOnly, check, installer_cache_verified: installerCacheIndex >= 0 }, null, 2));
if (strict && (npmOnly || inventory.review_gaps.length)) process.exitCode = 1;
