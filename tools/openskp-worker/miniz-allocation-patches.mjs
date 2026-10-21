// Original fail-early miniz reader repair; Apache-2.0.
// Anchors are from pinned MIT miniz; retain licenses/LICENSE.miniz.
// This closes one unnecessary allocation path, not process containment.
export function patchMinizExtraBounds(path, original) {
  let text = original.replaceAll('\r\n', '\n');
  if (path !== 'miniz_zip.c') return text;
  const marker = '/* GreenLens extra-field bounds patch v1; retain upstream MIT notice. */\n';
  if (text.includes(marker.trim())) throw new Error(`${path}: extra-field patch already present`);
  const before = `                        const mz_uint8 *pExtra_data;
                        void *buf = NULL;

                        if (MZ_ZIP_CENTRAL_DIR_HEADER_SIZE + filename_size + ext_data_size > n)
                        {
                            buf = MZ_MALLOC(ext_data_size);
                            if (buf == NULL)
                                return mz_zip_set_error(pZip, MZ_ZIP_ALLOC_FAILED);

                            if (pZip->m_pRead(pZip->m_pIO_opaque, cdir_ofs + MZ_ZIP_CENTRAL_DIR_HEADER_SIZE + filename_size, buf, ext_data_size) != ext_data_size)
                            {
                                MZ_FREE(buf);
                                return mz_zip_set_error(pZip, MZ_ZIP_FILE_READ_FAILED);
                            }

                            pExtra_data = (mz_uint8 *)buf;
                        }
                        else
                        {
                            pExtra_data = p + MZ_ZIP_CENTRAL_DIR_HEADER_SIZE + filename_size;
                        }
`;
  const count = text.split(before).length - 1;
  if (count !== 1) throw new Error(`${path}: expected one extra-field anchor, found ${count}`);
  const start = text.indexOf(before);
  const tail = '                        MZ_FREE(buf);\n';
  const end = text.indexOf(tail, text.indexOf('} while (extra_size_remaining);', start));
  if (end < 0) throw new Error(`${path}: missing pinned extra-field cleanup`);
  const region = text.slice(start, end + tail.length);
  if ((region.match(/MZ_FREE\(buf\);/g) ?? []).length !== 4)
    throw new Error(`${path}: changed extra-field cleanup closure`);
  const after = `                        const mz_uint8 *pExtra_data;
                        /* All bytes of a directory entry must fit in the declared
                         * central directory. The later total_header_size check
                         * already rejects this case; reject before any allocation
                         * or read outside that directory extent. UINT16 lengths
                         * keep this sum below 131116, within mz_uint. */
                        if (MZ_ZIP_CENTRAL_DIR_HEADER_SIZE + filename_size + ext_data_size > n)
                            return mz_zip_set_error(pZip, MZ_ZIP_INVALID_HEADER_OR_CORRUPTED);
                        pExtra_data = p + MZ_ZIP_CENTRAL_DIR_HEADER_SIZE + filename_size;
`;
  let repaired = region.replace(before, after);
  repaired = repaired.replaceAll('                                MZ_FREE(buf);\n', '')
    .replace(tail, '');
  if (/MZ_MALLOC|MZ_FREE|\bbuf\b/.test(repaired))
    throw new Error(`${path}: incomplete extra-field allocation removal`);
  text = text.slice(0, start) + repaired + text.slice(end + tail.length);
  return marker + text;
}
