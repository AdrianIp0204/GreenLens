// Prepare exact reviewed text sources for low-resource native unit tests only.
// No network, SKP parsing, worker build or conversion.
import fs from 'node:fs';
import path from 'node:path';
import crypto from 'node:crypto';
import {execFileSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';
import {minizPin, patchMiniz} from './patches.mjs';
import {patchMinizExtraBounds} from './miniz-allocation-patches.mjs';
import {validateNewExternalDestination} from './preparation-paths.mjs';
const here = path.dirname(fileURLToPath(import.meta.url));
const [miniz, destination, tinyGltf, ...extra] = process.argv.slice(2);
if (!tinyGltf || extra.length) throw new Error('Usage: node prepare-preallocation-tests.mjs MINIZ_CHECKOUT NEW_EXTERNAL_DESTINATION TINYGLTF_CHECKOUT');
const git = (...args) => execFileSync('git', ['-c','core.longpaths=true','-C',miniz,...args], {encoding:'utf8',maxBuffer:4*1024*1024}).trimEnd();
if (git('rev-parse','HEAD') !== minizPin || git('status','--porcelain','--untracked-files=all'))
  throw new Error('Exact clean pinned miniz source required');
if (git('remote','get-url','origin') !== 'https://github.com/richgel999/miniz.git')
  throw new Error('Unexpected miniz source origin');
const tinyPin='488a70a3df62a4df1a736e9e56fb8836580c4888';
const tinyGit=(...args)=>execFileSync('git',['-C',tinyGltf,...args],{encoding:'utf8',maxBuffer:4*1024*1024}).trimEnd();
if(tinyGit('rev-parse','HEAD')!==tinyPin || tinyGit('status','--porcelain','--untracked-files=all') ||
    tinyGit('remote','get-url','origin')!=='https://github.com/syoyo/tinygltf.git')
  throw new Error('Exact clean pinned TinyGLTF source/origin required');
const target = validateNewExternalDestination(destination);
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
    put(file,patchMinizExtraBounds(file,patchMiniz(file,actual)));
  }
  put('miniz_export.h','#pragma once\n#define MINIZ_EXPORT\n');
  put('preallocation_bounds.h',fs.readFileSync(path.join(here,'preallocation_bounds.h')));
  put('preallocation_test.cpp',fs.readFileSync(path.join(here,'tests/preallocation_test.cpp')));
  put('miniz_extra_bounds_test.hpp',fs.readFileSync(path.join(here,'tests/miniz_extra_bounds_test.hpp')));
  for(const file of ['parser_allocation_bounds.hpp','bounded_serialization.hpp','streaming_sha256.hpp'])
    put(file,fs.readFileSync(path.join(here,file)));
  for(const file of ['parser_allocation_test.cpp','serialization_test.cpp'])
    put(file,fs.readFileSync(path.join(here,'tests',file)));
  for(const file of ['tiny_gltf.h','json.hpp','LICENSE']) {
    const blob=execFileSync('git',['-C',tinyGltf,'show',`${tinyPin}:${file}`],{maxBuffer:4*1024*1024});
    const actual=fs.readFileSync(path.join(tinyGltf,file),'utf8').replaceAll('\r\n','\n');
    if(actual!==blob.toString('utf8').replaceAll('\r\n','\n'))throw new Error(`Pinned TinyGLTF differs: ${file}`);
    put(file==='LICENSE'?'LICENSE.tinygltf':file,actual);
  }
  put('LICENSE.nlohmann-json',fs.readFileSync(path.join(here,'licenses/LICENSE.nlohmann-json')));
  const manifest = {schema_version:'greenlens.openskp-preallocation-unit-source.v1',miniz_pin:minizPin,
    patch_sha256:sha(fs.readFileSync(path.join(here,'patches.mjs'))),tinygltf_pin:tinyPin,
    extra_patch_sha256:sha(fs.readFileSync(path.join(here,'miniz-allocation-patches.mjs'))),
    files:files.sort((a,b)=>a.path.localeCompare(b.path,'en')),skp_parses:0,worker_executions:0};
  fs.writeFileSync(path.join(target,'unit-source-manifest.json'),JSON.stringify(manifest,null,2)+'\n',{flag:'wx'});
  console.log(JSON.stringify({schema_version:manifest.schema_version,files:manifest.files.length,
    manifest_sha256:sha(fs.readFileSync(path.join(target,'unit-source-manifest.json')))}));
} catch(error) {
  fs.writeFileSync(path.join(target,'PREPARATION_FAILED.txt'),String(error)+'\n',{flag:'wx'});
  throw error;
}
