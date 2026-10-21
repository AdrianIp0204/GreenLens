// Original GreenLens parser construction admission. Apache-2.0.
// This bounds requested logical payload construction, not allocator overhead,
// aggregate process commitment, or source/placed geometry. No containment claim.
#pragma once
#include <cstddef>
#include <limits>
#include <stdexcept>
#include <string>
#include <vector>

namespace greenlens_parser {
inline constexpr std::size_t request_limit = 64u * 1024 * 1024;
inline constexpr std::size_t construction_limit = 256u * 1024 * 1024;

inline std::size_t checked_product(std::size_t count, std::size_t width) {
  if (width && count > std::numeric_limits<std::size_t>::max() / width)
    throw std::runtime_error("Parser construction size overflow");
  return count * width;
}
inline void require_elements(std::size_t count, std::size_t available,
                             std::size_t minimum_serialized_width) {
  if (!minimum_serialized_width || count > available / minimum_serialized_width)
    throw std::runtime_error("Truncated parser collection before allocation");
}
class ConstructionBudget {
 public:
  void charge(std::size_t count, std::size_t width = 1) {
    const auto bytes = checked_product(count, width);
    if (bytes > request_limit || bytes > construction_limit - used_)
      throw std::runtime_error("Parser construction budget exceeded before allocation");
    used_ += bytes;
  }
  std::size_t used() const { return used_; }
 private:
  // Monotonic across the one parse: even freed/replaced temporary values count.
  // This prevents repeated overlapping dictionary walks from resetting budget.
  std::size_t used_ = 0;
};
inline thread_local ConstructionBudget construction;

template<class T>
void reserve(std::vector<T>& out, std::size_t count) {
  if (!out.empty() || out.capacity())
    throw std::runtime_error("Parser bounded reserve requires a new collection");
  construction.charge(count, sizeof(T));
  out.reserve(count);
}
inline std::string text(const char* data, std::size_t size) {
  if (size == std::numeric_limits<std::size_t>::max())
    throw std::runtime_error("Parser string size overflow");
  construction.charge(size + 1);
  return std::string(data, size);
}
template<class Map>
void map_value() {
  // std::map's node/allocator overhead is deliberately not called payload.
  construction.charge(1, sizeof(typename Map::value_type));
}
template<class Attribute>
void attribute_copy(const Attribute& value, std::size_t depth = 0) {
  if (depth >= 64) throw std::runtime_error("Parser attribute copy depth exceeded");
  if (value.text.size() == std::numeric_limits<std::size_t>::max())
    throw std::runtime_error("Parser string size overflow");
  construction.charge(value.text.size() + 1);
  construction.charge(value.array.size(), sizeof(Attribute));
  for (const auto& child : value.array) attribute_copy(child, depth + 1);
}
template<class Dict>
void dictionary_copy(const Dict& dict) {
  for (const auto& entry : dict) {
    map_value<Dict>();
    if (entry.first.size() == std::numeric_limits<std::size_t>::max())
      throw std::runtime_error("Parser string size overflow");
    construction.charge(entry.first.size() + 1);
    attribute_copy(entry.second);
  }
}
} // namespace greenlens_parser
