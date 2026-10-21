// Original GreenLens in-memory exporter integration fixtures; Apache-2.0.
// Links the exact patched instanced_glb.cpp. No parser, worker, SKP file,
// containment protocol, independent SKP oracle or private input is executed.
#include <openskp/instanced_glb.hpp>
#include <json.hpp>
#include <array>
#include <cmath>
#include <cstdint>
#include <cstring>
#include <functional>
#include <iostream>
#include <limits>
#include <stdexcept>
#include <string>
#include <vector>

namespace {
using Json = nlohmann::json;
using Bytes = openskp::ByteBuffer;
using Matrix = std::array<double,16>;
std::size_t checks=0;
void check(bool value,const char* message) {
  ++checks; if(!value)throw std::runtime_error(message);
}
void rejects_with(const std::function<void()>& work,const char* expected) {
  bool failed=false;
  try { work(); } catch(const std::exception& error) {
    failed=true;
    check(std::string(error.what()).find(expected)!=std::string::npos,
          "Rejection occurred for an unexpected reason");
  }
  check(failed,"Expected exporter rejection");
}
std::uint32_t u32(const Bytes& bytes,std::size_t at) {
  check(at<=bytes.size() && bytes.size()-at>=4,"Truncated unsigned integer");
  std::uint32_t value=0;
  for(unsigned i=0;i<4;++i)value|=std::uint32_t(bytes.at(at+i))<<(8*i);
  return value;
}
float f32(const Bytes& bytes,std::size_t at) {
  const auto bits=u32(bytes,at);
  float value{};static_assert(sizeof(value)==sizeof(bits),"Expected float32");
  std::memcpy(&value,&bits,sizeof(value));return value;
}
Matrix identity() {return {1,0,0,0,0,1,0,0,0,0,1,0,0,0,0,1};}
Matrix multiply(const Matrix& a,const Matrix& b) {
  Matrix result{};
  for(int col=0;col<4;++col)for(int row=0;row<4;++row)
    for(int k=0;k<4;++k)result[col*4+row]+=a[k*4+row]*b[col*4+k];
  return result;
}
std::array<double,3> point(const Matrix& matrix,const std::array<double,3>& input) {
  std::array<double,3> output{};
  for(int row=0;row<3;++row)
    output[row]=matrix[row]*input[0]+matrix[4+row]*input[1]+matrix[8+row]*input[2]+matrix[12+row];
  return output;
}
Matrix node_matrix(const Json& node) {
  if(!node.contains("matrix"))return identity();
  check(node.at("matrix").size()==16,"Matrix must retain all 16 values");
  return node.at("matrix").get<Matrix>();
}

openskp::InstancedScene triangle_scene() {
  // Coordinates are already normalized glTF metres/Y-up. These fixtures do
  // not claim to establish any SKP decoding or source-unit conversion.
  openskp::InstancedScene scene;
  openskp::GltfMaterial material;
  material.pbr_metallic_roughness.base_color_factor={0.2,0.4,0.6,1.0};
  material.pbr_metallic_roughness.metallic_factor=0.1;
  material.pbr_metallic_roughness.roughness_factor=0.65;
  material.double_sided=true;
  scene.gltf_materials.push_back(material);
  openskp::LocalPrimitive primitive;
  primitive.positions={0,0,0,2,0,0,0,3,0};
  primitive.normals={0,0,1,0,0,1,0,0,1};
  primitive.uvs={0,0,1,0,0,1};primitive.indices={0,1,2};primitive.material_index=0;
  openskp::InstancedMeshResource mesh;
  mesh.id="original-triangle-resource";mesh.definition_name="Triangle definition";
  mesh.primitives.push_back(primitive);scene.mesh_resources.push_back(mesh);
  scene.scene_hierarchy.name="Original fixture root";
  openskp::InstancedNode group;group.name="Nested group";
  group.matrix=identity();group.matrix[12]=10;group.matrix[13]=20;group.matrix[14]=30;
  openskp::InstancedNode nested;nested.name="Repeated 窓";
  nested.mesh_resource_id=mesh.id;nested.matrix=identity();
  nested.matrix[0]=-1;nested.matrix[5]=2;nested.matrix[10]=0.5;
  group.children.push_back(nested);scene.scene_hierarchy.children.push_back(group);
  openskp::InstancedNode repeated;repeated.name="Repeated 窓";
  repeated.mesh_resource_id=mesh.id;repeated.matrix=identity();
  repeated.matrix[12]=5;repeated.matrix[13]=-2;repeated.matrix[14]=1;
  scene.scene_hierarchy.children.push_back(repeated);
  return scene;
}

void exported_geometry() {
  auto scene=triangle_scene();
  const auto first=openskp::to_instanced_glb(scene);
  const auto second=openskp::to_instanced_glb(scene);
  check(first==second,"Pure in-memory export is not deterministic");
  check(first.size()<=64u*1024*1024 && first.size()%4==0,"Invalid total GLB byte count");
  check(u32(first,0)==0x46546c67 && u32(first,4)==2 && u32(first,8)==first.size(),"Invalid GLB header");
  const auto json_length=u32(first,12);
  check(u32(first,16)==0x4e4f534a && json_length%4==0,"Invalid JSON chunk");
  check(json_length<=first.size()-20,"JSON extends beyond GLB");
  const auto json=Json::parse(first.begin()+20,first.begin()+20+json_length);
  const std::size_t bin_header=20+json_length;
  check(u32(first,bin_header)==108 && u32(first,bin_header+4)==0x004e4942,"Expected one 108-byte BIN chunk");
  check(bin_header+8+108==first.size(),"Unexpected trailing or omitted GLB bytes");
  check(json.at("asset").at("version")=="2.0","Wrong glTF version");
  check(json.at("buffers").size()==1 && json.at("buffers").at(0).at("byteLength")==108,"Geometry buffer length changed");
  check(!json.contains("images") && !json.contains("textures"),"Texture-free fixture acquired image output");
  check(json.at("scene")==0 && json.at("scenes").at(0).at("nodes")==Json::array({0}),"Scene root changed");
  const auto& nodes=json.at("nodes");
  check(nodes.size()==4,"Nested/repeated occurrences were flattened or merged");
  check(nodes.at(0).at("children")==Json::array({1,3}) && nodes.at(1).at("children")==Json::array({2}),"Parent-first node hierarchy changed");
  check(nodes.at(2).at("name")=="Repeated 窓" && nodes.at(3).at("name")=="Repeated 窓","Duplicate Unicode occurrence names changed");
  check(!nodes.at(0).contains("matrix"),"Identity root was not serialized consistently");
  check(!nodes.at(1).contains("mesh"),"Grouping node acquired geometry");
  check(nodes.at(2).at("mesh")==0 && nodes.at(3).at("mesh")==0,"Repeated placements do not share the same mesh");
  const auto world=multiply(node_matrix(nodes.at(1)),node_matrix(nodes.at(2)));
  check(point(world,{2,3,0})==std::array<double,3>{8,26,30},"Nested mirrored/nonuniform transform changed");
  check(point(node_matrix(nodes.at(3)),{2,3,0})==std::array<double,3>{7,1,1},"Repeated occurrence transform changed");
  const auto& meshes=json.at("meshes");
  check(meshes.size()==1 && meshes.at(0).at("primitives").size()==1,"Reusable geometry was duplicated");
  const auto& primitive=meshes.at(0).at("primitives").at(0);
  check(primitive.at("mode")==4 && primitive.at("material")==0 && primitive.at("indices")==3,"Primitive topology/material changed");
  check(primitive.at("attributes").at("POSITION")==0 && primitive.at("attributes").at("NORMAL")==1 && primitive.at("attributes").at("TEXCOORD_0")==2,"Attribute accessor ordering changed");
  const std::array<std::size_t,4> offsets={0,36,72,96}, lengths={36,36,24,12};
  const std::array<const char*,4> types={"VEC3","VEC3","VEC2","SCALAR"};
  check(json.at("bufferViews").size()==4 && json.at("accessors").size()==4,"Accessor/view count changed");
  for(std::size_t i=0;i<4;++i) {
    const auto& view=json.at("bufferViews").at(i);const auto& accessor=json.at("accessors").at(i);
    check(view.at("buffer")==0 && view.value("byteOffset",std::size_t{0})==offsets[i] && view.at("byteLength")==lengths[i],"Unexpected binary view layout");
    check(accessor.at("bufferView")==i && accessor.at("count")==3 && accessor.at("type")==types[i] && accessor.at("componentType")== (i==3?5125:5126),"Unexpected accessor contract");
  }
  check(json.at("accessors").at(0).at("min")==Json::array({0,0,0}) && json.at("accessors").at(0).at("max")==Json::array({2,3,0}),"Definition-local bounds changed");
  const auto& original=scene.mesh_resources.at(0).primitives.at(0);
  const std::array<const std::vector<float>*,3> vectors={&original.positions,&original.normals,&original.uvs};
  for(std::size_t attribute=0;attribute<vectors.size();++attribute)
    for(std::size_t i=0;i<vectors[attribute]->size();++i)
      check(f32(first,bin_header+8+offsets[attribute]+4*i)==vectors[attribute]->at(i),"Local vertex/normal/UV bytes changed or were transform-baked");
  for(std::size_t i=0;i<3;++i)check(u32(first,bin_header+8+96+4*i)==i,"Triangle index changed");
  const auto& output_material=json.at("materials").at(0);
  const auto& pbr=output_material.at("pbrMetallicRoughness");
  check(output_material.at("doubleSided")==true && pbr.at("baseColorFactor")==Json::array({0.2,0.4,0.6,1.0}),"Material colour/sidedness changed");
  check(pbr.at("metallicFactor")==0.1 && pbr.at("roughnessFactor")==0.65,"Material factors changed");
  check(scene.mesh_resources.size()==1 && scene.scene_hierarchy.children.size()==2 && original.positions==std::vector<float>({0,0,0,2,0,0,0,3,0}),"Exporter mutated the source scene");
}
void rejected_inputs() {
  auto invalid=triangle_scene();
  invalid.mesh_resources.at(0).primitives.at(0).positions.at(0)=std::numeric_limits<float>::quiet_NaN();
  openskp::InstancedGlbOptions textures;textures.textures=true;
  // The intentional nonfinite geometry would fail make_model validation first
  // if the unsupported option were not rejected ahead of construction.
  rejects_with([&]{openskp::to_instanced_glb(invalid,textures);},"omits textures before export construction");
  rejects_with([&]{openskp::to_instanced_glb(invalid);},"must be finite");
  invalid=triangle_scene();invalid.mesh_resources.at(0).primitives.at(0).indices.at(2)=3;
  rejects_with([&]{openskp::to_instanced_glb(invalid);},"index is out of range");
  invalid=triangle_scene();invalid.mesh_resources.at(0).primitives.at(0).normals.pop_back();
  rejects_with([&]{openskp::to_instanced_glb(invalid);},"normals must match positions");
}
} // namespace
int main() {
  try {
    exported_geometry();rejected_inputs();
    std::cout<<"{\"schema_version\":\"greenlens.instanced-export-unit.v1\",\"assertions\":"<<checks
      <<",\"in_memory_scenes\":1,\"rejected_inputs\":4,\"skp_parses\":0,\"worker_executions\":0,\"containment_qualified\":false}\n";
    return 0;
  } catch(const std::exception& error) {std::cerr<<error.what()<<'\n';return 1;}
}
