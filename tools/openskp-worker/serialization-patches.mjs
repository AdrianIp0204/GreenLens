// Original development-only output-copy repair. Retain upstream MIT notices.
import {replaceExactly} from './patches.mjs';
export function patchSerialization(file, original) {
  if (file !== 'src/instanced_glb.cpp') return original;
  let text = replaceExactly(original, '#include <openskp/instanced_glb.hpp>',
    '#include <openskp/instanced_glb.hpp>\n#include "bounded_serialization.hpp"', file);
  text=replaceExactly(text,'  auto model = make_model(scene, options.textures);',
    '  if (options.textures) throw std::runtime_error("Strict worker omits textures before export construction");\n  auto model = make_model(scene, false);',file);
  const start = text.indexOf('  std::ostringstream stream(std::ios::binary | std::ios::out);');
  const end = text.indexOf('\n}\n\nvoid export_instanced_glb', start);
  if (start < 0 || end < 0) throw new Error('Pinned GLB serialization body unavailable');
  const before = text.slice(start, end);
  if (!before.endsWith('  return ByteBuffer(serialized.begin(), serialized.end());'))
    throw new Error('Pinned GLB serialization suffix changed');
  return replaceExactly(text, before, `  // The development worker exports the texture-free static surface subset only.
  // Keep TinyGLTF's model serializer; bound output before requesting storage.
  // This does not bound make_model/JsonDocument allocations or native commitment.
  if (options.textures || !model.images.empty() || model.buffers.size() != 1 ||
      !model.buffers[0].uri.empty())
    throw std::runtime_error("Strict worker requires one embedded buffer and no images");
  gltf::detail::JsonDocument output;
  gltf::SerializeGltfModel(&model, output);
  gltf::detail::json buffers;
  gltf::detail::json buffer;
  gltf::SerializeNumberProperty("byteLength", model.buffers[0].data.size(), buffer);
  if (!model.buffers[0].name.empty())
    gltf::SerializeStringProperty("name", model.buffers[0].name, buffer);
  gltf::SerializeExtrasAndExtensions(model.buffers[0], buffer);
  gltf::detail::JsonPushBack(buffers, std::move(buffer));
  gltf::detail::JsonAddMember(output, "buffers", std::move(buffers));
  return greenlens_strict::encode_glb(output, model.buffers[0].data, kGlbLimit);`, file);
}
