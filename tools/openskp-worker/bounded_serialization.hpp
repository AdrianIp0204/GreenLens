// Original GreenLens bounded output construction; Apache-2.0.
// Bounds encoded output requests, NOT the JSON DOM, allocator or process memory.
#pragma once
#include <algorithm>
#include <cstddef>
#include <cstdint>
#include <cstring>
#include <iomanip>
#include <limits>
#include <ostream>
#include <stdexcept>
#include <streambuf>
#include <vector>

namespace greenlens_strict {
using EncodedBytes = std::vector<unsigned char>;

class BoundedSink : public std::streambuf {
 public:
  // nullptr means count-only: no output buffer is allocated.
  BoundedSink(unsigned char* destination, std::size_t limit)
      : destination_(destination), limit_(limit) {}
  std::size_t size() const noexcept { return size_; }
 protected:
  std::streamsize xsputn(const char* data, std::streamsize count) override {
    if (count < 0 || static_cast<std::uintmax_t>(count) > limit_ - size_)
      throw std::length_error("Encoded output exceeds its byte budget");
    const auto n = static_cast<std::size_t>(count);
    if (destination_ && n) std::memcpy(destination_ + size_, data, n);
    size_ += n;
    return count;
  }
  int_type overflow(int_type value) override {
    if (traits_type::eq_int_type(value, traits_type::eof())) return traits_type::not_eof(value);
    const char byte = traits_type::to_char_type(value);
    xsputn(&byte, 1);
    return value;
  }
 private:
  unsigned char* destination_;
  std::size_t limit_;
  std::size_t size_ = 0;
};

template<class Json>
std::size_t emit_json(const Json& value, unsigned char* destination,
                      std::size_t limit, int indent = -1, bool newline = false) {
  if (indent != -1 && indent != 2) throw std::invalid_argument("Unsupported JSON indentation");
  BoundedSink sink(destination, limit);
  std::ostream stream(&sink);
  stream.exceptions(std::ios::badbit | std::ios::failbit);
  if (indent >= 0) stream << std::setw(indent);
  stream << value;
  if (newline) stream.put('\n');
  stream.flush();
  return sink.size();
}

template<class Json>
EncodedBytes encode_json(const Json& value, std::size_t limit,
                         int indent = -1, bool newline = false) {
  const auto size = emit_json(value, nullptr, limit, indent, newline);
  EncodedBytes bytes(size); // Exact encoded size admitted before allocation.
  if (emit_json(value, bytes.data(), size, indent, newline) != size)
    throw std::runtime_error("JSON changed between count and write");
  return bytes;
}

inline std::size_t padded4(std::size_t size, std::size_t limit) {
  if (size > limit || ((4 - size % 4) % 4) > limit - size)
    throw std::length_error("GLB padding exceeds byte budget");
  return size + (4 - size % 4) % 4;
}

inline std::size_t glb_size(std::size_t json, std::size_t binary, std::size_t limit) {
  if (limit > std::numeric_limits<std::uint32_t>::max() || limit < 20)
    throw std::length_error("Invalid GLB byte budget");
  const auto j = padded4(json, limit - 20);
  auto total = 20 + j;
  if (binary) {
    if (limit - total < 8) throw std::length_error("GLB chunk exceeds byte budget");
    total += 8;
    total += padded4(binary, limit - total);
  }
  return total;
}

inline void glb_u32(EncodedBytes& bytes, std::size_t at, std::uint32_t value) {
  for (unsigned i = 0; i != 4; ++i) bytes.at(at + i) = static_cast<unsigned char>(value >> (8 * i));
}

template<class Json>
EncodedBytes encode_glb(const Json& json, const EncodedBytes& binary, std::size_t limit) {
  const auto json_size = emit_json(json, nullptr, limit);
  const auto total = glb_size(json_size, binary.size(), limit);
  EncodedBytes bytes(total, 0); // No stream/string/binary copies before admission.
  glb_u32(bytes, 0, 0x46546c67); glb_u32(bytes, 4, 2);
  glb_u32(bytes, 8, static_cast<std::uint32_t>(total));
  const auto padded_json = padded4(json_size, limit);
  glb_u32(bytes, 12, static_cast<std::uint32_t>(padded_json));
  glb_u32(bytes, 16, 0x4e4f534a);
  if (emit_json(json, bytes.data() + 20, json_size) != json_size)
    throw std::runtime_error("GLB JSON changed between count and write");
  std::fill(bytes.begin() + 20 + json_size, bytes.begin() + 20 + padded_json, ' ');
  if (!binary.empty()) {
    const auto at = 20 + padded_json;
    glb_u32(bytes, at, static_cast<std::uint32_t>(padded4(binary.size(), limit)));
    glb_u32(bytes, at + 4, 0x004e4942);
    std::copy(binary.begin(), binary.end(), bytes.begin() + at + 8);
  }
  return bytes;
}
} // namespace greenlens_strict
