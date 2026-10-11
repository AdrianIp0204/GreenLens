use anyhow::{bail, ensure, Context, Result};
use fs2::FileExt;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Component, Path, PathBuf},
};
use uuid::Uuid;

pub const MAX_PACKAGE_ENTRIES: usize = 5000;
pub const MAX_EVIDENCE_FILE_BYTES: u64 = 64 * 1024 * 1024;
pub const MAX_PACKAGE_BYTES: u64 = 256 * 1024 * 1024;

/// Writer and reader use the same uncompressed archive budget.
#[derive(Default)]
pub struct PackageBudget {
    paths: std::collections::HashSet<String>,
    total_bytes: u64,
}
impl PackageBudget {
    pub fn add(&mut self, name: &str, bytes: u64) -> Result<()> {
        relative_path(name)?;
        ensure!(
            name.len() <= 240,
            "Evidence path exceeds the 240-byte portable limit"
        );
        ensure!(self.paths.len() < MAX_PACKAGE_ENTRIES, "Evidence package exceeds 5000 entries. Save a project with fewer retained runs before exporting");
        ensure!(
            self.paths.insert(name.to_ascii_lowercase()),
            "Duplicate evidence ZIP entry"
        );
        ensure!(
            bytes <= MAX_EVIDENCE_FILE_BYTES,
            "Evidence file exceeds 64 MiB. Reduce retained runs or sensor count before exporting"
        );
        self.total_bytes = self
            .total_bytes
            .checked_add(bytes)
            .context("Evidence size overflow")?;
        ensure!(self.total_bytes <= MAX_PACKAGE_BYTES, "Evidence package exceeds 256 MiB uncompressed. Export a project with fewer retained runs");
        Ok(())
    }
}

pub fn bytes_hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

// A reparse attribute alone does not imply a link: OneDrive also sets it on
// ordinary hydrated cloud files. Inspect the tag without following it.
#[cfg(windows)]
fn validate_windows_attributes(attributes: u32, tag: u32, is_directory: bool) -> Result<()> {
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_ATTRIBUTE_OFFLINE, FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS, FILE_ATTRIBUTE_REPARSE_POINT,
    };
    if attributes & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        // Windows SDK IsReparseTagNameSurrogate and IO_REPARSE_TAG_CLOUD_MASK.
        ensure!(
            tag & 0x2000_0000 == 0,
            "Directory links and name-surrogate reparse points are not allowed in evidence"
        );
        let cloud = tag & !0x0000_f000 == 0x9000_001a || tag == 0x8000_0021;
        ensure!(cloud, "Unsupported reparse tag in evidence: {tag:#x}");
    }
    // A cloud directory can contain unrelated online-only children. Only the
    // specific evidence files need to be hydrated; never trigger a download.
    if !is_directory {
        ensure!(
            attributes & (FILE_ATTRIBUTE_OFFLINE | FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS) == 0,
            "Evidence file is not fully available locally. Make this project available offline in OneDrive, then retry"
        );
    }
    Ok(())
}

#[cfg(windows)]
fn inspect_windows_handle(file: &File) -> Result<fs::Metadata> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Storage::FileSystem::{
        FileAttributeTagInfo, GetFileInformationByHandleEx, FILE_ATTRIBUTE_TAG_INFO,
    };
    let mut info = FILE_ATTRIBUTE_TAG_INFO::default();
    // SAFETY: the file owns a live handle and info is the correctly sized
    // writable structure for FileAttributeTagInfo.
    let ok = unsafe {
        GetFileInformationByHandleEx(
            file.as_raw_handle(),
            FileAttributeTagInfo,
            (&mut info as *mut FILE_ATTRIBUTE_TAG_INFO).cast(),
            std::mem::size_of::<FILE_ATTRIBUTE_TAG_INFO>() as u32,
        )
    };
    if ok == 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    let metadata = file.metadata()?;
    validate_windows_attributes(info.FileAttributes, info.ReparseTag, metadata.is_dir())?;
    Ok(metadata)
}

fn checked_metadata(path: &Path) -> Result<fs::Metadata> {
    let metadata = fs::symlink_metadata(path)?;
    ensure!(
        !metadata.file_type().is_symlink(),
        "Symlinks are not allowed in evidence"
    );
    #[cfg(windows)]
    {
        use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
        use windows_sys::Win32::Storage::FileSystem::{
            FILE_ATTRIBUTE_REPARSE_POINT, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT,
        };
        if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            let file = OpenOptions::new()
                .read(true)
                .access_mode(0)
                .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_BACKUP_SEMANTICS)
                .open(path)?;
            return inspect_windows_handle(&file);
        }
        validate_windows_attributes(metadata.file_attributes(), 0, metadata.is_dir())?;
    }
    Ok(metadata)
}

/// While retained on Windows, this handle denies deletion/renaming of the
/// checked directory. This prevents project/runs path replacement during a
/// promotion. It does not require a directory to be outside OneDrive.
struct DirectoryGuard {
    #[cfg(windows)]
    _file: File,
}
impl DirectoryGuard {
    fn open(path: &Path) -> Result<Self> {
        ensure!(
            checked_metadata(path)?.is_dir(),
            "Expected an evidence directory"
        );
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            use windows_sys::Win32::Storage::FileSystem::{
                FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_LIST_DIRECTORY,
                FILE_SHARE_READ, FILE_SHARE_WRITE,
            };
            let file = OpenOptions::new()
                .read(true)
                .access_mode(FILE_LIST_DIRECTORY)
                .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
                .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_BACKUP_SEMANTICS)
                .open(path)?;
            ensure!(
                inspect_windows_handle(&file)?.is_dir(),
                "Expected an evidence directory"
            );
            Ok(Self { _file: file })
        }
        #[cfg(not(windows))]
        {
            Ok(Self {})
        }
    }
}

fn create_checked_directories(
    root: &Path,
    relative: &Path,
) -> Result<(PathBuf, Vec<DirectoryGuard>)> {
    let mut current = root.to_owned();
    let mut guards = vec![DirectoryGuard::open(root)?];
    for component in relative.components() {
        ensure!(
            matches!(component, Component::Normal(_)),
            "Unsafe directory path"
        );
        current.push(component.as_os_str());
        match fs::symlink_metadata(&current) {
            Ok(_) => (),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                match fs::create_dir(&current) {
                    Ok(()) => (),
                    Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => (),
                    Err(error) => return Err(error.into()),
                }
            }
            Err(error) => return Err(error.into()),
        }
        guards.push(DirectoryGuard::open(&current)?);
    }
    Ok((current, guards))
}

fn read_limited(mut source: impl Read, limit: u64) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    source.by_ref().take(limit + 1).read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 <= limit,
        "JSON exceeds 64 MiB safety limit"
    );
    Ok(bytes)
}

/// Check availability before opening, then bind the size/policy checks to the
/// opened file. The actual read stays bounded if an external writer grows it.
pub(crate) fn read_json_bytes(path: &Path) -> Result<Vec<u8>> {
    let meta = checked_metadata(path)?;
    ensure!(
        !meta.file_type().is_symlink() && meta.is_file(),
        "Expected a regular file"
    );
    ensure!(
        meta.len() <= MAX_EVIDENCE_FILE_BYTES,
        "JSON exceeds 64 MiB safety limit"
    );
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        use windows_sys::Win32::Storage::FileSystem::FILE_FLAG_OPEN_REPARSE_POINT;
        options.custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
    }
    let file = options.open(path)?;
    #[cfg(windows)]
    let opened = inspect_windows_handle(&file)?;
    #[cfg(not(windows))]
    let opened = file.metadata()?;
    ensure!(opened.is_file(), "Expected a regular file");
    ensure!(
        opened.len() <= MAX_EVIDENCE_FILE_BYTES,
        "JSON exceeds 64 MiB safety limit"
    );
    read_limited(file, MAX_EVIDENCE_FILE_BYTES)
}

pub fn read_json<T: DeserializeOwned>(path: &Path) -> Result<T> {
    Ok(serde_json::from_slice(&read_json_bytes(path)?)?)
}
pub fn write_json(path: &Path, value: &impl Serialize) -> Result<()> {
    let mut data = serde_json::to_vec_pretty(value)?;
    data.push(b'\n');
    ensure!(data.len() as u64 <= MAX_EVIDENCE_FILE_BYTES, "Generated JSON exceeds 64 MiB. Keep the current project and export a smaller analysis project");
    fs::write(path, data)?;
    Ok(())
}
pub fn relative_path(text: &str) -> Result<PathBuf> {
    ensure!(
        !text.is_empty() && !text.contains('\\') && !text.contains(':') && !text.contains('\0'),
        "Unsafe evidence path"
    );
    for part in text.split('/') {
        ensure!(
            !part.is_empty() && part != "." && part != ".." && !part.ends_with(['.', ' ']),
            "Unsafe evidence path component"
        );
        ensure!(
            !part
                .chars()
                .any(|c| c.is_control() || "<>\"|?*".contains(c)),
            "Invalid evidence filename"
        );
        let stem = part
            .split('.')
            .next()
            .unwrap_or_default()
            .to_ascii_uppercase();
        ensure!(
            !["CON", "PRN", "AUX", "NUL", "CONIN$", "CONOUT$"].contains(&stem.as_str())
                && !(stem.len() == 4
                    && (stem.starts_with("COM") || stem.starts_with("LPT"))
                    && matches!(stem.as_bytes()[3], b'1'..=b'9')),
            "Reserved Windows filename"
        );
    }
    let path = Path::new(text);
    ensure!(
        path.components().all(|c| matches!(c, Component::Normal(_))),
        "Evidence paths must be relative and contained"
    );
    Ok(path.to_owned())
}
pub fn regular_child(root: &Path, text: &str) -> Result<PathBuf> {
    let relative = relative_path(text)?;
    ensure!(
        checked_metadata(root)?.is_dir(),
        "Expected an evidence directory"
    );
    let mut path = root.to_owned();
    for part in relative.components() {
        path.push(part.as_os_str());
        checked_metadata(&path)?;
    }
    ensure!(path.is_file(), "Missing evidence file");
    Ok(path)
}
pub struct WriterLock {
    file: File,
    _directory: DirectoryGuard,
}
impl WriterLock {
    pub fn acquire(root: &Path) -> Result<Self> {
        fs::create_dir_all(root)?;
        let directory = DirectoryGuard::open(root)?;
        let lock_path = root.join(".greenlens.lock");
        if fs::symlink_metadata(&lock_path).is_ok() {
            ensure!(
                checked_metadata(&lock_path)?.is_file(),
                "Expected a regular lock file"
            );
        }
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(lock_path)?;
        file.try_lock_exclusive()
            .context("Project is open for writing elsewhere; close the other writer")?;
        Ok(Self {
            file,
            _directory: directory,
        })
    }
}
impl Drop for WriterLock {
    fn drop(&mut self) {
        let _ = FileExt::unlock(&self.file);
    }
}

pub fn revision(root: &Path) -> Result<Option<String>> {
    let path = root.join("project.json");
    match fs::symlink_metadata(&path) {
        Ok(_) => {
            let path = regular_child(root, "project.json")?;
            Ok(Some(bytes_hash(&read_json_bytes(&path)?)))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}
pub fn check_revision(root: &Path, expected: Option<&str>) -> Result<()> {
    ensure!(
        revision(root)?.as_deref() == expected,
        "Project changed externally. Reopen before saving; changes were not overwritten"
    );
    Ok(())
}
pub fn atomic_json(root: &Path, value: &impl Serialize) -> Result<String> {
    let temporary = root.join(format!(".project-{}.tmp", Uuid::new_v4()));
    let mut data = serde_json::to_vec_pretty(value)?;
    data.push(b'\n');
    ensure!(
        data.len() as u64 <= MAX_EVIDENCE_FILE_BYTES,
        "Project manifest exceeds the 64 MiB readable limit. The existing project was retained; save a project with fewer historical runs"
    );
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&temporary)?;
    file.write_all(&data)?;
    file.sync_all()?;
    drop(file);
    atomic_replace(&temporary, &root.join("project.json"))?;
    Ok(bytes_hash(&data))
}
#[cfg(windows)]
fn atomic_replace(source: &Path, target: &Path) -> Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
    };
    let source: Vec<u16> = source.as_os_str().encode_wide().chain(Some(0)).collect();
    let target: Vec<u16> = target.as_os_str().encode_wide().chain(Some(0)).collect();
    let success = unsafe {
        MoveFileExW(
            source.as_ptr(),
            target.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if success == 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    Ok(())
}
#[cfg(not(windows))]
fn atomic_replace(source: &Path, target: &Path) -> Result<()> {
    fs::rename(source, target)?;
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Checksums {
    pub schema_version: String,
    pub sha256: BTreeMap<String, String>,
}
pub fn seal(root: &Path, files: &[&str]) -> Result<()> {
    let mut hashes = BTreeMap::new();
    for name in files {
        hashes.insert(
            (*name).to_string(),
            bytes_hash(&fs::read(regular_child(root, name)?)?),
        );
    }
    write_json(
        &root.join("checksums.json"),
        &Checksums {
            schema_version: "0.1.0".into(),
            sha256: hashes,
        },
    )
}
pub fn verify_seal(root: &Path) -> Result<Checksums> {
    let checksums: Checksums = read_json(&regular_child(root, "checksums.json")?)?;
    ensure!(
        checksums.schema_version == "0.1.0" && !checksums.sha256.is_empty(),
        "Unsupported/empty checksums"
    );
    for (name, digest) in &checksums.sha256 {
        let file = regular_child(root, name)?;
        ensure!(
            fs::metadata(&file)?.len() <= MAX_EVIDENCE_FILE_BYTES,
            "Evidence file too large"
        );
        ensure!(
            bytes_hash(&fs::read(file)?) == *digest,
            "Evidence integrity failure: {name}"
        );
    }
    Ok(checksums)
}
pub fn promote_run(source: &Path, project: &Path, id: &str) -> Result<()> {
    promote_evidence(source, project, "runs", id)
}

/// Caller owns the project writer lock, as for run promotion.
pub fn promote_weather(source: &Path, project: &Path, id: &str) -> Result<()> {
    promote_evidence(source, project, "weather", id)
}

fn promote_evidence(source: &Path, project: &Path, prefix: &str, id: &str) -> Result<()> {
    Uuid::parse_str(id).context("Invalid evidence identifier")?;
    let sums = verify_seal(source)?;
    let (runs, _destination_guards) = create_checked_directories(project, Path::new(prefix))?;
    let target = runs.join(id);
    if fs::symlink_metadata(&target).is_ok() {
        let _existing_guard = DirectoryGuard::open(&target)?;
        ensure!(
            verify_seal(&target)?.sha256 == sums.sha256,
            "Evidence identity conflict"
        );
        return Ok(());
    }
    let temporary = runs.join(format!(".pending-{}", Uuid::new_v4()));
    fs::create_dir(&temporary)?;
    for name in sums
        .sha256
        .keys()
        .chain(std::iter::once(&"checksums.json".to_string()))
    {
        let relative = relative_path(name)?;
        let destination = temporary.join(&relative);
        let parent = relative.parent().unwrap_or_else(|| Path::new(""));
        let (_parent, _parent_guards) = create_checked_directories(&temporary, parent)?;
        let mut source = File::open(regular_child(source, name)?)?;
        let mut destination = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(destination)?;
        std::io::copy(&mut source, &mut destination)?;
        destination.sync_all()?;
    }
    verify_seal(&temporary)?;
    fs::rename(temporary, target)?;
    Ok(())
}

pub fn zip_directory(root: &Path, files: &[String], destination: &Path) -> Result<()> {
    ensure!(
        !destination.exists(),
        "Export destination already exists; choose a new filename"
    );
    // Reject a package the structural reader cannot accept before opening any
    // destination file. Generated report/comparison bytes are included here.
    let mut budget = PackageBudget::default();
    let mut sizes = Vec::with_capacity(files.len());
    for name in files {
        let size = fs::metadata(regular_child(root, name)?)?.len();
        budget.add(name, size)?;
        sizes.push(size);
    }
    let temporary = destination.with_extension(format!("{}.tmp", Uuid::new_v4()));
    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)?;
    let mut zip = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    for (name, expected_size) in files.iter().zip(sizes) {
        let path = regular_child(root, name)?;
        zip.start_file(name, options)?;
        let mut file = File::open(path)?;
        let copied = std::io::copy(
            &mut Read::by_ref(&mut file).take(MAX_EVIDENCE_FILE_BYTES + 1),
            &mut zip,
        )?;
        ensure!(
            copied == expected_size,
            "Evidence changed while exporting; retry after closing other writers"
        );
    }
    zip.finish()?.sync_all()?;
    fs::rename(temporary, destination)?;
    Ok(())
}

pub fn extract_package(package: &Path, destination: &Path) -> Result<()> {
    ensure!(!destination.exists(), "Extraction destination must be new");
    let mut zip = zip::ZipArchive::new(File::open(package)?)?;
    ensure!(
        zip.len() <= MAX_PACKAGE_ENTRIES,
        "Evidence ZIP contains too many entries"
    );
    let mut budget = PackageBudget::default();
    for i in 0..zip.len() {
        let item = zip.by_index(i)?;
        budget.add(item.name(), item.size())?;
        ensure!(
            !item.is_dir(),
            "Evidence ZIP contains unexpected directory entries"
        );
        ensure!(
            item.unix_mode().is_none_or(|m| m & 0o170000 != 0o120000),
            "ZIP symlinks are forbidden"
        );
    }
    fs::create_dir_all(destination)?;
    let mut actual_total = 0u64;
    for i in 0..zip.len() {
        let mut item = zip.by_index(i)?;
        let declared_size = item.size();
        let path = destination.join(relative_path(item.name())?);
        fs::create_dir_all(path.parent().context("Invalid destination")?)?;
        let mut out = OpenOptions::new().create_new(true).write(true).open(path)?;
        let copied = std::io::copy(
            &mut Read::by_ref(&mut item).take(MAX_EVIDENCE_FILE_BYTES + 1),
            &mut out,
        )?;
        if copied > MAX_EVIDENCE_FILE_BYTES {
            bail!("ZIP entry exceeds actual size limit");
        }
        ensure!(
            copied == declared_size,
            "ZIP entry size differs from declared size"
        );
        actual_total = actual_total
            .checked_add(copied)
            .context("ZIP actual size overflow")?;
        ensure!(
            actual_total <= MAX_PACKAGE_BYTES,
            "ZIP exceeds actual total size limit"
        );
    }
    let sums = verify_seal(destination)?;
    ensure!(
        sums.sha256.contains_key("project.json"),
        "Evidence package does not seal project.json"
    );
    let covered: std::collections::HashSet<String> = sums
        .sha256
        .keys()
        .map(|s| s.to_ascii_lowercase())
        .chain(std::iter::once("checksums.json".into()))
        .collect();
    ensure!(
        budget.paths == covered,
        "Evidence checksum coverage differs from package contents"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn oversized_external_revision_preserves_pending_climate_evidence() {
        use crate::{ActiveJob, Application, JobState, JobStatus};
        use std::sync::{atomic::AtomicBool, Arc, Mutex};

        let directory = tempfile::tempdir().unwrap();
        let scratch = tempfile::tempdir().unwrap();
        let saved = crate::save_project(directory.path(), &crate::example(), None).unwrap();
        let record = crate::climate::tests::fixture(scratch.path(), false);
        let original_record = serde_json::to_value(&record).unwrap();
        let original_project = serde_json::to_value(&saved.project).unwrap();
        let original_seal = fs::read(scratch.path().join("checksums.json")).unwrap();
        let app = Application {
            active: Mutex::new(Some(ActiveJob {
                status: Arc::new(Mutex::new(JobStatus {
                    id: record.id.clone(),
                    state: JobState::Completed,
                    log: vec!["Synthetic pending evidence; no engine executed".into()],
                    error: None,
                    result: None,
                    climate_result: Some(record),
                    pending_evidence: true,
                })),
                cancel: Arc::new(AtomicBool::new(false)),
                handle: None,
                scratch: scratch.path().to_owned(),
                project_id: Some(saved.project.id.clone()),
            })),
        };
        let manifest = directory.path().join("project.json");
        let external = OpenOptions::new().write(true).open(&manifest).unwrap();
        #[cfg(windows)]
        {
            use std::{ffi::c_void, os::windows::io::AsRawHandle};
            #[link(name = "kernel32")]
            unsafe extern "system" {
                fn DeviceIoControl(
                    handle: *mut c_void,
                    code: u32,
                    input: *const c_void,
                    input_size: u32,
                    output: *mut c_void,
                    output_size: u32,
                    returned: *mut u32,
                    overlapped: *mut c_void,
                ) -> i32;
            }
            let mut returned = 0;
            // Mark only this owned temporary fixture sparse before extending
            // its logical length. No large output or engine process is needed.
            let ok = unsafe {
                DeviceIoControl(
                    external.as_raw_handle(),
                    0x0009_00c4, // FSCTL_SET_SPARSE
                    std::ptr::null(),
                    0,
                    std::ptr::null_mut(),
                    0,
                    &mut returned,
                    std::ptr::null_mut(),
                )
            };
            assert_ne!(ok, 0, "Sparse fixture requires a supporting filesystem");
        }
        external.set_len(MAX_EVIDENCE_FILE_BYTES + 1).unwrap();
        drop(external);

        let failure = app
            .collect(directory.path(), &saved.project, Some(&saved.revision))
            .unwrap_err();
        assert!(failure.to_string().contains("64 MiB"), "{failure:#}");
        assert!(
            crate::save_project(directory.path(), &saved.project, Some(&saved.revision))
                .unwrap_err()
                .to_string()
                .contains("64 MiB")
        );
        assert!(crate::open_project(directory.path())
            .unwrap_err()
            .to_string()
            .contains("64 MiB"));
        assert_eq!(
            fs::metadata(&manifest).unwrap().len(),
            MAX_EVIDENCE_FILE_BYTES + 1
        );
        assert_eq!(
            serde_json::to_value(&saved.project).unwrap(),
            original_project
        );
        assert!(!directory.path().join("runs").exists());
        assert!(!directory.path().join("weather").exists());
        let status = app.status().unwrap();
        assert!(status.pending_evidence);
        assert_eq!(
            serde_json::to_value(status.climate_result.unwrap()).unwrap(),
            original_record
        );
        assert_eq!(
            fs::read(scratch.path().join("checksums.json")).unwrap(),
            original_seal
        );
        assert!(app.prepare_close(false).is_err());
    }

    #[cfg(windows)]
    #[test]
    fn offline_manifest_revision_is_rejected_before_hashing() {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::Storage::FileSystem::{
            SetFileAttributesW, FILE_ATTRIBUTE_NORMAL, FILE_ATTRIBUTE_OFFLINE,
        };
        let directory = tempfile::tempdir().unwrap();
        let manifest = directory.path().join("project.json");
        fs::write(&manifest, b"{\"synthetic\":true}\n").unwrap();
        let path: Vec<u16> = manifest.as_os_str().encode_wide().chain(Some(0)).collect();
        // This sets the offline attribute on an owned local test file only.
        // It does not create a cloud file, use OneDrive or access the network.
        assert_ne!(
            unsafe { SetFileAttributesW(path.as_ptr(), FILE_ATTRIBUTE_OFFLINE) },
            0
        );
        let result = revision(directory.path());
        let open_result = crate::open_project(directory.path());
        assert_ne!(
            unsafe { SetFileAttributesW(path.as_ptr(), FILE_ATTRIBUTE_NORMAL) },
            0
        );
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("not fully available locally"));
        assert!(open_result
            .unwrap_err()
            .to_string()
            .contains("not fully available locally"));
        assert!(revision(directory.path()).unwrap().is_some());
    }

    #[test]
    fn manifest_revision_distinguishes_absent_from_nonregular() {
        let directory = tempfile::tempdir().unwrap();
        assert!(revision(directory.path()).unwrap().is_none());
        fs::create_dir(directory.path().join("project.json")).unwrap();
        assert!(revision(directory.path()).is_err());
    }

    #[test]
    fn bounded_bytes_accept_exact_limit_and_stop_after_one_extra_byte() {
        use std::io::Cursor;
        let expected = vec![b'x'; 32];
        assert_eq!(read_limited(Cursor::new(&expected), 32).unwrap(), expected);
        let mut growing_source = Cursor::new(vec![b'x'; 1_024]);
        assert!(read_limited(&mut growing_source, 32).is_err());
        assert_eq!(growing_source.position(), 33);
    }

    #[test]
    fn oversized_manifest_preserves_previous_file_without_temporary_write() {
        let project = tempfile::tempdir().unwrap();
        let prior_revision = atomic_json(project.path(), &vec!["retained project"]).unwrap();
        let prior_bytes = fs::read(project.path().join("project.json")).unwrap();
        let oversized = "x".repeat(MAX_EVIDENCE_FILE_BYTES as usize);
        assert!(atomic_json(project.path(), &oversized).is_err());
        assert_eq!(
            fs::read(project.path().join("project.json")).unwrap(),
            prior_bytes
        );
        assert_eq!(revision(project.path()).unwrap(), Some(prior_revision));
        assert_eq!(fs::read_dir(project.path()).unwrap().count(), 1);
    }
    #[test]
    fn shared_archive_budget_accepts_exact_bounds_and_rejects_one_over() {
        let mut entries = PackageBudget::default();
        for i in 0..MAX_PACKAGE_ENTRIES {
            entries.add(&format!("file-{i}"), 0).unwrap();
        }
        assert!(entries.add("one-too-many", 0).is_err());
        let mut bytes = PackageBudget::default();
        for i in 0..4 {
            bytes
                .add(&format!("large-{i}"), MAX_EVIDENCE_FILE_BYTES)
                .unwrap();
        }
        assert!(bytes.add("one-byte-too-many", 1).is_err());
        assert!(PackageBudget::default()
            .add("too-large", MAX_EVIDENCE_FILE_BYTES + 1)
            .is_err());
    }
    #[test]
    fn oversized_writer_input_is_rejected_before_creating_any_export() {
        let source = tempfile::tempdir().unwrap();
        let output = tempfile::tempdir().unwrap();
        File::create(source.path().join("large"))
            .unwrap()
            .set_len(MAX_EVIDENCE_FILE_BYTES + 1)
            .unwrap();
        let destination = output.path().join("evidence.zip");
        assert!(zip_directory(source.path(), &["large".into()], &destination).is_err());
        assert_eq!(fs::read_dir(output.path()).unwrap().count(), 0);
    }
    #[test]
    fn reject_paths() {
        for s in ["../a", "/a", "C:/a", "a\\b", "a/../b", ""] {
            assert!(relative_path(s).is_err(), "{s}");
        }
    }
    #[test]
    fn lock_and_conflict() {
        let d = tempfile::tempdir().unwrap();
        let _lock = WriterLock::acquire(d.path()).unwrap();
        assert!(WriterLock::acquire(d.path()).is_err());
        let rev = atomic_json(d.path(), &vec![1]).unwrap();
        assert!(check_revision(d.path(), Some(&rev)).is_ok());
        assert!(check_revision(d.path(), None).is_err());
    }
    #[test]
    fn integrity_rejects_tampering() {
        let d = tempfile::tempdir().unwrap();
        fs::write(d.path().join("x"), "first").unwrap();
        seal(d.path(), &["x"]).unwrap();
        fs::write(d.path().join("x"), "changed").unwrap();
        assert!(verify_seal(d.path()).is_err());
    }

    #[cfg(windows)]
    #[test]
    fn cloud_metadata_is_distinguished_from_redirecting_and_offline_reparse_points() {
        const REPARSE: u32 = 0x400;
        assert!(validate_windows_attributes(0, 0, false).is_ok());
        for variant in 0..=15 {
            assert!(
                validate_windows_attributes(REPARSE, 0x9000_001a | (variant << 12), false).is_ok()
            );
        }
        assert!(validate_windows_attributes(REPARSE, 0x8000_0021, false).is_ok());
        assert!(validate_windows_attributes(REPARSE, 0xa000_0003, true).is_err());
        assert!(validate_windows_attributes(REPARSE, 0xa000_000c, true).is_err());
        assert!(validate_windows_attributes(REPARSE, 0x8000_0017, false).is_err());
        assert!(validate_windows_attributes(REPARSE | 0x1000, 0x9000_001a, false).is_err());
        assert!(validate_windows_attributes(REPARSE | 0x0040_0000, 0x9000_001a, false).is_err());
        assert!(validate_windows_attributes(REPARSE | 0x0040_0000, 0x9000_001a, true).is_ok());
    }

    #[cfg(windows)]
    #[test]
    fn retained_directory_handle_prevents_destination_replacement() {
        let parent = tempfile::tempdir().unwrap();
        let original = parent.path().join("run-destination");
        let moved = parent.path().join("moved-destination");
        fs::create_dir(&original).unwrap();
        let guard = DirectoryGuard::open(&original).unwrap();
        assert!(fs::rename(&original, &moved).is_err());
        assert!(original.is_dir());
        drop(guard);
        fs::rename(&original, &moved).unwrap();
        assert!(moved.is_dir());
    }
}
