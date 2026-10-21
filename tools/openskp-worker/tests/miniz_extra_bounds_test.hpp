// Original synthetic central-directory fixtures; Apache-2.0.
// Include after the unit's Bytes/put*/empty_files_zip/Allocations helpers, inside
// its anonymous namespace. No SKP data/parser or process-memory claim.
#pragma once

Bytes one_zip64_extra_file(bool zip64_end_record) {
  auto input = empty_files_zip(1, zip64_end_record);
  constexpr std::size_t directory = 35, directory_end = 86, extra_size = 20;
  input.insert(input.begin() + directory_end, extra_size, 0);
  put32(input, directory + 20, UINT32_MAX); // ZIP64 compressed size follows.
  put32(input, directory + 24, UINT32_MAX); // ZIP64 uncompressed size follows.
  put16(input, directory + 30, extra_size);
  put16(input, directory_end, 0x0001);      // ZIP64 extra-field identifier.
  put16(input, directory_end + 2, 16);     // Two little-endian uint64 sizes, both zero.
  if (zip64_end_record) {
    const auto end64 = directory_end + extra_size;
    put64(input, end64 + 40, 51 + extra_size);
    put64(input, end64 + 56 + 8, end64);  // Locator binds shifted ZIP64 EOCD.
  } else {
    put32(input, directory_end + extra_size + 12, 51 + extra_size);
  }
  return input;
}

void miniz_extra_bounds_case(bool zip64_end_record, std::uint16_t absent_extra_bytes) {
  const bool positive = absent_extra_bytes == 0;
  auto input = positive ? one_zip64_extra_file(zip64_end_record) : empty_files_zip(1, zip64_end_record);
  if (!positive) {
    constexpr std::size_t directory = 35;
    put32(input, directory + 20, UINT32_MAX);
    put32(input, directory + 24, UINT32_MAX);
    put16(input, directory + 30, absent_extra_bytes);
    // The declared central directory remains exactly 51 bytes: its header and
    // five-character name. No extra field exists inside the declared extent.
  }
  Allocations allocations;
  mz_zip_archive zip{};
  zip.m_pAlloc = allocate;
  zip.m_pRealloc = reallocate;
  zip.m_pFree = release;
  zip.m_pAlloc_opaque = &allocations;
  const bool ok = mz_zip_reader_init_mem(&zip, input.data(), input.size(), 0) != 0;
  const auto error = mz_zip_get_last_error(&zip);
  if (positive) {
    check(ok, "Valid ZIP64 extra field rejected");
    mz_zip_archive_file_stat stat{};
    check(mz_zip_reader_get_num_files(&zip) == 1 && mz_zip_reader_file_stat(&zip, 0, &stat) != 0,
          "Valid extra-field entry unavailable");
    check(stat.m_comp_size == 0 && stat.m_uncomp_size == 0, "ZIP64 empty dimensions changed");
    check(mz_zip_reader_end(&zip) != 0, "Extra-field ZIP cleanup failed");
  } else {
    check(!ok && error == MZ_ZIP_INVALID_HEADER_OR_CORRUPTED,
          "Out-of-directory extra field must be malformed input, not allocation/I/O failure");
  }
  check(allocations.calls == 4, "Unexpected extra-field allocator callback count");
  check(!allocations.recording_failed && allocations.live == 0, "Extra-field allocation/cleanup failed");
}

void miniz_extra_bounds_cases() {
  for (const bool zip64 : {false, true}) {
    miniz_extra_bounds_case(zip64, 0);      // Positive exact extra field.
    miniz_extra_bounds_case(zip64, 1);      // One byte over declared directory.
    miniz_extra_bounds_case(zip64, 65535);  // Maximum encoded extra size.
  }
}
