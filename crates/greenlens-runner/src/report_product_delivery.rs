//! Verified byte copies for local report delivery. A copy or shell handoff is
//! deliberately not evidence that a human viewer displayed the document.
use crate::persistence::{
    bytes_hash, evidence_file_hash, regular_child, write_new_verified_evidence_bytes, WriterLock,
    MAX_EVIDENCE_FILE_BYTES,
};
use anyhow::{ensure, Context, Result};
use serde::Serialize;
use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};

pub const MAX_CACHED_PRODUCTS: usize = 128;
pub const MAX_CACHED_BYTES: u64 = 256 * 1024 * 1024;

#[derive(Debug, Serialize)]
pub struct PreparedProduct {
    pub path: PathBuf,
    pub sha256: String,
    pub bytes: u64,
    pub reused: bool,
}

fn check(cancel: &AtomicBool) -> Result<()> {
    ensure!(
        !cancel.load(Ordering::Relaxed),
        "Report delivery cancelled; no viewer handoff was requested"
    );
    Ok(())
}
fn validated(bytes: &[u8], expected: &str, extension: &str) -> Result<()> {
    ensure!(
        matches!(extension, "pdf" | "png"),
        "Only retained PDF/PNG delivery is supported"
    );
    ensure!(
        !bytes.is_empty() && bytes.len() as u64 <= MAX_EVIDENCE_FILE_BYTES,
        "Retained product exceeds the shared file limit"
    );
    ensure!(
        bytes_hash(bytes) == expected,
        "Retained product digest differs; no copy was written"
    );
    Ok(())
}

/// Reuse only an exact-byte match. Never evict: an external viewer's lifetime
/// cannot be observed reliably. At the cap, Save Copy remains available. Cache
/// maintenance is explicit while all external viewers are closed.
pub fn prepare_cached(
    cache: &Path,
    bytes: &[u8],
    expected: &str,
    extension: &str,
    cancel: &AtomicBool,
) -> Result<PreparedProduct> {
    validated(bytes, expected, extension)?;
    check(cancel)?;
    fs::create_dir_all(cache)?;
    let _lock = WriterLock::acquire(cache)?;
    let name = format!("{expected}.{extension}");
    let target = cache.join(&name);
    if target.exists() {
        ensure!(evidence_file_hash(&regular_child(cache, &name)?)? == expected, "Cached report copy was changed. Preserve the issued product and use Save Copy to a new destination");
        check(cancel)?;
        return Ok(PreparedProduct {
            path: target,
            sha256: expected.into(),
            bytes: bytes.len() as u64,
            reused: true,
        });
    }
    let (mut count, mut total) = (0usize, 0u64);
    for entry in fs::read_dir(cache)? {
        check(cancel)?;
        let entry = entry?;
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| anyhow::anyhow!("Invalid report cache member"))?;
        if name == ".greenlens.lock" {
            continue;
        }
        let metadata = fs::metadata(regular_child(cache, &name)?)?;
        count += 1;
        total = total
            .checked_add(metadata.len())
            .context("Report cache size overflow")?;
        ensure!(
            count < MAX_CACHED_PRODUCTS && total <= MAX_CACHED_BYTES,
            "Report copy cache is full. Use Save Copy; existing viewer copies were retained"
        );
    }
    ensure!(
        total + bytes.len() as u64 <= MAX_CACHED_BYTES,
        "Report copy cache is full. Use Save Copy; existing viewer copies were retained"
    );
    check(cancel)?;
    write_new_verified_evidence_bytes(cache, &name, bytes, expected, &mut |_| check(cancel))?;
    Ok(PreparedProduct {
        path: target,
        sha256: expected.into(),
        bytes: bytes.len() as u64,
        reused: false,
    })
}

/// Explicit new-file delivery never replaces a user's existing file or an
/// immutable issue member. Parent validation uses the shared evidence policy.
pub fn save_copy(
    destination: &Path,
    bytes: &[u8],
    expected: &str,
    extension: &str,
    cancel: &AtomicBool,
) -> Result<PreparedProduct> {
    validated(bytes, expected, extension)?;
    check(cancel)?;
    ensure!(
        !destination.exists(),
        "Save Copy requires a new filename; the existing file was preserved"
    );
    ensure!(
        destination
            .extension()
            .and_then(|s| s.to_str())
            .is_some_and(|s| s.eq_ignore_ascii_case(extension)),
        "Save Copy must retain the product's file type"
    );
    let parent = destination
        .parent()
        .context("Copy destination has no parent")?;
    let name = destination
        .file_name()
        .and_then(|n| n.to_str())
        .context("Copy filename is invalid")?;
    write_new_verified_evidence_bytes(parent, name, bytes, expected, &mut |_| check(cancel))?;
    Ok(PreparedProduct {
        path: destination.into(),
        sha256: expected.into(),
        bytes: bytes.len() as u64,
        reused: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn content_cache_reuses_bytes_and_detects_tampering_without_replacing_them() {
        let temp = tempfile::tempdir().unwrap();
        let cache = temp.path().join("copies 空間 with spaces");
        let bytes = b"%PDF-1.7\noriginal synthetic byte fixture";
        let hash = bytes_hash(bytes);
        let cancel = AtomicBool::new(false);
        let a = prepare_cached(&cache, bytes, &hash, "pdf", &cancel).unwrap();
        let b = prepare_cached(&cache, bytes, &hash, "pdf", &cancel).unwrap();
        assert_eq!(a.path, b.path);
        assert!(!a.reused && b.reused);
        fs::write(&a.path, b"changed").unwrap();
        assert!(prepare_cached(&cache, bytes, &hash, "pdf", &cancel).is_err());
        assert_eq!(fs::read(a.path).unwrap(), b"changed");
    }
    #[test]
    fn copy_cancel_conflict_digest_and_utf8_filename_boundaries_preserve_sources() {
        let temp = tempfile::tempdir().unwrap();
        let target = temp.path().join("issued copy 空間.pdf");
        let bytes = b"%PDF synthetic";
        let hash = bytes_hash(bytes);
        let cancelled = AtomicBool::new(true);
        assert!(save_copy(&target, bytes, &hash, "pdf", &cancelled).is_err());
        assert!(!target.exists());
        let running = AtomicBool::new(false);
        assert!(save_copy(&target, bytes, &"0".repeat(64), "pdf", &running).is_err());
        save_copy(&target, bytes, &hash, "pdf", &running).unwrap();
        assert!(save_copy(
            &target,
            b"changed",
            &bytes_hash(b"changed"),
            "pdf",
            &running
        )
        .is_err());
        assert_eq!(fs::read(&target).unwrap(), bytes);
    }
    #[test]
    fn full_cache_does_not_evict_and_save_copy_remains_available() {
        let temp = tempfile::tempdir().unwrap();
        let cache = temp.path().join("cache");
        fs::create_dir(&cache).unwrap();
        for n in 0..MAX_CACHED_PRODUCTS {
            fs::write(cache.join(format!("{n}.pdf")), b"old viewer copy").unwrap();
        }
        let bytes = b"%PDF synthetic";
        let hash = bytes_hash(bytes);
        let running = AtomicBool::new(false);
        assert!(prepare_cached(&cache, bytes, &hash, "pdf", &running).is_err());
        assert!(cache.join("0.pdf").exists());
        save_copy(&temp.path().join("user.pdf"), bytes, &hash, "pdf", &running).unwrap();
    }
}
