// Original GreenLens development worker; Apache-2.0. NOT an application importer.
// All parser code is the pinned external MIT source plus reviewed strict patches.
#include <windows.h>
#include <bcrypt.h>
#include <algorithm>
#include <array>
#include <cmath>
#include <filesystem>
#include <iostream>
#include <limits>
#include <map>
#include <set>
#include <string>
#include <vector>
#include <json.hpp>
#include "internal.hpp"
#include <openskp/instanced_glb.hpp>

namespace {
using Json = nlohmann::json;
using Bytes = openskp::ByteBuffer;
namespace fs = std::filesystem;
namespace limit = greenlens_strict;
using Matrix = std::array<double, 16>;
struct Handle {
  HANDLE value = INVALID_HANDLE_VALUE;
  explicit Handle(HANDLE h) : value(h) {
    if (h == INVALID_HANDLE_VALUE) throw std::runtime_error("Windows file handle unavailable");
  }
  ~Handle() { CloseHandle(value); }
  Handle(const Handle&) = delete;
};
void check(bool condition, const char* message) { limit::require(condition, message); }
void require_job() {
  BOOL in_job = FALSE;
  check(IsProcessInJob(GetCurrentProcess(), nullptr, &in_job) && in_job, "Supervised Job Object required");
  JOBOBJECT_EXTENDED_LIMIT_INFORMATION info{};
  check(QueryInformationJobObject(nullptr, JobObjectExtendedLimitInformation, &info, sizeof(info), nullptr), "Cannot inspect containing job limits");
  const auto flags = info.BasicLimitInformation.LimitFlags;
  check((flags & JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE) && (flags & JOB_OBJECT_LIMIT_JOB_MEMORY), "Kill-on-close and hard aggregate committed-memory job limits required");
  check(info.JobMemoryLimit > 0 && info.JobMemoryLimit <= 1024ull*1024*1024, "Job committed-memory limit must be at most 1 GiB");
}
std::string digest(const std::uint8_t* data, std::size_t size) {
  BCRYPT_ALG_HANDLE algorithm = nullptr;
  BCRYPT_HASH_HANDLE hash = nullptr;
  check(BCryptOpenAlgorithmProvider(&algorithm, BCRYPT_SHA256_ALGORITHM, nullptr, 0) >= 0, "SHA-256 provider unavailable");
  struct CloseAlgorithm { BCRYPT_ALG_HANDLE h; ~CloseAlgorithm(){ BCryptCloseAlgorithmProvider(h,0); } } ca{algorithm};
  DWORD object_size=0, written=0;
  check(BCryptGetProperty(algorithm,BCRYPT_OBJECT_LENGTH,reinterpret_cast<PUCHAR>(&object_size),sizeof(object_size),&written,0)>=0, "SHA-256 property unavailable");
  std::vector<std::uint8_t> object(object_size);
  check(BCryptCreateHash(algorithm,&hash,object.data(),object_size,nullptr,0,0)>=0,"SHA-256 context unavailable");
  struct CloseHash { BCRYPT_HASH_HANDLE h; ~CloseHash(){ BCryptDestroyHash(h); } } ch{hash};
  check(size <= std::numeric_limits<ULONG>::max(),"Hash input too large");
  check(BCryptHashData(hash,const_cast<PUCHAR>(data),static_cast<ULONG>(size),0)>=0,"SHA-256 update failed");
  std::array<std::uint8_t,32> result{};
  check(BCryptFinishHash(hash,result.data(),static_cast<ULONG>(result.size()),0)>=0,"SHA-256 finalization failed");
  constexpr char hex[]="0123456789abcdef";
  std::string out;
  for(auto byte:result){out.push_back(hex[byte>>4]);out.push_back(hex[byte&15]);}
  return out;
}
std::string digest(const Bytes& b){return digest(b.data(),b.size());}
std::string digest(const std::string& s){return digest(reinterpret_cast<const std::uint8_t*>(s.data()),s.size());}
Bytes read_source(HANDLE file) {
  FILE_ATTRIBUTE_TAG_INFO attributes{};
  check(GetFileInformationByHandleEx(file,FileAttributeTagInfo,&attributes,sizeof(attributes)),"Input attributes unavailable");
  check(!(attributes.FileAttributes & (FILE_ATTRIBUTE_DIRECTORY|FILE_ATTRIBUTE_REPARSE_POINT)),"Input must be a regular non-reparse file");
  LARGE_INTEGER size{};
  check(GetFileSizeEx(file,&size) && size.QuadPart>0 && size.QuadPart<=limit::source_bytes,"Input is empty or exceeds 128 MiB");
  Bytes result(static_cast<std::size_t>(size.QuadPart));
  LARGE_INTEGER start{};
  check(SetFilePointerEx(file,start,nullptr,FILE_BEGIN),"Cannot seek input");
  std::size_t offset=0;
  while(offset<result.size()){
    DWORD n=0;
    const auto request=static_cast<DWORD>(std::min<std::size_t>(1024*1024,result.size()-offset));
    check(ReadFile(file,result.data()+offset,request,&n,nullptr) && n>0,"Input read incomplete");
    offset+=n;
  }
  return result;
}
void write_new(const fs::path& path, const std::uint8_t* data, std::size_t size) {
  Handle file(CreateFileW(path.c_str(),GENERIC_WRITE,0,nullptr,CREATE_NEW,FILE_ATTRIBUTE_NORMAL,nullptr));
  std::size_t offset=0;
  while(offset<size){
    DWORD n=0;
    const auto request=static_cast<DWORD>(std::min<std::size_t>(1024*1024,size-offset));
    check(WriteFile(file.value,data+offset,request,&n,nullptr) && n>0,"Candidate write incomplete");
    offset+=n;
  }
  check(FlushFileBuffers(file.value),"Candidate flush failed");
}
void write_new(const fs::path& path,const std::string& data){write_new(path,reinterpret_cast<const std::uint8_t*>(data.data()),data.size());}
void finite(double value){check(std::isfinite(value),"Nonfinite geometry");}
void add_count(std::size_t& count,std::size_t n){check(n<=limit::entities-count,"Source/placed entity budget exceeded");count+=n;}

// Independently reject missing topology before invoking the upstream triangulator.
// This is structural validation, not independent validation of unknown SKP decoding.
void validate_builder(const openskp::GeometryBuilder& b,std::size_t& total,Json& inventory) {
  add_count(total,b.vertices.size());add_count(total,b.edges.size());add_count(total,b.faces.size());add_count(total,b.instances.size());
  check(b.section_planes.empty() && b.texts.empty() && b.dimensions.empty() && b.construction_lines.empty() && b.construction_points.empty(),"Unsupported non-surface drawing entities; candidate is not complete");
  for(const auto& vertex:b.vertices)for(double value:vertex.second)finite(value);
  std::set<openskp::EntityId> used_edges;
  inventory=Json::array();
  for(const auto& entry:b.faces){
    const auto& f=entry.second;
    check(!f.loops.empty(),"Face has no outer loop");
    for(double value:f.normal)finite(value);
    const double normal_length=std::hypot(f.normal[0],f.normal[1],f.normal[2]);
    check(normal_length>1e-12,"Degenerate source face normal");
    Json loop_sizes=Json::array();
    for(const auto& loop:f.loops){
      check(loop.size()>=3 && loop.size()<=4096,"Unsupported/degenerate face loop size");
      std::optional<openskp::EntityId> first,last;
      for(const auto& co:loop){
        auto e=b.edges.find(co.edge_id);
        check(e!=b.edges.end() && e->second.first && e->second.second,"Unresolved source edge");
        check(co.orientation==1 || co.orientation==-1,"Invalid coedge orientation");
        auto a=co.orientation==1?*e->second.first:*e->second.second;
        auto z=co.orientation==1?*e->second.second:*e->second.first;
        check(a!=z && b.vertices.count(a) && b.vertices.count(z),"Missing or zero-length source edge");
        if(last)check(*last==a,"Noncontiguous face loop");else first=a;
        last=z;used_edges.insert(co.edge_id);
      }
      check(first==last,"Unclosed face loop");loop_sizes.push_back(loop.size());
    }
    inventory.push_back({{"source_face_id",entry.first},{"loop_edge_counts",loop_sizes},{"hidden",f.hidden},{"layer_identifier",f.layer}});
  }
  check(used_edges.size()==b.edges.size(),"Loose edges are not represented by this surface-only candidate");
  for(const auto& instance:b.instances){
    check(instance.matrix.size()==13 && instance.ref_idx,"Missing instance transform or definition");
    for(double value:instance.matrix)finite(value);
    check(std::abs(openskp::transform_determinant(instance.matrix))>1e-12,"Singular instance transform");
    check(instance.name.size()<=4096 && instance.layer.size()<=4096,"Instance text exceeds metadata bound");
  }
}
Json validate_raw(const openskp::RawParsed& raw){
  std::size_t total=0;
  add_count(total,raw.definitions.size());
  Json faces, definitions=Json::array();
  validate_builder(raw.root.builder,total,faces);
  Json root_faces=faces;
  for(const auto& entry:raw.definitions){
    const auto& definition=entry.second;
    check(!definition.is_image && !definition.always_faces_camera && !definition.shadows_face_sun,"Image/billboard definition unsupported");
    validate_builder(definition.builder,total,faces);
    definitions.push_back({{"source_definition_id",entry.first},{"name",definition.name},{"faces",faces}});
  }
  std::set<openskp::EntityId> active;
  std::size_t placed=0;
  std::size_t triangle_bound=0;
  std::function<void(const openskp::GeometryBuilder&,std::size_t)> walk;
  walk=[&](const openskp::GeometryBuilder& b,std::size_t depth){
    check(depth<=limit::max_depth,"Placement depth exceeded");
    for(const auto& entry:b.faces){
      const auto& loops=entry.second.loops;
      std::size_t vertices=0;for(const auto& loop:loops)vertices+=loop.size();
      // Conservative two-sided upper bound before mesh expansion. A lower exact
      // count does not permit exceeding this predeclared development admission.
      const auto bound=2*(vertices+2*(loops.size()-1)-2);
      check(bound<=limit::triangles-triangle_bound,"Expanded triangle upper bound exceeded before triangulation");
      triangle_bound+=bound;
    }
    for(const auto& instance:b.instances){
      add_count(placed,1);
      auto found=raw.definitions.find(*instance.ref_idx);
      check(found!=raw.definitions.end(),"Missing placed definition");
      check(active.emplace(*instance.ref_idx).second,"Recursive placed definition");
      walk(found->second.builder,depth+1);active.erase(*instance.ref_idx);
    }
  };
  walk(raw.root.builder,0);
  return {{"source_entity_count",total},{"placed_instance_count",placed},{"conservative_expanded_triangle_bound",triangle_bound},{"root_face_inventory",root_faces},{"definition_inventory",definitions},{"model_display_units",raw.units?Json(*raw.units):Json(nullptr)}};
}
Matrix multiply(const Matrix&a,const Matrix&b){
  Matrix out{};for(int col=0;col<4;++col)for(int row=0;row<4;++row)for(int k=0;k<4;++k)out[col*4+row]+=a[k*4+row]*b[col*4+k];return out;
}
Json inspect_scene(const openskp::InstancedScene& scene){
  std::map<std::string,const openskp::InstancedMeshResource*> resources;
  for(const auto& r:scene.mesh_resources)check(resources.emplace(r.id,&r).second,"Duplicate mesh resource identity");
  std::size_t expanded=0,nodes=0;
  Json occurrences=Json::array();
  std::array<double,3> lo{INFINITY,INFINITY,INFINITY},hi{-INFINITY,-INFINITY,-INFINITY};
  std::function<void(const openskp::InstancedNode&,const Matrix&,const std::string&,std::size_t)> walk;
  walk=[&](const openskp::InstancedNode&node,const Matrix&parent,const std::string&path,std::size_t depth){
    check(depth<=limit::max_depth,"Scene depth exceeded");add_count(nodes,1);
    for(double value:node.matrix)finite(value);
    const Matrix world=multiply(parent,node.matrix);
    const std::size_t glb_index=nodes-1; // Pinned emit_node reserves parent before recursive children.
    Json resource=node.mesh_resource_id?Json(*node.mesh_resource_id):Json(nullptr);
    occurrences.push_back({{"occurrence_path",path},{"glb_node_index",glb_index},{"name",node.name},{"definition_name",node.definition_name},{"source_guid",node.guid},{"local_matrix_gltf",node.matrix},{"world_matrix_gltf",world},{"source_hidden",node.source_hidden},{"layer",node.layer},{"tag_hidden",scene.layer_hidden.count(node.layer)?scene.layer_hidden.at(node.layer):false},{"mesh_resource_id",resource}});
    if(node.mesh_resource_id){
      auto found=resources.find(*node.mesh_resource_id);check(found!=resources.end(),"Unresolved scene mesh");
      for(const auto& primitive:found->second->primitives){
        check(primitive.positions.size()%3==0 && primitive.indices.size()%3==0,"Invalid primitive shape");
        const auto triangles=primitive.indices.size()/3;
        check(triangles<=limit::triangles-expanded,"Expanded triangle budget exceeded");expanded+=triangles;
        for(auto index:primitive.indices)check(index<primitive.positions.size()/3,"Invalid primitive index");
        for(std::size_t i=0;i<primitive.positions.size();i+=3){
          for(int axis=0;axis<3;++axis){
            const auto v=world[axis]*primitive.positions[i]+world[4+axis]*primitive.positions[i+1]+world[8+axis]*primitive.positions[i+2]+world[12+axis];
            finite(v);lo[axis]=std::min(lo[axis],v);hi[axis]=std::max(hi[axis],v);
          }
        }
      }
    }
    for(std::size_t i=0;i<node.children.size();++i)walk(node.children[i],world,path+"/"+std::to_string(i),depth+1);
  };
  Matrix identity{1,0,0,0,0,1,0,0,0,0,1,0,0,0,0,1};
  walk(scene.scene_hierarchy,identity,"0",0);
  check(expanded>0,"No supported surface triangles");
  return {{"node_count",nodes},{"expanded_triangles",expanded},{"occurrences",occurrences},{"bounds_gltf_m",{{"min",lo},{"max",hi}}},{"layer_hidden",scene.layer_hidden}};
}
} // namespace

int wmain(int argc,wchar_t**argv){
  try{
    check(argc==4,"Usage: worker INPUT_SKP NEW_EMPTY_STAGE EXPECTED_SOURCE_SHA256");
    require_job();
    const fs::path source(argv[1]),stage(argv[2]);
    const std::wstring wide_hash(argv[3]);
    check(wide_hash.size()==64 && std::all_of(wide_hash.begin(),wide_hash.end(),[](wchar_t c){return(c>=L'0'&&c<=L'9')||(c>=L'a'&&c<=L'f');}),"Expected lowercase SHA-256 required");
    const std::string expected(wide_hash.begin(),wide_hash.end());
    Handle directory(CreateFileW(stage.c_str(),GENERIC_READ,FILE_SHARE_READ,nullptr,OPEN_EXISTING,FILE_FLAG_BACKUP_SEMANTICS|FILE_FLAG_OPEN_REPARSE_POINT,nullptr));
    FILE_ATTRIBUTE_TAG_INFO attributes{};
    check(GetFileInformationByHandleEx(directory.value,FileAttributeTagInfo,&attributes,sizeof(attributes)) && (attributes.FileAttributes&FILE_ATTRIBUTE_DIRECTORY) && !(attributes.FileAttributes&FILE_ATTRIBUTE_REPARSE_POINT),"Stage must be a non-reparse directory");
    check(fs::is_empty(stage),"Stage must be empty; never overwrite previous evidence");
    Handle input(CreateFileW(source.c_str(),GENERIC_READ,FILE_SHARE_READ,nullptr,OPEN_EXISTING,FILE_FLAG_OPEN_REPARSE_POINT|FILE_FLAG_SEQUENTIAL_SCAN,nullptr));
    const Bytes bytes=read_source(input.value);
    check(digest(bytes)==expected,"Source hash mismatch");
    const std::string version=openskp::extract_version(bytes);
    check(version=="{13.0.1}" || version=="{26.2.243}","Encoded version outside the fixed independent matrix");
    std::size_t logged=0;
    openskp::ParseOptions options;
    options.log=[&](openskp::LogLevel,std::string_view message){
      // Bounded diagnostics contain source metadata; retain privately, never public automatically.
      if(message.find("Failed")!=std::string_view::npos)throw std::runtime_error("Upstream best-effort failure is fatal in strict worker");
      const auto line=Json{{"stage","parser"},{"message",message}}.dump()+'\n';
      // Count encoded bytes, including JSON escaping; reserve the final error receipt.
      check(line.size()<=limit::log_bytes-8192-logged,"Worker log budget exceeded");logged+=line.size();
      std::cerr<<line;
    };
    options.progress=[](const openskp::ParseProgress&){limit::check_time();};
    auto raw=openskp::full_parse(bytes,options);
    Json source_inventory=validate_raw(raw);
    auto scene=openskp::build_instanced_scene_raw(std::move(raw),options);
    Json metadata={{"schema_version","greenlens.openskp-development-candidate.v1"},{"status","unaccepted_candidate"},{"source_sha256",expected},{"encoded_version",version},{"source_pin","dceb9fff0efc9668961530b535d5375c001343dc"},{"source_units","SketchUp stored inches; output metres"},{"axes","right-handed glTF Y-up: (X,Z,-Y) from source Z-up"},{"north","unknown"},{"source",source_inventory},{"scene",inspect_scene(scene)},{"omissions",Json::array({"Texture and watermark image bytes; no image decoding","Saved application cameras/styles/pages and arbitrary property dictionaries are not reference geometry","Face-level visibility is inventoried but does not suppress triangles"})},{"qualification","No reader or physical qualification; independent fixture and Rust admission required"}};
    openskp::InstancedGlbOptions glb_options;glb_options.textures=false;
    const auto glb=openskp::to_instanced_glb(scene,glb_options);
    check(glb.size()<=limit::entry_bytes,"Candidate GLB exceeds 64 MiB");
    metadata["candidate_glb_sha256"]=digest(glb);
    const auto json=metadata.dump(2)+'\n';
    check(json.size()<=limit::metadata_bytes,"Candidate metadata exceeds 4 MiB");
    check(digest(read_source(input.value))==expected,"Source changed while parsing");
    limit::check_time();
    write_new(stage/L"candidate.glb",glb.data(),glb.size());
    write_new(stage/L"metadata.json",json);
    // Completion is last. Parent must independently fence bytes and validate GLB;
    // absence of this file always means incomplete, never a project result.
    const auto completion=Json{{"schema_version","greenlens.openskp-development-completion.v1"},{"status","unaccepted_candidate"},{"source_sha256",expected},{"glb_sha256",digest(glb)},{"metadata_sha256",digest(json)},{"glb_bytes",glb.size()},{"metadata_bytes",json.size()}}.dump(2)+'\n';
    write_new(stage/L"completion.json",completion);
    std::cout<<completion;
    return 0;
  }catch(const std::exception&error){
    std::string message=error.what();if(message.size()>4096)message.resize(4096);
    std::cerr<<Json{{"status","rejected"},{"message",message}}.dump()<<'\n';
    return 2;
  }catch(...){std::cerr<<"{\"status\":\"rejected\",\"message\":\"unknown worker failure\"}\n";return 3;}
}
