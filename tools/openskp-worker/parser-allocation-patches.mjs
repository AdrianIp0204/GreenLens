// Original GreenLens parser-construction patches; Apache-2.0.
// Apply after patches.mjs to the exact pinned source. Upstream quotations are
// MIT OpenSKP source; retain LICENSE.openskp. No parser execution in this module.
import {replaceExactly} from './patches.mjs';

export function patchParserAllocations(path, original) {
  if (!['src/legacy.cpp', 'src/geometry.cpp'].includes(path)) return original;
  const marker = '// GreenLens parser construction admission v1; retain upstream MIT notice.\n';
  if (original.includes(marker.trim())) throw new Error(`${path}: parser patch already present`);
  let text = original.replaceAll('\r\n', '\n');
  const edit = (before, after) => { text = replaceExactly(text, before, after, path); };
  edit('#include "internal.hpp"', '#include "internal.hpp"\n#include "parser_allocation_bounds.hpp"');
  if (path === 'src/legacy.cpp') {
    edit('    need(size_t(z) * 2);\n    std::string s;', `    // Check serialized units before any UTF-8 output reservation. The pinned
    // decoder emits at most three bytes per UTF-16 code unit (unchanged semantics).
    need(0);
    greenlens_parser::require_elements(z, d.size() - p, 2);
    const auto utf8_capacity = greenlens_parser::checked_product(z, 3);
    greenlens_parser::construction.charge(utf8_capacity + 1);
    std::string s;
    s.reserve(utf8_capacity);`);
    edit('        v->entries[key] = typed(r.u8());', `        if (v->entries.count(key)) throw std::runtime_error("Duplicate legacy attribute key");
        greenlens_parser::map_value<ParsedAttrDict>();
        auto value = typed(r.u8());
        v->entries.emplace(std::move(key), std::move(value));`);
    edit('        if (n > 100000) throw std::runtime_error("attr array too large");\n        std::vector<ParsedAttribute> items;\n        items.reserve(n);', `        if (n > 100000) throw std::runtime_error("attr array too large");
        r.need(0);
        // Every child needs a type byte, including Null. Reject before reserve.
        greenlens_parser::require_elements(n, r.d.size() - r.p, 1);
        std::vector<ParsedAttribute> items;
        greenlens_parser::reserve(items, n);`);
    edit('    out[ev->name] = ev->entries;', `    if (out.count(ev->name)) throw std::runtime_error("Duplicate legacy attribute dictionary");
    greenlens_parser::map_value<ParsedAttrDictionaries>();
    greenlens_parser::construction.charge(ev->name.size() + 1);
    greenlens_parser::dictionary_copy(ev->entries);
    out.emplace(ev->name, ev->entries);`);
  } else {
    edit('  std::vector<std::pair<std::string, std::pair<size_t, size_t>>> out;\n  while (a + 6 <= z) {', `  if (a > z || z > p.size()) throw std::runtime_error("Invalid property span range");
  // Validate the complete span and count records before constructing its index.
  std::size_t count = 0, cursor = a;
  while (z - cursor >= 6) {
    const auto n = read_u32(p, cursor + 2);
    if (n > z - cursor - 6) throw std::runtime_error("Truncated property span before allocation");
    ++count;
    cursor += 6 + n;
  }
  if (cursor != z) throw std::runtime_error("Trailing incomplete property span before allocation");
  std::vector<std::pair<std::string, std::pair<size_t, size_t>>> out;
  greenlens_parser::construction.charge(count, 5); // Four-byte tags plus terminators.
  greenlens_parser::reserve(out, count);
  while (z - a >= 6) {`);
    edit('  if (spans.empty()) return ParsedAttribute::null();\n  auto& [tag, span] = spans[0];', `  if (spans.empty()) return ParsedAttribute::null();
  if (spans.size() != 1) throw std::runtime_error("Attribute wrapper must contain exactly one value");
  auto& [tag, span] = spans[0];`);
    edit('        std::string(reinterpret_cast<const char*>(p.data() + sa), sz - sa));', '        greenlens_parser::text(reinterpret_cast<const char*>(p.data() + sa), sz - sa));');
    edit(`    std::vector<ParsedAttribute> items;
    for (auto& [ctag, cspan] : parse_flat_spans(p, sa, sz)) {
      if (ctag != "A438") continue;`, `    auto children = parse_flat_spans(p, sa, sz);
    for (const auto& child : children)
      if (child.first != "A438") throw std::runtime_error("Unsupported attribute array child");
    std::vector<ParsedAttribute> items;
    greenlens_parser::reserve(items, children.size());
    for (auto& [ctag, cspan] : children) {`);
    edit('  return ParsedAttribute::null();\n}\n\n// Extract every attribute dictionary', '  throw std::runtime_error("Unsupported or malformed attribute value");\n}\n\n// Extract every attribute dictionary');
    edit('        key = std::string(reinterpret_cast<const char*>(p.data() + sa), sz - sa);', `        if (!key.empty()) throw std::runtime_error("Attribute key lacks a value");
        key = greenlens_parser::text(reinterpret_cast<const char*>(p.data() + sa), sz - sa);
        if (key.empty()) throw std::runtime_error("Empty attribute key");`);
    edit('      } else if (tag == "A438" && !key.empty()) {', '      } else if (tag == "A438") {\n        if (key.empty()) throw std::runtime_error("Attribute value lacks a key");');
    edit('        entries[key] = decode_a438_value(p, sa, sz);\n        key.clear();', `        if (entries.count(key)) throw std::runtime_error("Duplicate attribute key");
        greenlens_parser::map_value<ParsedAttrDict>();
        auto value = decode_a438_value(p, sa, sz);
        entries.emplace(std::move(key), std::move(value));
        key.clear();`);
    edit('        extract_entries(sa, sz, entries);\n      }\n    }\n  };', '        extract_entries(sa, sz, entries);\n      }\n    }\n    if (!key.empty()) throw std::runtime_error("Attribute key lacks a value");\n  };');
    edit('        std::string name(reinterpret_cast<const char*>(p.data() + sa), sz - sa);', `        auto name = greenlens_parser::text(reinterpret_cast<const char*>(p.data() + sa), sz - sa);
        if (name.empty()) throw std::runtime_error("Empty attribute dictionary name");
        if (out.count(name)) throw std::runtime_error("Duplicate attribute dictionary");
        greenlens_parser::map_value<ParsedAttrDictionaries>();`);
    edit('        out[name] = std::move(entries);', '        out.emplace(std::move(name), std::move(entries));');
  }
  return marker + text;
}
