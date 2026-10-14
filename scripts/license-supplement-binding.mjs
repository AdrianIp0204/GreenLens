// Exact npm-only notice-review binding. No network or package code execution.
import fs from 'node:fs';
import path from 'node:path';
import { createHash } from 'node:crypto';

export function validateNpmSupplement(record, directory, supplement) {
  const fail = () => { throw new Error(`Supplemental npm review no longer matches ${record.name}@${record.version}`); };
  if (record.ecosystem !== 'npm' || !record.installed ||
      record.declared_license !== supplement.declared_license ||
      record.source !== supplement.source_download ||
      record.integrity !== supplement.source_integrity ||
      !/^sha512-[A-Za-z0-9+/]{86}==$/.test(supplement.source_integrity || '')) fail();
  const files = supplement.package_files_sha256;
  if (!files || typeof files !== 'object' || Array.isArray(files) || !files['package.json']) fail();
  const base = path.resolve(directory);
  for (const [relative, expected] of Object.entries(files)) {
    if (!relative || relative.includes('\\') || relative.includes(':') ||
        relative.split('/').some(part => !part || part === '.' || part === '..') ||
        !/^[a-f0-9]{64}$/.test(expected)) fail();
    let file = base;
    for (const part of relative.split('/')) {
      file = path.join(file, part);
      if (fs.lstatSync(file).isSymbolicLink()) fail();
    }
    const metadata = fs.statSync(file);
    if (!metadata.isFile() || metadata.size > 2 * 1024 * 1024) fail();
    if (createHash('sha256').update(fs.readFileSync(file)).digest('hex') !== expected) fail();
  }
  const manifest = JSON.parse(fs.readFileSync(path.join(base, 'package.json'), 'utf8'));
  if (manifest.name !== record.name || manifest.version !== record.version || manifest.license !== record.declared_license) fail();
  if (supplement.redistribution && (supplement.redistribution.review !== 'manual_reviewed' ||
      !Array.isArray(supplement.redistribution.requirements) || !supplement.redistribution.requirements.length ||
      supplement.redistribution.requirements.some(value => typeof value !== 'string' || !value.trim()))) fail();
}
