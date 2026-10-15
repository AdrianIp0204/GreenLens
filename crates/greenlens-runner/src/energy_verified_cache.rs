//! Process-local reuse of fully validated energy metadata. Every reuse checks
//! the actual bytes of the original pinned member set; this is not a timestamp
//! cache, a stored verification claim, or scientific qualification.
use crate::{
    energy::EnergyRunSummary,
    project_open::{self, VerifiedEnergyRun},
};
use anyhow::{Context, Result};
use std::{
    collections::VecDeque,
    path::{Path, PathBuf},
    sync::{atomic::AtomicBool, Arc, Mutex, OnceLock},
};

const MAX_ENTRIES: usize = 8;
const MAX_METADATA_BYTES: usize = 64 * 1024 * 1024;

#[derive(Clone, PartialEq)]
struct Key {
    root: PathBuf,
    summary: EnergyRunSummary,
}
struct Entry<T> {
    key: Key,
    value: Arc<T>,
    bytes: usize,
}
struct BoundedCache<T> {
    entries: VecDeque<Entry<T>>,
    bytes: usize,
}
impl<T> Default for BoundedCache<T> {
    fn default() -> Self {
        Self {
            entries: VecDeque::new(),
            bytes: 0,
        }
    }
}
impl<T> BoundedCache<T> {
    fn get(&mut self, key: &Key) -> Option<Arc<T>> {
        let index = self.entries.iter().position(|entry| &entry.key == key)?;
        let entry = self.entries.remove(index)?;
        let value = Arc::clone(&entry.value);
        self.entries.push_back(entry);
        Some(value)
    }
    fn insert(&mut self, key: Key, value: Arc<T>, bytes: usize) {
        // Oversize metadata remains usable through the full reader, but is not
        // retained. No annual value matrix enters this cache.
        if bytes > MAX_METADATA_BYTES {
            return;
        }
        self.entries.retain(|entry| {
            if entry.key == key {
                self.bytes -= entry.bytes;
                false
            } else {
                true
            }
        });
        while self.entries.len() >= MAX_ENTRIES || self.bytes > MAX_METADATA_BYTES - bytes {
            if let Some(entry) = self.entries.pop_front() {
                self.bytes -= entry.bytes;
            }
        }
        self.bytes += bytes;
        self.entries.push_back(Entry { key, value, bytes });
    }
    fn invalidate(&mut self, key: &Key, value: &Arc<T>) {
        // A failed old read must not evict a concurrently validated replacement.
        self.entries.retain(|entry| {
            if &entry.key == key && Arc::ptr_eq(&entry.value, value) {
                self.bytes -= entry.bytes;
                false
            } else {
                true
            }
        });
    }
}

fn cache() -> &'static Mutex<BoundedCache<VerifiedEnergyRun>> {
    static CACHE: OnceLock<Mutex<BoundedCache<VerifiedEnergyRun>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(BoundedCache::default()))
}

pub(crate) fn seed(verified: VerifiedEnergyRun) {
    retain(Arc::new(verified));
}

fn retain(verified: Arc<VerifiedEnergyRun>) {
    let key = Key {
        root: verified.root().into(),
        summary: verified.summary().clone(),
    };
    let bytes = verified.metadata_bytes();
    // This optional acceleration cannot fail an otherwise valid project open.
    if let Ok(mut cache) = cache().lock() {
        cache.insert(key, verified, bytes);
    }
}

pub(crate) fn read_run(
    root: &Path,
    summary: &EnergyRunSummary,
) -> Result<crate::energy::EnergyRunRecord> {
    Ok(read_verified(root, summary)?.record().clone())
}

fn read_verified(root: &Path, summary: &EnergyRunSummary) -> Result<Arc<VerifiedEnergyRun>> {
    let root = std::fs::canonicalize(root).context("Cannot resolve energy project directory")?;
    let key = Key {
        root: root.clone(),
        summary: summary.clone(),
    };
    let cached = cache().lock().ok().and_then(|mut cache| cache.get(&key));
    if let Some(verified) = cached {
        // Release the mutex before any filesystem access or record clone.
        let checked = project_open::check_energy_member_bytes(
            &root,
            verified.members(),
            Arc::new(AtomicBool::new(false)),
        );
        if let Err(error) = checked {
            if let Ok(mut cache) = cache().lock() {
                cache.invalidate(&key, &verified);
            }
            return Err(error.context("Previously verified energy evidence is no longer unchanged"));
        }
        return Ok(verified);
    }
    let verified = Arc::new(project_open::verify_energy_for_inspection(&root, summary)?);
    retain(Arc::clone(&verified));
    Ok(verified)
}

/// Save As uses trusted metadata plus exact byte equality across the copy.
/// The destination token is unavailable until promotion and the original
/// pinned-member comparison both succeed. The final project open remains a
/// separate complete numeric pass; sensitivity inspection need not add one.
pub(crate) fn copy_run(
    source: &Path,
    destination: &Path,
    summary: &EnergyRunSummary,
) -> Result<()> {
    let verified = read_verified(source, summary)?;
    copy_verified_run(&verified, destination)
}

fn copy_verified_run(verified: &VerifiedEnergyRun, destination: &Path) -> Result<()> {
    crate::persistence::promote_run(
        &verified.root().join(&verified.summary().path),
        destination,
        &verified.summary().id,
    )?;
    let copied = project_open::verify_copied_energy(destination, verified)?;
    seed(copied);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persistence;
    use std::{collections::BTreeMap, sync::atomic::Ordering};

    fn key(root: &str, revision: &str) -> Key {
        Key {
            root: root.into(),
            summary: EnergyRunSummary {
                id: "6dd2ce0d-35d9-4d22-aa0b-126aa7a41968".into(),
                alternative_id: "A".into(),
                path: "runs/6dd2ce0d-35d9-4d22-aa0b-126aa7a41968".into(),
                physical_input_hash: "a".repeat(64),
                record_sha256: revision.into(),
            },
        }
    }

    #[test]
    fn identity_lru_byte_bounds_and_replacement_are_exact() {
        // Only cache bookkeeping uses these integers, never an accepted run.
        let mut cache = BoundedCache::default();
        let original = Arc::new(1);
        let k = key("one", "revision1");
        cache.insert(k.clone(), original.clone(), 4);
        assert!(cache.get(&key("two", "revision1")).is_none());
        assert!(cache.get(&key("one", "revision2")).is_none());
        let mut wrong_summary = k.clone();
        wrong_summary.summary.alternative_id = "B".into();
        assert!(cache.get(&wrong_summary).is_none());
        let replacement = Arc::new(2);
        cache.insert(k.clone(), replacement.clone(), 5);
        cache.invalidate(&k, &original);
        assert!(Arc::ptr_eq(&cache.get(&k).unwrap(), &replacement));
        assert_eq!(cache.bytes, 5);
        for index in 0..MAX_ENTRIES {
            cache.insert(key("one", &index.to_string()), Arc::new(index), 1);
        }
        assert_eq!(cache.entries.len(), MAX_ENTRIES);
        assert!(cache.get(&k).is_none());
        cache.insert(k.clone(), Arc::new(9), MAX_METADATA_BYTES);
        assert_eq!((cache.entries.len(), cache.bytes), (1, MAX_METADATA_BYTES));
        cache.insert(key("oversize", "x"), Arc::new(9), MAX_METADATA_BYTES + 1);
        assert_eq!((cache.entries.len(), cache.bytes), (1, MAX_METADATA_BYTES));
        let current = cache.get(&k).unwrap();
        cache.invalidate(&k, &current);
        assert_eq!((cache.entries.len(), cache.bytes), (0, 0));
    }

    #[test]
    fn pinned_members_reject_same_length_corruption_and_consistent_reseal() {
        let dir = tempfile::tempdir().unwrap();
        let data = dir.path().join("data.bin");
        let seal = dir.path().join("checksums.json");
        std::fs::write(&data, b"before").unwrap();
        let modified = std::fs::metadata(&data).unwrap().modified().unwrap();
        let first = persistence::bytes_hash(b"before");
        std::fs::write(
            &seal,
            serde_json::to_vec(&serde_json::json!({"data.bin":first})).unwrap(),
        )
        .unwrap();
        let pins = ["data.bin", "checksums.json"]
            .into_iter()
            .map(|name| {
                (
                    name.into(),
                    persistence::evidence_file_hash(&dir.path().join(name)).unwrap(),
                )
            })
            .collect::<BTreeMap<_, _>>();
        let check = || {
            project_open::check_energy_member_bytes(
                dir.path(),
                &pins,
                Arc::new(AtomicBool::new(false)),
            )
        };
        check().unwrap();
        std::fs::write(&data, b"after!").unwrap();
        std::fs::OpenOptions::new()
            .write(true)
            .open(&data)
            .unwrap()
            .set_times(std::fs::FileTimes::new().set_modified(modified))
            .unwrap();
        assert!(check().is_err());
        std::fs::write(
            &seal,
            serde_json::to_vec(&serde_json::json!({"data.bin":persistence::bytes_hash(b"after!")}))
                .unwrap(),
        )
        .unwrap();
        assert!(
            check().is_err(),
            "A rewritten self-consistent seal cannot replace pinned bytes"
        );
        std::fs::write(&data, b"before").unwrap();
        assert!(check().is_err(), "The outer seal itself is pinned");
        let cancel = Arc::new(AtomicBool::new(true));
        assert!(
            project_open::check_energy_member_bytes(dir.path(), &pins, cancel.clone())
                .unwrap_err()
                .to_string()
                .contains("cancelled")
        );
        cancel.store(false, Ordering::Relaxed);
        assert!(project_open::check_energy_member_bytes(dir.path(), &pins, cancel).is_err());
    }

    #[test]
    fn late_failed_lookup_does_not_invalidate_a_concurrent_replacement() {
        let cache = Arc::new(Mutex::new(BoundedCache::default()));
        let k = key("project", "record");
        let original = Arc::new(1usize);
        cache.lock().unwrap().insert(k.clone(), original.clone(), 1);
        let (read_started, read_observed) = std::sync::mpsc::channel();
        let (allow_failure, wait_for_replacement) = std::sync::mpsc::channel();
        let worker_cache = cache.clone();
        let worker_key = k.clone();
        let worker = std::thread::spawn(move || {
            let candidate = worker_cache.lock().unwrap().get(&worker_key).unwrap();
            read_started.send(()).unwrap();
            // Model the period of byte hashing: the shared mutex is released.
            wait_for_replacement.recv().unwrap();
            worker_cache
                .lock()
                .unwrap()
                .invalidate(&worker_key, &candidate);
        });
        read_observed.recv().unwrap();
        let replacement = Arc::new(2usize);
        cache
            .lock()
            .unwrap()
            .insert(k.clone(), replacement.clone(), 2);
        allow_failure.send(()).unwrap();
        worker.join().unwrap();
        let mut cache = cache.lock().unwrap();
        assert!(Arc::ptr_eq(&cache.get(&k).unwrap(), &replacement));
        assert_eq!((cache.entries.len(), cache.bytes), (1, 2));
    }

    #[test]
    #[ignore = "Requires retained real evidence: GREENLENS_TEST_RETAINED_ENERGY_PROJECT; no engine run"]
    fn retained_report_and_sensitivity_panel_requests_do_not_repeat_numeric_traversal() {
        let root = PathBuf::from(
            std::env::var("GREENLENS_TEST_RETAINED_ENERGY_PROJECT")
                .expect("Retained energy fixture must be supplied"),
        );
        let canonical_root = std::fs::canonicalize(&root).unwrap();
        {
            let mut cache = cache().lock().unwrap();
            cache
                .entries
                .retain(|entry| entry.key.root != canonical_root);
            cache.bytes = cache.entries.iter().map(|entry| entry.bytes).sum();
        }
        // Cancellation at the final acceptance stage must not publish any
        // partially verified cache entry, even though all series were read.
        let cancelled = Arc::new(AtomicBool::new(false));
        let cancellation = cancelled.clone();
        let admitted = project_open::admit_project(&root).unwrap();
        let error =
            project_open::verify_admitted_project(&root, &admitted, cancelled, move |event| {
                if event.stage == "completed" {
                    cancellation.store(true, Ordering::Relaxed);
                }
            })
            .unwrap_err();
        assert!(error.to_string().contains("cancelled"));
        assert!(!cache()
            .lock()
            .unwrap()
            .entries
            .iter()
            .any(|entry| entry.key.root == canonical_root));
        let before = project_open::test_numeric_passes();
        let view = crate::open_project(&root).unwrap();
        let energy = view.project.energy.as_ref().unwrap();
        assert_eq!(
            project_open::test_numeric_passes() - before,
            energy.runs.len()
        );
        assert!(
            energy.runs.len() >= 4,
            "Fixture must include full A/B sensitivity evidence"
        );
        let pair = crate::energy_services::default_run_ids(&root, &view.project).unwrap();
        let a = pair.get("A").expect("A run");
        let b = pair.get("B").expect("B run");
        let after_open = project_open::test_numeric_passes();
        for _ in 0..2 {
            crate::energy_services::report_case(&root, &view.project, a).unwrap();
            crate::energy_services::report_case(&root, &view.project, b).unwrap();
            let selection =
                crate::energy_sensitivity_workflow::export_selection(&root, &view.project, a, b)
                    .unwrap();
            assert!(
                selection.included_session_id.is_some(),
                "A finalized matching sensitivity session is required"
            );
        }
        assert_eq!(project_open::test_numeric_passes(), after_open);

        // Copy only one complete immutable run to a new root. A new project
        // root cannot borrow the first root's trust token; the first service
        // read must run the full reader before later reuse.
        let copied = tempfile::tempdir().unwrap();
        let summary = energy.runs.iter().find(|run| &run.id == a).unwrap();
        let retained = view.energy_results.iter().find(|run| &run.id == a).unwrap();
        let mut names = crate::energy::evidence_files(retained);
        names.push("checksums.json".into());
        for name in &names {
            let source = persistence::regular_child(&root.join(&summary.path), name).unwrap();
            let destination = copied.path().join(&summary.path).join(name);
            std::fs::create_dir_all(destination.parent().unwrap()).unwrap();
            std::fs::copy(source, destination).unwrap();
        }
        read_run(copied.path(), summary).unwrap();
        assert_eq!(project_open::test_numeric_passes(), after_open + 1);
        read_run(copied.path(), summary).unwrap();
        assert_eq!(project_open::test_numeric_passes(), after_open + 1);
        let changed = copied
            .path()
            .join(&summary.path)
            .join("engine/eplusout.end");
        let original = std::fs::read(&changed).unwrap();
        let mut altered = original.clone();
        assert!(!altered.is_empty());
        altered[0] ^= 1;
        std::fs::write(&changed, &altered).unwrap();
        assert!(read_run(copied.path(), summary).is_err());
        assert_eq!(
            project_open::test_numeric_passes(),
            after_open + 1,
            "Corrupted reuse must reject, not silently fall back"
        );
        std::fs::write(&changed, &original).unwrap();
        read_run(copied.path(), summary).unwrap();
        assert_eq!(
            project_open::test_numeric_passes(),
            after_open + 2,
            "Restored bytes need a fresh full pass after invalidation"
        );
        std::fs::write(&changed, &altered).unwrap();
        let run_root = copied.path().join(&summary.path);
        let seal_path = run_root.join("checksums.json");
        let mut seal: persistence::Checksums = persistence::read_json(&seal_path).unwrap();
        seal.sha256.insert(
            "engine/eplusout.end".into(),
            persistence::bytes_hash(&altered),
        );
        persistence::write_json(&seal_path, &seal).unwrap();
        assert!(
            read_run(copied.path(), summary).is_err(),
            "A new self-consistent seal cannot inherit the prior trust token"
        );
        assert_eq!(project_open::test_numeric_passes(), after_open + 2);

        // Actual Save As and Save use the same services as the installed app.
        // A warm source copy requires only the final destination open's one
        // pass/run. A cold source requires one source pass plus that final
        // destination pass; copied sensitivity receipts add no numeric pass.
        assert_eq!(energy.runs.len(), 4, "Use the fixed four-run CI fixture");
        let original_manifest = std::fs::read(root.join("project.json")).unwrap();
        let source_tokens = energy
            .runs
            .iter()
            .map(|run| read_verified(&root, run).unwrap())
            .collect::<Vec<_>>();
        let copies_parent = crate::local_root()
            .unwrap()
            .join("verification/retained-copy-regression");
        std::fs::create_dir_all(&copies_parent).unwrap();
        let copies = tempfile::tempdir_in(copies_parent).unwrap();
        let destination = copies.path().join("warm complete copy");
        let before_copy = project_open::test_numeric_passes();
        let copied_view = crate::save_project_as(
            Some(&root),
            &destination,
            &view.project,
            Some(&view.revision),
        )
        .unwrap();
        assert_eq!(
            project_open::test_numeric_passes() - before_copy,
            energy.runs.len(),
            "Warm Save As must not rederive copied runs during promotion or sensitivity inspection"
        );
        let receipt = copied_view.project_verification.as_ref().unwrap();
        assert_eq!(receipt.progress.numeric_passes.len(), energy.runs.len());
        assert!(receipt
            .progress
            .numeric_passes
            .values()
            .all(|count| *count == 1));
        assert_eq!(
            greenlens_core::canonical_hash(&view.energy_results).unwrap(),
            greenlens_core::canonical_hash(&copied_view.energy_results).unwrap()
        );
        for token in &source_tokens {
            project_open::check_energy_member_bytes(
                &destination,
                token.members(),
                Arc::new(AtomicBool::new(false)),
            )
            .unwrap();
        }
        let before_save = project_open::test_numeric_passes();
        crate::save_project(
            &destination,
            &copied_view.project,
            Some(&copied_view.revision),
        )
        .unwrap();
        assert_eq!(
            project_open::test_numeric_passes() - before_save,
            energy.runs.len(),
            "Warm Save must use byte-checked records before its final full open"
        );
        {
            let mut cache = cache().lock().unwrap();
            cache
                .entries
                .retain(|entry| entry.key.root != canonical_root);
            cache.bytes = cache.entries.iter().map(|entry| entry.bytes).sum();
        }
        let cold_destination = copies.path().join("cold complete copy");
        let before_cold_copy = project_open::test_numeric_passes();
        crate::save_project_as(
            Some(&root),
            &cold_destination,
            &view.project,
            Some(&view.revision),
        )
        .unwrap();
        assert_eq!(
            project_open::test_numeric_passes() - before_cold_copy,
            2 * energy.runs.len(),
            "Cold Save As needs one source validation and one final destination pass/run"
        );

        // Change an owned source after its token was obtained, and rewrite its
        // seal consistently. Promotion alone would accept this new seal; the
        // exact copied-byte fence must reject it without issuing a new token.
        let mutable_source = copies.path().join("changed source");
        std::fs::create_dir_all(&mutable_source).unwrap();
        persistence::promote_run(&root.join(&summary.path), &mutable_source, &summary.id).unwrap();
        let old_token = read_verified(&mutable_source, summary).unwrap();
        let changed_root = mutable_source.join(&summary.path);
        std::fs::write(changed_root.join("engine/eplusout.end"), &altered).unwrap();
        let mut changed_seal: persistence::Checksums =
            persistence::read_json(&changed_root.join("checksums.json")).unwrap();
        changed_seal.sha256.insert(
            "engine/eplusout.end".into(),
            persistence::bytes_hash(&altered),
        );
        persistence::write_json(&changed_root.join("checksums.json"), &changed_seal).unwrap();
        let rejected_destination = copies.path().join("rejected changed copy");
        std::fs::create_dir_all(&rejected_destination).unwrap();
        let before_rejection = project_open::test_numeric_passes();
        assert!(copy_verified_run(&old_token, &rejected_destination).is_err());
        assert_eq!(project_open::test_numeric_passes(), before_rejection);
        let rejected_root = std::fs::canonicalize(&rejected_destination).unwrap();
        assert!(!cache()
            .lock()
            .unwrap()
            .entries
            .iter()
            .any(|entry| entry.key.root == rejected_root));
        assert!(!rejected_destination.join("project.json").exists());
        assert_eq!(
            std::fs::read(root.join("project.json")).unwrap(),
            original_manifest
        );
        for token in &source_tokens {
            project_open::check_energy_member_bytes(
                &root,
                token.members(),
                Arc::new(AtomicBool::new(false)),
            )
            .unwrap();
        }
        println!(
            "{}",
            serde_json::json!({
                "schema": "greenlens.retained-document-write-regression.v1",
                "runs": energy.runs.len(),
                "warm_save_as_numeric_passes": energy.runs.len(),
                "warm_save_numeric_passes": energy.runs.len(),
                "cold_save_as_numeric_passes": 2 * energy.runs.len(),
                "changed_source_copy_rejected": true,
                "original_run_bytes_and_manifest_unchanged": true,
                "native_simulations": 0
            })
        );
    }
}
