// Prepare exact reviewed text sources for low-resource native unit tests only.
// No network, SKP parsing, worker build or conversion.
import fs from 'node:fs';
import path from 'node:path';
import crypto from 'node:crypto';
import {execFileSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';
import {minizPin, patchMiniz} from './patches.mjs';
const here = path.dirname(fileURLToPath(import.meta.url));
const [miniz, destination, ...extra] = process.argv.slice(2);
if (!destination || extra.length) throw new Error('Usage: node prepare-preallocation-tests.mjs MINIZ_CHECKOUT NEW_EXTERNAL_DESTINATION');
const git = (...args) => execFileSync('git', ['-c','core.longpaths=true','-C',miniz,...args], {encoding:'utf8',maxBuffer:4*1024*1024}).trimEnd();
if (git('rev-parse','HEAD') !== minizPin || git('status','--porcelain','--untracked-files=all'))
  throw new Error('Exact clean pinned miniz source required');
if (git('remote','get-url','origin') !== 'https://github.com/richgel999/miniz.git')
  throw new Error('Unexpected miniz source origin');
const target = path.resolve(destination);
const external = path.resolve(process.env.LOCALAPPDATA ?? '', 'GreenLens');
if (!process.env.LOCALAPPDATA || !target.toLowerCase().startsWith(external.toLowerCase()+path.sep))
  throw new Error('Unit sources must be outside the checkout under LOCALAPPDATA/GreenLens');
if (fs.existsSync(target)) throw new Error('Preserve existing unit-test sources; use a new destination');
for (let parent = path.dirname(target); fs.existsSync(parent); parent = path.dirname(parent)) {
  if (fs.lstatSync(parent).isSymbolicLink()) throw new Error('Reparse destination ancestry unsupported');
  if (path.dirname(parent) === parent) break;
}
const sha = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
const files = [];
const put = (relative, data) => {
  fs.writeFileSync(path.join(target,relative),data,{flag:'wx'});
  files.push({path:relative,bytes:Buffer.byteLength(data),sha256:sha(data)});
};
fs.mkdirSync(target,{recursive:true});
try {
  for (const file of ['miniz.c','miniz.h','miniz_common.h','miniz_tdef.c','miniz_tdef.h','miniz_tinfl.c','miniz_tinfl.h','miniz_zip.c','miniz_zip.h','LICENSE']) {
    const blob = execFileSync('git',['-C',miniz,'show',`${minizPin}:${file}`],{maxBuffer:4*1024*1024});
    const actual = fs.readFileSync(path.join(miniz,file),'utf8').replaceAll('\r\n','\n');
    if (actual !== blob.toString('utf8').replaceAll('\r\n','\n')) throw new Error(`Pinned working source differs: ${file}`);
    put(file,patchMiniz(file,actual));
  }
  put('miniz_export.h','#pragma once\n#define MINIZ_EXPORT\n');
  put('preallocation_bounds.h',fs.readFileSync(path.join(here,'preallocation_bounds.h')));
  put('preallocation_test.cpp',fs.readFileSync(path.join(here,'tests/preallocation_test.cpp')));
  const manifest = {schema_version:'greenlens.openskp-preallocation-unit-source.v1',miniz_pin:minizPin,
    patch_sha256:sha(fs.readFileSync(path.join(here,'patches.mjs'))),
    files:files.sort((a,b)=>a.path.localeCompare(b.path,'en')),skp_parses:0,worker_executions:0};
  fs.writeFileSync(path.join(target,'unit-source-manifest.json'),JSON.stringify(manifest,null,2)+'\n',{flag:'wx'});
  console.log(JSON.stringify({schema_version:manifest.schema_version,files:manifest.files.length,
    manifest_sha256:sha(fs.readFileSync(path.join(target,'unit-source-manifest.json')))}));
} catch(error) {
  fs.writeFileSync(path.join(target,'PREPARATION_FAILED.txt'),String(error)+'\n',{flag:'wx'});
  throw error;
}
