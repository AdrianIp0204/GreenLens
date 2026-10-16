// Original GreenLens strictness modifications; Apache-2.0.
// Anchors below quote pinned MIT OpenSKP source; retain LICENSE.openskp.
export const pin = 'dceb9fff0efc9668961530b535d5375c001343dc';
export function replaceExactly(text, before, after, label) {
  const count = text.split(before).length - 1;
  if (count !== 1) throw new Error(`${label}: expected one pinned anchor, found ${count}`);
  return text.replace(before, after);
}
export function patchSource(path, original) {
  const marker = '// GreenLens strict development patch v1; retain upstream MIT notice.\n';
  if (original.includes(marker.trim())) throw new Error(`${path}: patched anchor already present`);
  let text = original.replaceAll('\r\n', '\n');
  const edit = (before, after) => { text = replaceExactly(text, before, after, path); };
  if (path === 'src/internal.hpp') {
    edit('#pragma once', '#pragma once\n#include "strict_limits.hpp"');
  }
  if (path === 'src/core.cpp') {
    edit('    auto n = mz_zip_reader_get_num_files(&z);\n    for', `    auto n = mz_zip_reader_get_num_files(&z);
    if (n > greenlens_strict::zip_entries) throw std::runtime_error("ZIP entry count exceeded");
    std::uint64_t declared_total = 0;
    std::set<std::string> unique_names;
    for`);
    edit('      if (mz_zip_reader_file_stat(&z, i, &s) && !s.m_is_directory) names.emplace_back(s.m_filename);', `      if (!mz_zip_reader_file_stat(&z, i, &s)) throw std::runtime_error("Invalid ZIP directory record");
      validate_entry_size(s);
      if (s.m_uncomp_size > greenlens_strict::zip_bytes - declared_total)
        throw std::runtime_error("ZIP aggregate declared bytes exceeded");
      declared_total += s.m_uncomp_size;
      if (!unique_names.emplace(s.m_filename).second) throw std::runtime_error("Duplicate ZIP member");
      if (!s.m_is_directory) names.emplace_back(s.m_filename);`);
    edit('  std::vector<std::string> names;', '  std::vector<std::string> names;\n  std::size_t extracted_bytes = 0;');
    edit('    int i = mz_zip_reader_locate_file', `    // Geometry/material XML only. No image bytes, decoder, watermark or thumbnail extraction.
    if (name != "model.dat" && name != "meta/meta.dat" &&
        !(name.size() >= 4 && name.substr(name.size()-4) == ".xml")) return {};
    int i = mz_zip_reader_locate_file`);
    edit('    if (!mz_zip_reader_file_stat(&z, i, &s)) return {};', '    if (!mz_zip_reader_file_stat(&z, i, &s)) throw std::runtime_error("Invalid ZIP member metadata");');
    edit('    validate_entry_size(s);\n    ByteBuffer b', `    validate_entry_size(s);
    if (s.m_uncomp_size > greenlens_strict::zip_bytes - extracted_bytes)
      throw std::runtime_error("ZIP aggregate extraction budget exceeded");
    extracted_bytes += static_cast<std::size_t>(s.m_uncomp_size);
    ByteBuffer b`);
    edit('    if (!mz_zip_reader_extract_to_mem(&z, i, b.data(), b.size(), 0)) return {};', '    if (!mz_zip_reader_extract_to_mem(&z, i, b.data(), b.size(), 0)) throw std::runtime_error("ZIP extraction failed");');
    edit('16ull * 1024 * 1024 * 1024;  // 16 GB', 'greenlens_strict::entry_bytes;  // GreenLens 64 MiB');
    edit('  for (auto p = start; p + 6 <= end;) {', '  if (start > end || end > d.size()) throw std::runtime_error("Invalid header range");\n  auto p = start;\n  for (; p + 6 <= end;) {');
    edit('    if (n > end - p - 6) break;', '    if (n > end - p - 6) throw std::runtime_error("Truncated header record");');
    edit('  return h;\n}', '  if (p != end) throw std::runtime_error("Trailing incomplete header record");\n  return h;\n}');
    edit('        continue;\n      }\n      tag = one[0].tag;', '        throw std::runtime_error("Empty parsed model record");\n      }\n      tag = one[0].tag;');
  }
  if (path === 'src/tlv.cpp') {
    edit('  if (end > d.size() || start > end)', '  greenlens_strict::Depth strict_depth;\n  if (end > d.size() || start > end)');
    edit('    if (size > end - p - 6) break;', '    if (size > end - p - 6) throw std::runtime_error("Truncated TLV record");\n    greenlens_strict::count();');
    edit('  return out;\n}\n\nstd::vector<std::pair', '  if (p != end) throw std::runtime_error("Trailing incomplete TLV record");\n  return out;\n}\n\nstd::vector<std::pair');
    edit('    if (n > d.size() - p - 6) break;', '    if (n > d.size() - p - 6) throw std::runtime_error("Truncated flat TLV record");\n    greenlens_strict::check_time();');
    edit('  return out;\n}\n}  // namespace openskp', '  if (p != d.size()) throw std::runtime_error("Trailing incomplete flat TLV record");\n  return out;\n}\n}  // namespace openskp');
  }
  if (path === 'src/geometry.cpp') {
    edit('    if (n > z - a - 6) break;', '    if (n > z - a - 6) throw std::runtime_error("Truncated property TLV span");');
    edit('  return out;\n}\n\n// Tags parse_tlv_recursive', '  if (a != z) throw std::runtime_error("Trailing incomplete property span");\n  return out;\n}\n\n// Tags parse_tlv_recursive');
    edit('static ParsedAttribute decode_a438_value(const ByteBuffer& p, size_t a, size_t z) {', 'static ParsedAttribute decode_a438_value(const ByteBuffer& p, size_t a, size_t z) {\n  greenlens_strict::Depth strict_depth;');
    edit('  extract_entries = [&](size_t a, size_t z, ParsedAttrDict& entries) {', '  extract_entries = [&](size_t a, size_t z, ParsedAttrDict& entries) {\n    greenlens_strict::Depth strict_depth;');
    edit('  walk_dicts = [&](size_t a, size_t z) {', '  walk_dicts = [&](size_t a, size_t z) {\n    greenlens_strict::Depth strict_depth;');
    edit('void collect_geometry(const std::vector<TlvNode>& es, GeometryBuilder& b) {', 'void collect_geometry(const std::vector<TlvNode>& es, GeometryBuilder& b) {\n  greenlens_strict::Depth strict_depth;');
    edit('      if (id && p && p->payload.size() >= 24)', '      if (!id || !p || p->payload.size() != 24 || b.vertices.count(*id)) throw std::runtime_error("Missing or duplicate source vertex");\n      if (id && p && p->payload.size() >= 24)');
    edit('    } else if (e.tag == "B80B") {\n      auto id = entity_id(e);', '    } else if (e.tag == "B80B") {\n      auto id = entity_id(e);\n      if (!id || b.edges.count(*id)) throw std::runtime_error("Missing or duplicate source edge identity");');
    edit('    } else if (e.tag == "AC0D") {\n      auto id = entity_id(e);', '    } else if (e.tag == "AC0D") {\n      auto id = entity_id(e);\n      if (!id || b.faces.count(*id)) throw std::runtime_error("Missing or duplicate source face identity");');
    edit('              if (eid && ori) co.push_back({*eid, *ori});', '              if (!eid || !ori) throw std::runtime_error("Incomplete coedge");\n              co.push_back({*eid, *ori});');
    edit('            if (!co.empty()) f.loops.push_back(std::move(co));', '            if (co.empty()) throw std::runtime_error("Empty face loop");\n            f.loops.push_back(std::move(co));');
  }
  if (path === 'src/legacy.cpp') {
    edit('  uint64_t alloc(Entry e) {', '  uint64_t alloc(Entry e) {\n    greenlens_strict::count();');
    edit('      std::optional<std::string> expect = {}) {\n    auto tag', '      std::optional<std::string> expect = {}) {\n    greenlens_strict::Depth strict_depth;\n    auto tag');
    edit('  ParsedAttribute typed(uint8_t t) {', '  ParsedAttribute typed(uint8_t t) {\n    greenlens_strict::Depth strict_depth;');
    edit('      v.blob = r.raw(z);', '      r.need(z); r.p += z; // GreenLens: omit image payload before allocation.');
    edit('      v->blob = r.raw(z);', '      r.need(z); r.p += z; // GreenLens: omit image payload before allocation.');
    const begin = text.indexOf('    burn_stack.push_back(0);', text.indexOf('entity_list(uint32_t'));
    const end = text.indexOf('\n    return v;', begin);
    if (begin < 0 || end < 0) throw new Error('Pinned legacy list anchor absent');
    const before = text.slice(begin, end);
    edit(before, `    if (n > greenlens_strict::entities) throw std::runtime_error("Legacy entity count exceeded");
    greenlens_strict::Depth strict_depth;
    std::vector<std::tuple<uint64_t, std::string, std::shared_ptr<V>>> v;
    v.reserve(n);
    for (uint32_t index = 0; index < n; ++index) {
      greenlens_strict::check_time();
      v.push_back(object()); // No catch, burn-credit truncation or prefix success.
    }`);
    edit('    if (!v) continue;\n    if (v->k == "edge")', '    if (!v) throw std::runtime_error("Null or unsupported drawing entity");\n    if (v->k == "edge")');
    edit('            co.push_back({EntityId(u->edge), u->sense ? -1 : 1});\n          }', '            co.push_back({EntityId(u->edge), u->sense ? -1 : 1});\n          } else throw std::runtime_error("Unresolved face edge");');
    edit('      b.construction_points.push_back(std::move(cp));\n    }\n  }\n}', '      b.construction_points.push_back(std::move(cp));\n    } else throw std::runtime_error("Unsupported drawing entity in strict surface subset");\n  }\n}');
  }
  if (path === 'src/face_groups.cpp') {
    edit('    if (i == b.edges.end()) continue;', '    if (i == b.edges.end()) throw std::runtime_error("Missing face edge");');
    edit('    if (id && (v.empty() || v.back() != *id)) v.push_back(*id);', '    if (!id || !b.vertices.count(*id)) throw std::runtime_error("Missing edge vertex");\n    if (v.empty() || v.back() != *id) v.push_back(*id);');
    edit('      if (!x.empty()) loops.push_back(std::move(x));', '      if (x.size() < 3) throw std::runtime_error("Degenerate face loop");\n      loops.push_back(std::move(x));');
    edit('    if (loops.empty()) continue;', '    if (loops.empty()) throw std::runtime_error("Missing face loops");');
    edit('    const auto triangles = triangulate_face_3d(vv, loops, f.normal);', '    const auto triangles = triangulate_face_3d(vv, loops, f.normal);\n    if (triangles.empty()) throw std::runtime_error("Empty face triangulation");');
  }
  if (path === 'src/triangulator.cpp') {
    edit('      if (i == verts.end()) return {};', '      if (i == verts.end()) throw std::runtime_error("Unresolved triangulation vertex");');
  }
  if (path === 'src/earcut.cpp') {
    const start = text.indexOf('    if (!cut) {');
    const end = text.indexOf('\n  }\n  if (p.size()', start);
    if (start < 0 || end < 0) throw new Error('Pinned ear clipping fallback absent');
    edit(text.slice(start, end), '    if (!cut) throw std::runtime_error("Triangulation stalled; no eroding fallback permitted");');
    edit('  if (p.size() == 3 &&', '  if (p.size() != 3) throw std::runtime_error("Incomplete triangulation");\n  if (p.size() == 3 &&');
    edit('  if (loops.size() > 2 && holes_overlap(loops)) return triangulate_by_filtering_holes(loops);', '  if (loops.size() > 2 && holes_overlap(loops)) throw std::runtime_error("Overlapping holes unsupported; no centroid approximation");');
  }
  if (path === 'include/openskp/instanced_scene.hpp') {
    edit('struct InstancedNode {\n  std::string name;', 'struct InstancedNode {\n  bool source_hidden{false}; // GreenLens: display only; geometry remains present.\n  std::string name;');
  }
  if (path === 'src/instanced_scene.cpp') {
    edit('    std::vector<InstancedNode> nodes;\n    for (auto& i', '    greenlens_strict::Depth strict_depth;\n    std::vector<InstancedNode> nodes;\n    for (auto& i');
    edit('      const auto new_matrix = multiply_matrices', '      if (instance_counter >= greenlens_strict::entities) throw std::runtime_error("Placed instance budget exceeded");\n      const auto new_matrix = multiply_matrices');
    edit('      node.name = display_name;', '      node.source_hidden = i.hidden;\n      node.name = display_name;');
    edit('          active.erase(*i.ref_idx);\n        }\n      }', '          active.erase(*i.ref_idx);\n        } else throw std::runtime_error("Unresolved component definition");\n      } else throw std::runtime_error("Missing component reference");');
  }
  if (path === 'src/instanced_glb.cpp') {
    edit('constexpr std::size_t kGlbLimit = std::numeric_limits<std::uint32_t>::max();', 'constexpr std::size_t kGlbLimit = 64u * 1024 * 1024; // GreenLens candidate limit');
  }
  if (path === 'src/vff.cpp') {
    edit('std::optional<std::string> read_meta_units(const ByteBuffer& meta_bytes) {', 'std::optional<std::string> read_meta_units(const ByteBuffer& meta_bytes) {\n  greenlens_strict::Depth strict_depth;');
  }
  return text === original.replaceAll('\r\n','\n') ? text : marker + text;
}
