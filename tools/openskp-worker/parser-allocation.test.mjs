// Source-only verification: no SKP read, compiled parser or worker execution.
import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import crypto from 'node:crypto';
import {fileURLToPath} from 'node:url';
import {pin, patchSource} from './patches.mjs';
import {patchParserAllocations} from './parser-allocation-patches.mjs';
const here=path.dirname(fileURLToPath(import.meta.url));
const source=process.env.GREENLENS_OPEN_SKP_AUDIT_SOURCE ?? path.join(process.env.LOCALAPPDATA ?? '', 'GreenLens/dependencies/openskp-audit',pin,'packages/cpp');
const raw= file => fs.readFileSync(path.join(source,file),'utf8').replaceAll('\r\n','\n');
const prepared= file => patchParserAllocations(file,patchSource(file,raw(file)));
for (const file of ['src/legacy.cpp','src/geometry.cpp']) test(`deterministic exact source allocation patch: ${file}`,()=>{
  const original=patchSource(file,raw(file));
  const patched=patchParserAllocations(file,original);
  assert.equal(patched,patchParserAllocations(file,original));
  assert.throws(()=>patchParserAllocations(file,patched),/already present/);
  assert.throws(()=>patchParserAllocations(file,original.replace('#include "internal.hpp"','')),/found 0/);
  assert.throws(()=>patchParserAllocations(file,original+'\n#include "internal.hpp"'),/found 2/);
});
test('legacy source pin hash and ordering are explicit',()=>{
  // LF Git-blob digest; the prior audit's df37... digest describes CRLF workbytes.
  assert.equal(crypto.createHash('sha256').update(raw('src/legacy.cpp')).digest('hex'),'810e3153c80f7f816726e59ae88763c1670522d15118308a4f87459d278faf41');
  const code=prepared('src/legacy.cpp');
  const typed=code.slice(code.indexOf('  ParsedAttribute typed('),code.indexOf('  std::vector<std::tuple<uint64_t',code.indexOf('  ParsedAttribute typed(')));
  assert.ok(typed.indexOf('require_elements(')<typed.indexOf('greenlens_parser::reserve('));
  assert.ok(!typed.includes('items.reserve('));
  assert.match(typed,/greenlens_strict::Depth/);
  const strings=code.slice(code.indexOf('  std::string utf16()'),code.indexOf('// V is now defined'));
  assert.ok(strings.indexOf('require_elements(')<strings.indexOf('construction.charge('));
  assert.ok(strings.indexOf('construction.charge(')<strings.indexOf('s.reserve('));
  assert.match(code,/dictionary_copy\(ev->entries\);\n    out.emplace/);
});
test('modern source validates complete spans before vector construction',()=>{
  assert.equal(crypto.createHash('sha256').update(raw('src/geometry.cpp')).digest('hex'),'f059dae0444e7a6f59552a54b54ae7dbda7a1a2fbcc19194ac6eec6498a850e9');
  const code=prepared('src/geometry.cpp');
  const begin=code.indexOf('static std::vector<std::pair<std::string, std::pair<size_t, size_t>>> parse_flat_spans(');
  const span=code.slice(begin,code.indexOf('// Tags parse_tlv_recursive',begin));
  assert.ok(span.indexOf('a > z || z > p.size()')<span.indexOf('std::size_t count'));
  assert.ok(span.indexOf('cursor != z')<span.indexOf('greenlens_parser::reserve('));
  assert.ok(span.indexOf('greenlens_parser::reserve(')<span.indexOf('out.emplace_back('));
  assert.match(code,/spans.size\(\) != 1/);
  assert.match(code,/Unsupported attribute array child/);
  assert.match(code,/Unsupported or malformed attribute value/);
});
test('modern dictionary and strings admit before construction and never overwrite duplicates',()=>{
  const code=prepared('src/geometry.cpp');
  assert.match(code,/greenlens_parser::text\(reinterpret_cast<const char\*>\(p.data\(\) \+ sa\), sz - sa\)/);
  assert.match(code,/entries.count\(key\).*Duplicate attribute key/);
  assert.match(code,/out.count\(name\).*Duplicate attribute dictionary/);
  assert.match(code,/map_value<ParsedAttrDict>\(\);\n        auto value = decode/);
  assert.ok(!code.includes('entries[key] =') && !code.includes('out[name] ='));
  assert.match(code,/Attribute key lacks a value/);
  assert.match(code,/Attribute value lacks a key/);
});
test('construction budget states exact limits and excludes total heap claims',()=>{
  const header=fs.readFileSync(path.join(here,'parser_allocation_bounds.hpp'),'utf8');
  assert.match(header,/request_limit = 64u \* 1024 \* 1024/);
  assert.match(header,/construction_limit = 256u \* 1024 \* 1024/);
  assert.match(header,/Monotonic across the one parse/);
  assert.match(header,/not allocator overhead/);
  assert.match(header,/inline thread_local ConstructionBudget construction/);
  assert.ok(!header.includes('VirtualAlloc') && !header.includes('malloc('));
});
