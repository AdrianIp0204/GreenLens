// Original GreenLens Windows hash helper; Apache-2.0.
// A held input is rehashed in fixed 64 KiB chunks, never a second whole buffer.
#pragma once
#include <windows.h>
#include <bcrypt.h>
#include <algorithm>
#include <array>
#include <cstddef>
#include <cstdint>
#include <stdexcept>
#include <string>
#include <vector>
namespace greenlens_strict {
class Sha256 {
 public:
  Sha256() {
    if (BCryptOpenAlgorithmProvider(&algorithm_, BCRYPT_SHA256_ALGORITHM, nullptr, 0) < 0)
      throw std::runtime_error("SHA-256 provider unavailable");
    try {
      DWORD bytes = 0, written = 0;
      if (BCryptGetProperty(algorithm_, BCRYPT_OBJECT_LENGTH, reinterpret_cast<PUCHAR>(&bytes),
          sizeof(bytes), &written, 0) < 0 || written != sizeof(bytes) || !bytes || bytes > 65536)
        throw std::runtime_error("SHA-256 context outside bounded provider contract");
      object_.resize(bytes);
      if (BCryptCreateHash(algorithm_, &hash_, object_.data(), bytes, nullptr, 0, 0) < 0)
        throw std::runtime_error("SHA-256 context unavailable");
    } catch (...) { BCryptCloseAlgorithmProvider(algorithm_, 0); throw; }
  }
  ~Sha256() { BCryptDestroyHash(hash_); BCryptCloseAlgorithmProvider(algorithm_, 0); }
  Sha256(const Sha256&) = delete;
  Sha256& operator=(const Sha256&) = delete;
  void add(const unsigned char* data, std::size_t size) {
    while (size) {
      const auto n = static_cast<ULONG>(std::min<std::size_t>(size, 65536));
      if (BCryptHashData(hash_, const_cast<PUCHAR>(data), n, 0) < 0)
        throw std::runtime_error("SHA-256 update failed");
      data += n; size -= n;
    }
  }
  std::string finish() {
    std::array<unsigned char, 32> result{};
    if (BCryptFinishHash(hash_, result.data(), static_cast<ULONG>(result.size()), 0) < 0)
      throw std::runtime_error("SHA-256 finalization failed");
    constexpr char hex[] = "0123456789abcdef";
    std::string out(64, '0');
    for (std::size_t i = 0; i < result.size(); ++i) {
      out[2*i] = hex[result[i] >> 4]; out[2*i+1] = hex[result[i] & 15];
    }
    return out;
  }
 private:
  BCRYPT_ALG_HANDLE algorithm_ = nullptr;
  BCRYPT_HASH_HANDLE hash_ = nullptr;
  std::vector<unsigned char> object_;
};
inline std::string digest_bytes(const unsigned char* data, std::size_t size) {
  Sha256 hash; hash.add(data, size); return hash.finish();
}
template<class Checkpoint>
std::string digest_held_file(HANDLE file, std::size_t expected, Checkpoint checkpoint) {
  LARGE_INTEGER size{}, start{};
  if (!GetFileSizeEx(file, &size) || size.QuadPart < 0 ||
      static_cast<unsigned long long>(size.QuadPart) != expected ||
      !SetFilePointerEx(file, start, nullptr, FILE_BEGIN))
    throw std::runtime_error("Held source size changed or seek failed");
  Sha256 hash;
  std::array<unsigned char, 65536> chunk{};
  std::size_t read = 0;
  while (read < expected) {
    checkpoint();
    DWORD n = 0;
    const auto request = static_cast<DWORD>(std::min(chunk.size(), expected - read));
    if (!ReadFile(file, chunk.data(), request, &n, nullptr) || n == 0)
      throw std::runtime_error("Held source read incomplete");
    hash.add(chunk.data(), n); read += n;
  }
  DWORD extra = 0;
  if (!ReadFile(file, chunk.data(), 1, &extra, nullptr) || extra != 0 ||
      !GetFileSizeEx(file, &size) || static_cast<unsigned long long>(size.QuadPart) != expected)
    throw std::runtime_error("Held source length changed during hashing");
  checkpoint();
  return hash.finish();
}
} // namespace greenlens_strict
