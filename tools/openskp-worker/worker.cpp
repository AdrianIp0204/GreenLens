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
#include "bounded_serialization.hpp"
#include "streaming_sha256.hpp"
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
void failure_diagnostic(const char* message) noexcept {
  // Fixed storage and direct pipe write also work after allocator exhaustion.
  // Escape bytes (including non-ASCII) without allocating or emitting invalid JSON.
  std::array<char,8192> buffer{};
  std::size_t size=0;
  auto append=[&](const char* text){while(*text)buffer[size++]=*text++;};
  append("{\"status\":\"rejected\",\"message\":\"");
  constexpr char hex[]="0123456789abcdef";
  std::size_t n=0;
  for(;message && message[n] && n<1024;++n){
    const auto c=static_cast<unsigned char>(message[n]);
    if(c>=32 && c<127 && c!='"' && c!='\\')buffer[size++]=static_cast<char>(c);
    else {append("\\u00");buffer[size++]=hex[c>>4];buffer[size++]=hex[c&15];}
  }
  if(message && n==1024 && message[n])append(" [diagnostic truncated]");
  append("\"}\n");
  DWORD written=0;
  WriteFile(GetStdHandle(STD_ERROR_HANDLE),buffer.data(),static_cast<DWORD>(size),&written,nullptr);
}
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
  return limit::digest_bytes(data, size);
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
  Json root_faces=std::move(faces);
  for(const auto& entry:raw.definitions){
    const auto& definition=entry.second;
    check(!definition.is_image && !definition.always_faces_camera && !definition.shadows_face_sun,"Image/billboard definition unsupported");
    validate_builder(definition.builder,total,faces);
    definitions.push_back({{"source_definition_id",entry.first},{"name",definition.name}});
    definitions.back()["faces"]=std::move(faces);
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
  Json result={{"source_entity_count",total},{"placed_instance_count",placed},{"conservative_expanded_triangle_bound",triangle_bound},{"model_display_units",raw.units?Json(*raw.units):Json(nullptr)}};
  result["root_face_inventory"]=std::move(root_faces);
  result["definition_inventory"]=std::move(definitions);
  return result;
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
  Json result={{"node_count",nodes},{"expanded_triangles",expanded},{"bounds_gltf_m",{{"min",lo},{"max",hi}}},{"layer_hidden",scene.layer_hidden}};
  result["occurrences"]=std::move(occurrences);
  return result;
}
} // namespace

int wmain(int argc,wchar_t**argv){
  try{
    check(argc==4,"Usage: worker INPUT_SKP NEW_EMPTY_STAGE EXPECTED_SOURCE_SHA256");
    require_job();
    const fs::path source(argv[1]),stage(argv[2]);
    const std::wstring wide_hash(argv[3]);
    check(wide_hash.size()==64 && std::all_of(wide_hash.begin(),wide_hash.end(),[](wchar_t c){return(c>=L'0'&&c<=L'9')||(c>=L'a'&&c<=L'f');}),"Expected lowercase SHA-256 required");
    std::string expected;expected.reserve(64);
    for(const auto c:wide_hash)expected.push_back(static_cast<char>(c)); // ASCII checked above.
    Handle directory(CreateFileW(stage.c_str(),GENERIC_READ,FILE_SHARE_READ,nullptr,OPEN_EXISTING,FILE_FLAG_BACKUP_SEMANTICS|FILE_FLAG_OPEN_REPARSE_POINT,nullptr));
    FILE_ATTRIBUTE_TAG_INFO attributes{};
    check(GetFileInformationByHandleEx(directory.value,FileAttributeTagInfo,&attributes,sizeof(attributes)) && (attributes.FileAttributes&FILE_ATTRIBUTE_DIRECTORY) && !(attributes.FileAttributes&FILE_ATTRIBUTE_REPARSE_POINT),"Stage must be a non-reparse directory");
    check(fs::is_empty(stage),"Stage must be empty; never overwrite previous evidence");
    Handle input(CreateFileW(source.c_str(),GENERIC_READ,FILE_SHARE_READ,nullptr,OPEN_EXISTING,FILE_FLAG_OPEN_REPARSE_POINT|FILE_FLAG_SEQUENTIAL_SCAN,nullptr));
    Bytes bytes=read_source(input.value);
    check(digest(bytes)==expected,"Source hash mismatch");
    const auto source_size=bytes.size();
    const std::string version=openskp::extract_version(bytes);
    check(version=="{13.0.1}" || version=="{26.2.243}","Encoded version outside the fixed independent matrix");
    std::size_t logged=0;
    openskp::ParseOptions options;
    options.log=[&](openskp::LogLevel,std::string_view message){
      // Bounded diagnostics contain source metadata; retain privately, never public automatically.
      if(message.find("Failed")!=std::string_view::npos)throw std::runtime_error("Upstream best-effort failure is fatal in strict worker");
      // Admit worst-case JSON escaping before copying source text into the DOM.
      const auto remaining=limit::log_bytes-8192-logged;
      check(remaining>=64 && message.size()<=(remaining-64)/6,"Worker log budget exceeded");
      const auto line=limit::encode_json(Json{{"stage","parser"},{"message",message}},remaining,-1,true);
      logged+=line.size();
      std::cerr.write(reinterpret_cast<const char*>(line.data()),static_cast<std::streamsize>(line.size()));
      check(static_cast<bool>(std::cerr),"Worker diagnostic pipe failed");
    };
    options.progress=[](const openskp::ParseProgress&){limit::check_time();};
    auto raw=openskp::full_parse(bytes,options);
    Bytes().swap(bytes); // Release original byte copy before geometry/output assembly.
    Json source_inventory=validate_raw(raw);
    auto scene=openskp::build_instanced_scene_raw(std::move(raw),options);
    raw={}; // The rvalue-reference API does not itself destroy the parsed owner.
    Json metadata={{"schema_version","greenlens.openskp-development-candidate.v1"},{"status","unaccepted_candidate"},{"source_sha256",expected},{"encoded_version",version},{"source_pin","dceb9fff0efc9668961530b535d5375c001343dc"},{"source_units","SketchUp stored inches; output metres"},{"axes","right-handed glTF Y-up: (X,Z,-Y) from source Z-up"},{"north","unknown"},{"omissions",Json::array({"Texture and watermark image bytes; no image decoding","Saved application cameras/styles/pages and arbitrary property dictionaries are not reference geometry","Face-level visibility is inventoried but does not suppress triangles"})},{"qualification","No reader or physical qualification; independent fixture and Rust admission required"}};
    metadata["source"]=std::move(source_inventory);
    metadata["scene"]=inspect_scene(scene);
    openskp::InstancedGlbOptions glb_options;glb_options.textures=false;
    const auto glb=openskp::to_instanced_glb(scene,glb_options);
    scene={}; // All retained occurrence data now belongs to metadata.
    check(glb.size()<=limit::entry_bytes,"Candidate GLB exceeds 64 MiB");
    metadata["candidate_glb_sha256"]=digest(glb);
    const auto json=limit::encode_json(metadata,limit::metadata_bytes,2,true);
    metadata=Json(); // Encoded evidence replaces, rather than duplicates, this DOM.
    check(limit::digest_held_file(input.value,source_size,[]{limit::check_time();})==expected,"Source changed while parsing");
    limit::check_time();
    write_new(stage/L"candidate.glb",glb.data(),glb.size());
    write_new(stage/L"metadata.json",json.data(),json.size());
    // Completion is last. Parent must independently fence bytes and validate GLB;
    // absence of this file always means incomplete, never a project result.
    const auto completion=limit::encode_json(Json{{"schema_version","greenlens.openskp-development-completion.v1"},{"status","unaccepted_candidate"},{"source_sha256",expected},{"glb_sha256",digest(glb)},{"metadata_sha256",digest(json)},{"glb_bytes",glb.size()},{"metadata_bytes",json.size()}},8192,2,true);
    write_new(stage/L"completion.json",completion.data(),completion.size());
    std::cout.write(reinterpret_cast<const char*>(completion.data()),static_cast<std::streamsize>(completion.size()));
    check(static_cast<bool>(std::cout),"Worker completion pipe failed");
    return 0;
  }catch(const std::exception&error){
    // The failure fallback must not allocate another JSON tree after bad_alloc.
    // Detailed diagnostics are emitted earlier only when their budget admits them.
    failure_diagnostic(error.what());
    return 2;
  }catch(...){failure_diagnostic("unknown worker failure");return 3;}
}
