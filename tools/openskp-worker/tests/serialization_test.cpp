// Original synthetic output/hash tests; Apache-2.0. No SKP parser/worker.
#define TINYGLTF_IMPLEMENTATION
#define TINYGLTF_NO_STB_IMAGE
#define TINYGLTF_NO_STB_IMAGE_WRITE
#include "tiny_gltf.h"
#include "bounded_serialization.hpp"
#include "streaming_sha256.hpp"
#include <filesystem>
#include <functional>
#include <iostream>
#include <sstream>
namespace {
using Json = nlohmann::json;
using namespace greenlens_strict;
std::size_t checks=0;
void check(bool value,const char* message) { ++checks; if(!value)throw std::runtime_error(message); }
void rejects(const std::function<void()>& work) {
  bool failed=false; try { work(); } catch(const std::exception&) { failed=true; }
  check(failed,"Expected bounded rejection");
}
std::string string(const EncodedBytes& bytes) { return {bytes.begin(),bytes.end()}; }
void json_checks() {
  const std::vector<Json> fixtures={nullptr,true,0,-1,0.1,
    Json{{"label","窗 \"quote\"\n\\"},{"values",{0,1.25,-1e300,1e-300}},{"empty",Json::array()}},
    std::string("a\0b\1\t",6),std::string(2049,'x')};
  for(const auto& fixture:fixtures) for(int indent:{-1,2})for(bool newline:{false,true}) {
    const auto expected=fixture.dump(indent)+(newline?"\n":"");
    const auto actual=encode_json(fixture,expected.size(),indent,newline);
    check(string(actual)==expected,"Streaming JSON differs from pinned serializer");
    rejects([&]{encode_json(fixture,expected.size()-1,indent,newline);});
  }
  const Json invalid=std::string("\xff",1);
  rejects([&]{encode_json(invalid,1024);});
  rejects([&]{encode_json(Json{},128,3);});
  rejects([&]{encode_json(Json{},128,0);});
  std::array<unsigned char,4> fence{0x77,0x77,0x77,0x77};
  BoundedSink sink(fence.data(),2); std::ostream out(&sink);
  out.exceptions(std::ios::badbit|std::ios::failbit);
  out.write("ab",2); rejects([&]{out.write("cd",2);});
  check(sink.size()==2 && fence[0]=='a' && fence[1]=='b' && fence[2]==0x77 && fence[3]==0x77,
        "Bounded output overwrote destination after failed admission");
  rejects([&]{glb_size(SIZE_MAX,0,64*1024*1024);});
  rejects([&]{glb_size(1,SIZE_MAX,64*1024*1024);});
  rejects([&]{glb_size(1,1,19);});
}
void glb_checks() {
  // The old pinned TinyGLTF writer is a compatibility oracle, not a geometry oracle.
  // Explicit chunks/JSON below independently check framing, byte lengths and order.
  for(std::size_t n=0;n<9;++n) {
    tinygltf::Model model;
    model.asset.version="2.0";model.asset.generator="Original GreenLens byte fixture";
    model.scenes.emplace_back();model.scenes[0].nodes={0};model.defaultScene=0;
    model.nodes.emplace_back();model.nodes[0].name="Repeated 窗";model.nodes[0].children={1,2};
    model.nodes.emplace_back();model.nodes[1].name="Repeated 窗";model.nodes[1].translation={1,2,3};
    model.nodes.emplace_back();model.nodes[2].scale={-1,2,0.5};
    model.buffers.emplace_back();for(std::size_t i=0;i<n;++i)model.buffers[0].data.push_back(static_cast<unsigned char>(i+1));
    std::ostringstream original(std::ios::binary|std::ios::out);tinygltf::TinyGLTF writer;
    check(writer.WriteGltfSceneToStream(&model,original,false,true),"Pinned old writer failed");
    tinygltf::detail::JsonDocument output;
    tinygltf::SerializeGltfModel(&model,output);
    tinygltf::detail::json buffers,buffer;
    tinygltf::SerializeNumberProperty("byteLength",model.buffers[0].data.size(),buffer);
    tinygltf::detail::JsonPushBack(buffers,std::move(buffer));
    tinygltf::detail::JsonAddMember(output,"buffers",std::move(buffers));
    const auto expected=original.str();
    const auto actual=encode_glb(output,model.buffers[0].data,expected.size());
    check(string(actual)==expected,"GLB differs from pinned writer on original fixture");
    auto u32=[&](std::size_t at){std::uint32_t value=0;for(unsigned i=0;i<4;++i)value|=std::uint32_t(actual.at(at+i))<<(8*i);return value;};
    check(u32(0)==0x46546c67 && u32(4)==2 && u32(8)==actual.size() && actual.size()%4==0,"Invalid GLB header");
    check(u32(16)==0x4e4f534a,"Wrong JSON chunk type");
    const auto decoded=Json::parse(actual.begin()+20,actual.begin()+20+u32(12));
    check(decoded.at("buffers").at(0).at("byteLength")==n,"Wrong binary length");
    check(decoded.at("nodes").at(0).at("children")==Json::array({1,2}),"Node order changed");
    if(n) {const auto at=20+u32(12);check(u32(at+4)==0x004e4942 && u32(at)==((n+3)/4)*4,"Wrong BIN chunk");}
    rejects([&]{encode_glb(output,model.buffers[0].data,expected.size()-1);});
  }
}
void hash_checks(const std::filesystem::path& folder) {
  check(digest_bytes(nullptr,0)=="e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855","Empty SHA-256 mismatch");
  check(digest_bytes(reinterpret_cast<const unsigned char*>("abc"),3)=="ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad","ABC SHA-256 mismatch");
  const auto file=folder/L"hash fixture 空間.bin";
  HANDLE handle=CreateFileW(file.c_str(),GENERIC_READ|GENERIC_WRITE,FILE_SHARE_READ,nullptr,CREATE_NEW,FILE_ATTRIBUTE_NORMAL,nullptr);
  check(handle!=INVALID_HANDLE_VALUE,"Expected new external hash fixture");
  struct Close {HANDLE h;~Close(){CloseHandle(h);}} close{handle};
  EncodedBytes bytes(2*65536+17);for(std::size_t i=0;i<bytes.size();++i)bytes[i]=static_cast<unsigned char>(i*37);
  DWORD written=0;check(WriteFile(handle,bytes.data(),static_cast<DWORD>(bytes.size()),&written,nullptr) && written==bytes.size(),"Fixture write failed");
  std::size_t checkpoints=0;
  const auto expected=digest_bytes(bytes.data(),bytes.size());
  check(digest_held_file(handle,bytes.size(),[&]{++checkpoints;})==expected && checkpoints==4,"Streamed hash/chunk checkpoints differ");
  rejects([&]{digest_held_file(handle,bytes.size()-1,[]{});});
  rejects([&]{digest_held_file(handle,bytes.size(),[]{throw std::runtime_error("cancelled checkpoint");});});
  LARGE_INTEGER at{};check(SetFilePointerEx(handle,at,nullptr,FILE_BEGIN)!=0,"Fixture seek failed");
  const unsigned char changed=255;check(WriteFile(handle,&changed,1,&written,nullptr)!=0,"Fixture edit failed");
  check(digest_held_file(handle,bytes.size(),[]{})!=expected,"Changed source bytes not detected");
}
}
int wmain(int argc,wchar_t** argv) {
  try {
    check(argc==2,"Pass a new existing external unit-test directory");
    json_checks();glb_checks();hash_checks(argv[1]);
    std::cout<<"{\"schema_version\":\"greenlens.serialization-unit.v1\",\"assertions\":"<<checks
             <<",\"json_fixtures\":8,\"glb_fixtures\":9,\"skp_parses\":0,\"worker_executions\":0,\"containment_qualified\":false}\n";
    return 0;
  } catch(const std::exception& error) {std::cerr<<error.what()<<'\n';return 1;}
}
