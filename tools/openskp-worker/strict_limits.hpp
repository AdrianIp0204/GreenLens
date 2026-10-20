// GreenLens development-only OpenSKP guard additions. Apache-2.0.
#pragma once
#include <chrono>
#include <cstddef>
#include <stdexcept>
#include <string>
#include "preallocation_bounds.h"

namespace greenlens_strict {
inline constexpr std::size_t source_bytes = 128u * 1024 * 1024;
inline constexpr std::size_t entry_bytes = GREENLENS_GLB_BYTE_LIMIT;
inline constexpr std::size_t zip_bytes = 256u * 1024 * 1024;
inline constexpr std::size_t zip_entries = GREENLENS_ZIP_ENTRY_LIMIT;
inline constexpr std::size_t entities = 50000;
inline constexpr std::size_t max_depth = 64;
inline constexpr std::size_t triangles = 1000000;
inline constexpr std::size_t metadata_bytes = 4u * 1024 * 1024;
inline constexpr std::size_t log_bytes = 1024u * 1024;
inline thread_local std::size_t depth = 0;
inline thread_local std::size_t records = 0;
inline thread_local auto started = std::chrono::steady_clock::now();
inline void check_time() {
  if (std::chrono::steady_clock::now() - started > std::chrono::seconds(120))
    throw std::runtime_error("GreenLens worker deadline exceeded");
}
inline void count(std::size_t n = 1) {
  check_time();
  if (n > entities - records) throw std::runtime_error("GreenLens source record budget exceeded");
  records += n;
}
struct Depth {
  Depth() {
    check_time();
    if (depth >= max_depth) throw std::runtime_error("GreenLens parser depth budget exceeded");
    ++depth;
  }
  ~Depth() { --depth; }
  Depth(const Depth&) = delete;
  Depth& operator=(const Depth&) = delete;
};
inline void require(bool condition, const char* message) {
  if (!condition) throw std::runtime_error(message);
}
}  // namespace greenlens_strict
