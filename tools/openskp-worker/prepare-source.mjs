// Development source preparation only. No download, build or model parsing.
import fs from 'node:fs';
import path from 'node:path';
import crypto from 'node:crypto';
import {execFileSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';
import {pin, patchSource} from './patches.mjs';

const here = path.dirname(fileURLToPath(import.meta.url));
const [openSkp, miniz, tinyGltf, destination, ...extra] = process.argv.slice(2);
if (!destination || extra.length) throw new Error('Usage: node prepare-source.mjs OPENSKP_CHECKOUT MINIZ_CHECKOUT TINYGLTF_CHECKOUT NEW_EXTERNAL_DESTINATION');
const sha = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
const git = (directory, ...args) => execFileSync('git', ['-c', 'core.longpaths=true', '-C', directory, ...args], {encoding:'utf8', maxBuffer:16*1024*1024}).trimEnd();
const sources = [
  {name:'openskp', root:openSkp, pin},
  {name:'miniz', root:miniz, pin:'77d0dce8627735138c51770d1799a1ef48f2117d'},
  {name:'tinygltf', root:tinyGltf, pin:'488a70a3df62a4df1a736e9e56fb8836580c4888'},
];
for (const source of sources) {
  if (git(source.root, 'rev-parse', 'HEAD') !== source.pin) throw new Error(`Wrong ${source.name} commit`);
  if (git(source.root, 'status', '--porcelain', '--untracked-files=no')) throw new Error(`Modified ${source.name} source`);
}
const target = path.resolve(destination);
const external = path.resolve(process.env.LOCALAPPDATA ?? '', 'GreenLens');
if (!process.env.LOCALAPPDATA || !target.toLowerCase().startsWith(external.toLowerCase()+path.sep)) throw new Error('Destination must be a new external LOCALAPPDATA/GreenLens directory');
if (fs.existsSync(target)) throw new Error('Destination exists: preserve it; never overwrite a previous preparation');
for (let parent = path.dirname(target); fs.existsSync(parent); parent = path.dirname(parent)) {
  if (fs.lstatSync(parent).isSymbolicLink()) throw new Error('Reparse/symlink destination ancestry is unsupported');
  if (path.dirname(parent) === parent) break;
}
const manifest = {schema_version:'greenlens.openskp-development-source.v1', source_pins:sources.map(({name,pin})=>({name,pin})), patch_sha256:sha(fs.readFileSync(path.join(here,'patches.mjs'))), files:[]};
const copy = (source, relative, output, patch=false) => {
  const raw = fs.readFileSync(path.join(source.root, relative));
  // CRLF working checkouts are allowed; the exact LF Git blob must still match.
  const blob = execFileSync('git',['-C',source.root,'show',`${source.pin}:${relative.replaceAll(String.fromCharCode(92),'/')}`],{maxBuffer:16*1024*1024});
  const normalized = raw.toString('utf8').replaceAll('\r\n','\n');
  if (normalized !== blob.toString('utf8').replaceAll('\r\n','\n')) throw new Error(`Source changed: ${relative}`);
  const data = Buffer.from(patch ? patchSource(relative.slice('packages/cpp/'.length), normalized) : normalized);
  const dest = path.join(target,output);
  fs.mkdirSync(path.dirname(dest),{recursive:true});
  fs.writeFileSync(dest,data,{flag:'wx'});
  manifest.files.push({source:source.name, path:output.replaceAll(String.fromCharCode(92),'/'), original_sha256:sha(blob), prepared_sha256:sha(data)});
};
fs.mkdirSync(target,{recursive:true});
try {
  const cpp = path.join(openSkp,'packages/cpp');
  const walk = (relative) => {
    for (const entry of fs.readdirSync(path.join(cpp,relative),{withFileTypes:true})) {
      const child = path.join(relative,entry.name);
      if (entry.isSymbolicLink()) throw new Error('Source symlink unsupported');
      if (entry.isDirectory() && entry.name !== '_fragments_fb') walk(child);
      else if (entry.isDirectory()) continue; // Unused ThatOpen/FlatBuffers generated headers.
      else copy(sources[0], `packages/cpp/${child.replaceAll(String.fromCharCode(92),'/')}`,`openskp/${child}`,true);
    }
  };
  walk('include');
  const units = ['core','earcut','errors','face_groups','geometry','instanced_glb','instanced_scene','legacy','model','observability','pages_dimensions','tlv','transforms','triangulator','vff'];
  for (const unit of units) copy(sources[0],`packages/cpp/src/${unit}.cpp`,`openskp/src/${unit}.cpp`,true);
  for (const header of ['internal.hpp','face_groups.hpp']) copy(sources[0],`packages/cpp/src/${header}`,`openskp/src/${header}`,true);
  copy(sources[0],'LICENSE','licenses/LICENSE.openskp');
  for (const file of ['miniz.c','miniz.h','miniz_common.h','miniz_tdef.c','miniz_tdef.h','miniz_tinfl.c','miniz_tinfl.h','miniz_zip.c','miniz_zip.h','LICENSE']) copy(sources[1],file,`miniz/${file}`);
  for (const file of ['tiny_gltf.h','json.hpp','LICENSE']) copy(sources[2],file,`tinygltf/${file}`);
  const limits = fs.readFileSync(path.join(here,'strict_limits.hpp'));
  fs.writeFileSync(path.join(target,'openskp/src/strict_limits.hpp'),limits,{flag:'wx'});
  manifest.strict_limits_sha256=sha(limits);
  manifest.files.sort((a,b)=>a.path.localeCompare(b.path,'en'));
  fs.writeFileSync(path.join(target,'source-manifest.json'),JSON.stringify(manifest,null,2)+'\n',{flag:'wx'});
  console.log(JSON.stringify({schema_version:manifest.schema_version,prepared_files:manifest.files.length,manifest_sha256:sha(fs.readFileSync(path.join(target,'source-manifest.json')))}));
} catch (error) {
  // Failed preparation remains inspectable; do not recursively delete or reuse it.
  fs.writeFileSync(path.join(target,'PREPARATION_FAILED.txt'),String(error)+'\n',{flag:'wx'});
  throw error;
}
