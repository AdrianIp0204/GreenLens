/* Original GreenLens development admission helpers; Apache-2.0.
 * These bound specific requested buffer lengths, not process commitment.
 * C compatible so the pinned miniz and C++ exporter use the same limits.
 */
#ifndef GREENLENS_PREALLOCATION_BOUNDS_H
#define GREENLENS_PREALLOCATION_BOUNDS_H
#include <stddef.h>
#include <stdint.h>

#define GREENLENS_ZIP_ENTRY_LIMIT 5000u
#define GREENLENS_GLB_BYTE_LIMIT (64u * 1024u * 1024u)

/* Match the existing MSVC C dialect used by miniz without changing its flags. */
#if defined(_MSC_VER) && !defined(__cplusplus)
#define GREENLENS_BOUNDS_INLINE __inline
#else
#define GREENLENS_BOUNDS_INLINE inline
#endif

static GREENLENS_BOUNDS_INLINE int greenlens_zip_entry_count_admitted(uint64_t count) {
  return count <= GREENLENS_ZIP_ENTRY_LIMIT;
}

/* A GLB array starts at a four-byte boundary. Reject before addition,
 * multiplication or allocation. On failure both outputs remain untouched.
 */
static GREENLENS_BOUNDS_INLINE int greenlens_glb_append_size(
    size_t current, size_t count, size_t element_size,
    size_t* aligned_offset, size_t* resulting_size) {
  const size_t limit = GREENLENS_GLB_BYTE_LIMIT;
  size_t padding;
  size_t aligned;
  if (!aligned_offset || !resulting_size || !element_size || current > limit)
    return 0;
  padding = (4u - current % 4u) % 4u;
  if (padding > limit - current) return 0;
  aligned = current + padding;
  if (count > (limit - aligned) / element_size) return 0;
  *aligned_offset = aligned;
  *resulting_size = aligned + count * element_size;
  return 1;
}
#undef GREENLENS_BOUNDS_INLINE
#endif
