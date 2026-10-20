// Original low-resource synthetic buffer/ZIP tests; Apache-2.0.
// No OpenSKP code, SKP file, worker, converter, renderer or engine is executed.
#include <array>
#include <cstdint>
#include <cstdlib>
#include <iostream>
#include <limits>
#include <stdexcept>
#include <string>
#include <vector>
#include "preallocation_bounds.h"
#include "miniz.h"

namespace {
std::size_t checks = 0;
void check(bool value, const char* message) {
  ++checks;
  if (!value) throw std::runtime_error(message);
}

void glb_bounds() {
  const auto maximum = std::numeric_limits<std::size_t>::max();
  std::size_t aligned = 17, result = 19;
  auto reject = [&](std::size_t current, std::size_t count, std::size_t width) {
    aligned = 17; result = 19;
    check(!greenlens_glb_append_size(current, count, width, &aligned, &result),
          "Oversize/overflow append admitted");
    check(aligned == 17 && result == 19, "Failed admission changed outputs");
  };
  reject(maximum, 1, 1);
  reject(0, maximum, 4);
  reject(0, 1, maximum);
  reject(0, 0, 0);
  reject(GREENLENS_GLB_BYTE_LIMIT + std::size_t{1}, 0, 1);
  reject(GREENLENS_GLB_BYTE_LIMIT - 1, 1, 1);
  reject(GREENLENS_GLB_BYTE_LIMIT - 3, 1, 4);
  check(greenlens_glb_append_size(GREENLENS_GLB_BYTE_LIMIT - 4, 1, 4,
                                  &aligned, &result) &&
        aligned == GREENLENS_GLB_BYTE_LIMIT - 4 &&
        result == GREENLENS_GLB_BYTE_LIMIT, "Exact limit rejected");
  check(greenlens_glb_append_size(GREENLENS_GLB_BYTE_LIMIT - 1, 0, 1,
                                  &aligned, &result) &&
        result == GREENLENS_GLB_BYTE_LIMIT, "Valid padding-only append rejected");
  check(!greenlens_glb_append_size(0, 1, 4, nullptr, &result), "Null output admitted");
  check(!greenlens_glb_append_size(0, 1, 4, &aligned, nullptr), "Null output admitted");

  // Independently evaluate small alignment and aggregate appends without large
  // allocation. This includes one-byte arrays and every possible padding size.
  for (std::size_t current = 0; current < 32; ++current) {
    for (std::size_t count = 0; count < 32; ++count) {
      for (std::size_t width : {std::size_t{1}, std::size_t{4}, std::size_t{8}}) {
        const auto expected_aligned = ((current + 3) / 4) * 4;
        const auto expected = expected_aligned + count * width;
        check(greenlens_glb_append_size(current, count, width, &aligned, &result) &&
              aligned == expected_aligned && result == expected, "Small append arithmetic differs");
      }
    }
  }
  std::size_t total = 0;
  for (const auto count : {std::size_t{1}, std::size_t{3}, std::size_t{2}}) {
    check(greenlens_glb_append_size(total, count, 1, &aligned, &result), "Sequence rejected");
    total = result;
  }
  check(total == 10, "Padding omitted from cumulative reserve");
  check(greenlens_zip_entry_count_admitted(0) &&
        greenlens_zip_entry_count_admitted(5000) &&
        !greenlens_zip_entry_count_admitted(5001) &&
        !greenlens_zip_entry_count_admitted(UINT64_MAX), "ZIP count boundary differs");
}

using Bytes = std::vector<unsigned char>;
void put16(Bytes& out, std::size_t at, std::uint16_t value) {
  out.at(at) = static_cast<unsigned char>(value);
  out.at(at + 1) = static_cast<unsigned char>(value >> 8);
}
void put32(Bytes& out, std::size_t at, std::uint32_t value) {
  for (unsigned shift = 0; shift < 4; ++shift)
    out.at(at + shift) = static_cast<unsigned char>(value >> (shift * 8));
}
void put64(Bytes& out, std::size_t at, std::uint64_t value) {
  for (unsigned shift = 0; shift < 8; ++shift)
    out.at(at + shift) = static_cast<unsigned char>(value >> (shift * 8));
}

// Original, independently constructed uncompressed ZIP structure. Every entry
// is an empty file; only central-directory intake is exercised, never SKP.
Bytes empty_files_zip(std::uint32_t count, bool zip64) {
  check(count <= 5001, "Synthetic test fixture bound exceeded");
  Bytes out;
  out.reserve(static_cast<std::size_t>(count) * 90 + 98);
  for (std::uint32_t i = 0; i < count; ++i) {
    const auto at = out.size();
    out.resize(at + 35, 0);
    put32(out, at, 0x04034b50);
    put16(out, at + 4, 20);
    put16(out, at + 26, 5);
    out[at + 30] = 'f';
    for (unsigned digit = 0; digit < 4; ++digit)
      out[at + 31 + digit] = static_cast<unsigned char>('a' + ((i >> (digit * 4)) & 15));
  }
  const auto directory = out.size();
  for (std::uint32_t i = 0; i < count; ++i) {
    const auto at = out.size();
    out.resize(at + 51, 0);
    put32(out, at, 0x02014b50);
    put16(out, at + 4, 20);
    put16(out, at + 6, 20);
    put16(out, at + 28, 5);
    put32(out, at + 42, i * 35);
    out[at + 46] = 'f';
    for (unsigned digit = 0; digit < 4; ++digit)
      out[at + 47 + digit] = static_cast<unsigned char>('a' + ((i >> (digit * 4)) & 15));
  }
  const auto directory_size = out.size() - directory;
  if (zip64) {
    const auto at = out.size();
    out.resize(at + 76, 0);
    put32(out, at, 0x06064b50);
    put64(out, at + 4, 44);
    put16(out, at + 12, 45);
    put16(out, at + 14, 45);
    put64(out, at + 24, count);
    put64(out, at + 32, count);
    put64(out, at + 40, directory_size);
    put64(out, at + 48, directory);
    put32(out, at + 56, 0x07064b50);
    put64(out, at + 64, at);
    put32(out, at + 72, 1);
  }
  const auto end = out.size();
  out.resize(end + 22, 0);
  put32(out, end, 0x06054b50);
  put16(out, end + 8, static_cast<std::uint16_t>(zip64 ? 65535 : count));
  put16(out, end + 10, static_cast<std::uint16_t>(zip64 ? 65535 : count));
  put32(out, end + 12, zip64 ? UINT32_MAX : static_cast<std::uint32_t>(directory_size));
  put32(out, end + 16, zip64 ? UINT32_MAX : static_cast<std::uint32_t>(directory));
  return out;
}

struct Allocations {
  std::array<std::size_t, 16> requests{};
  std::size_t calls = 0, live = 0;
  bool recording_failed = false;
};
void* allocate(void* opaque, std::size_t count, std::size_t size) {
  auto& observed = *static_cast<Allocations*>(opaque);
  if (!size || count > (1024u * 1024u) / size || observed.calls >= observed.requests.size()) {
    observed.recording_failed = true;
    return nullptr;
  }
  observed.requests[observed.calls++] = count * size;
  void* pointer = std::malloc(count * size);
  if (pointer) ++observed.live;
  return pointer;
}
void* reallocate(void* opaque, void* address, std::size_t count, std::size_t size) {
  auto& observed = *static_cast<Allocations*>(opaque);
  if (!size || count > (1024u * 1024u) / size || observed.calls >= observed.requests.size()) {
    observed.recording_failed = true;
    return nullptr;
  }
  observed.requests[observed.calls++] = count * size;
  const bool was_null = address == nullptr;
  void* pointer = std::realloc(address, count * size);
  if (pointer && was_null) ++observed.live;
  return pointer;
}
void release(void* opaque, void* address) {
  auto& observed = *static_cast<Allocations*>(opaque);
  if (address) {
    if (observed.live == 0) observed.recording_failed = true;
    else --observed.live;
  }
  std::free(address);
}

std::size_t zip_case(std::uint32_t count, bool zip64, std::size_t initial_request) {
  const auto input = empty_files_zip(count, zip64);
  Allocations allocations;
  mz_zip_archive zip{};
  zip.m_pAlloc = allocate;
  zip.m_pRealloc = reallocate;
  zip.m_pFree = release;
  zip.m_pAlloc_opaque = &allocations;
  const bool ok = mz_zip_reader_init_mem(&zip, input.data(), input.size(), 0) != 0;
  const auto error = mz_zip_get_last_error(&zip);
  if (count <= 5000) {
    check(ok, "Bounded synthetic ZIP did not initialize");
    check(mz_zip_reader_get_num_files(&zip) == count, "ZIP count changed");
    if (count > 0) {
      check(allocations.calls == 4, "Expected state/directory/two index allocations");
      mz_zip_archive_file_stat stat{};
      check(mz_zip_reader_file_stat(&zip, count - 1, &stat) != 0 &&
            stat.m_uncomp_size == 0, "Last admitted entry unavailable");
    } else check(allocations.calls == 1, "Empty ZIP should allocate state only");
    check(mz_zip_reader_end(&zip) != 0, "ZIP cleanup failed");
  } else {
    check(!ok && error == MZ_ZIP_TOO_MANY_FILES, "Excessive count has wrong rejection class");
    check(allocations.calls == 1 && allocations.requests[0] == initial_request,
          "Rejected ZIP allocated directory/offset storage before admission");
  }
  check(!allocations.recording_failed && allocations.live == 0, "ZIP allocator accounting/cleanup failed");
  return allocations.requests[0];
}
}

int main() {
  try {
    glb_bounds();
    const auto state_request = zip_case(0, false, 0);
    zip_case(1, false, state_request);
    zip_case(5000, false, state_request);
    zip_case(5001, false, state_request);
    zip_case(5000, true, state_request);
    zip_case(5001, true, state_request);
    std::cout << "{\"schema_version\":\"greenlens.openskp-preallocation-tests.v1\","
                 "\"passed\":" << checks << ",\"failed\":0,\"zip_intake_cases\":6,"
                 "\"skp_parses\":0,\"worker_executions\":0,\"containment_qualified\":false}\n";
    return 0;
  } catch (const std::exception& error) {
    std::cerr << "Preallocation test failed: " << error.what() << '\n';
    return 1;
  }
}
