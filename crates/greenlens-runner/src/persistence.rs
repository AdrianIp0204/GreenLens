use anyhow::{bail, ensure, Context, Result};
use fs2::FileExt;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
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
pub(crate) struct DirectoryGuard {
    #[cfg(windows)]
    _file: File,
}
impl DirectoryGuard {
    pub(crate) fn open(path: &Path) -> Result<Self> {
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

/// Create one immutable relative member while retaining checked parent handles.
/// Callers keep their existing project transaction lock; this does not acquire
/// a second root WriterLock or overwrite an existing member.
pub(crate) fn write_new_evidence_bytes(root: &Path, name: &str, bytes: &[u8]) -> Result<()> {
    ensure!(
        bytes.len() as u64 <= MAX_EVIDENCE_FILE_BYTES,
        "Evidence file too large"
    );
    let relative = relative_path(name)?;
    let parent = relative.parent().unwrap_or_else(|| Path::new(""));
    let (_parent, _guards) = create_checked_directories(root, parent)?;
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(root.join(relative))?;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(())
}

/// Verify a bounded sibling temporary before publishing a new file without
/// replacing an existing destination. The checkpoint receives cumulative bytes
/// written and read (at most twice the input length). It may cancel at any point
/// before publication. Once publication succeeds there is no late cancellation
/// error: the caller must report the created copy even if a viewer handoff stops.
pub(crate) fn write_new_verified_evidence_bytes(
    root: &Path,
    name: &str,
    bytes: &[u8],
    expected_sha256: &str,
    checkpoint: &mut impl FnMut(u64) -> Result<()>,
) -> Result<()> {
    ensure!(
        bytes.len() as u64 <= MAX_EVIDENCE_FILE_BYTES,
        "Evidence file too large"
    );
    checkpoint(0)?;
    let relative = relative_path(name)?;
    let parent = relative.parent().unwrap_or_else(|| Path::new(""));
    let (parent, _guards) = create_checked_directories(root, parent)?;
    let target = root.join(relative);
    let temporary = parent.join(format!(".delivery-{}.tmp", Uuid::new_v4()));
    let mut options = OpenOptions::new();
    options.create_new(true).read(true).write(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        use windows_sys::Win32::Storage::FileSystem::{
            DELETE, FILE_GENERIC_READ, FILE_GENERIC_WRITE, FILE_SHARE_DELETE, FILE_SHARE_READ,
        };
        // Deny another writer while retaining the handle through promotion.
        options
            .access_mode(FILE_GENERIC_READ | FILE_GENERIC_WRITE | DELETE)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_DELETE);
    }
    let mut file = options.open(&temporary)?;
    let result = (|| {
        let mut processed = 0u64;
        for chunk in bytes.chunks(64 * 1024) {
            checkpoint(processed)?;
            file.write_all(chunk)?;
            processed += chunk.len() as u64;
            checkpoint(processed)?;
        }
        file.sync_all()?;
        file.seek(SeekFrom::Start(0))?;
        let mut digest = Sha256::new();
        let mut read_bytes = 0u64;
        let mut buffer = [0u8; 64 * 1024];
        loop {
            checkpoint(processed)?;
            // Probe one extra byte to detect growth without an unbounded read.
            let remaining = bytes.len() as u64 + 1 - read_bytes;
            let limit = buffer.len().min(remaining as usize);
            let count = file.read(&mut buffer[..limit])?;
            if count == 0 {
                break;
            }
            read_bytes += count as u64;
            ensure!(
                read_bytes <= bytes.len() as u64,
                "Written copy size changed"
            );
            digest.update(&buffer[..count]);
            processed += count as u64;
            checkpoint(processed)?;
        }
        ensure!(
            read_bytes == bytes.len() as u64
                && format!("{:x}", digest.finalize()) == expected_sha256,
            "Written copy failed integrity verification; no destination was published"
        );
        checkpoint(processed)?;
        promote_new_file(&file, &temporary, &target)?;
        Ok(())
    })();
    #[cfg(windows)]
    if result.is_err() {
        discard_owned_temporary(&file);
    }
    drop(file);
    // Windows publication/cleanup binds the held file, even if another actor
    // renamed its path. The portable hard-link fallback removes its temp name.
    #[cfg(not(windows))]
    let _ = fs::remove_file(&temporary);
    result
}

#[cfg(windows)]
fn rename_info_buffer(target: &Path) -> Result<(Vec<u64>, u32)> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::FILE_RENAME_INFO;
    const _: () = assert!(std::mem::align_of::<u64>() >= std::mem::align_of::<FILE_RENAME_INFO>());
    let name: Vec<u16> = target.as_os_str().encode_wide().collect();
    ensure!(
        !name.is_empty() && !name.contains(&0),
        "Invalid copy destination"
    );
    let name_bytes = name
        .len()
        .checked_mul(std::mem::size_of::<u16>())
        .context("Copy filename too large")?;
    let offset = std::mem::offset_of!(FILE_RENAME_INFO, FileName);
    // FileNameLength excludes the terminator, but FILE_RENAME_INFO documents
    // FileName as NUL-terminated. Reserve it inside the advertised buffer too:
    // allocator/alignment padding is not a reliable string terminator.
    // https://learn.microsoft.com/windows/win32/api/winbase/ns-winbase-file_rename_info
    let size = offset
        .checked_add(name_bytes)
        .and_then(|size| size.checked_add(std::mem::size_of::<u16>()))
        .context("Copy destination path too large")?
        .max(std::mem::size_of::<FILE_RENAME_INFO>());
    let buffer_size: u32 = size.try_into().context("Copy destination path too large")?;
    let name_size: u32 = name_bytes.try_into().context("Copy filename too large")?;
    let mut storage = vec![0u64; size.div_ceil(std::mem::size_of::<u64>())];
    // SAFETY: the zeroed buffer is aligned for FILE_RENAME_INFO and large enough
    // for the variable-length UTF-16 filename plus its reserved zero terminator.
    // ReplaceIfExists remains false and RootDirectory remains null.
    unsafe {
        let info = storage.as_mut_ptr().cast::<FILE_RENAME_INFO>();
        (*info).FileNameLength = name_size;
        std::ptr::copy_nonoverlapping(
            name.as_ptr(),
            storage.as_mut_ptr().cast::<u8>().add(offset).cast::<u16>(),
            name.len(),
        );
    }
    Ok((storage, buffer_size))
}

#[cfg(windows)]
fn promote_new_file(file: &File, _source: &Path, target: &Path) -> Result<()> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Storage::FileSystem::{FileRenameInfo, SetFileInformationByHandle};
    let (storage, buffer_size) = rename_info_buffer(target)?;
    // SAFETY: rename_info_buffer owns the correctly sized/aligned structure
    // through this call. The live handle is the exact file written and hashed
    // above; no pathname lookup can substitute a different source.
    let success = unsafe {
        SetFileInformationByHandle(
            file.as_raw_handle(),
            FileRenameInfo,
            storage.as_ptr().cast(),
            buffer_size,
        )
    };
    if success == 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    Ok(())
}

#[cfg(windows)]
fn discard_owned_temporary(file: &File) {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Storage::FileSystem::{
        FileDispositionInfo, SetFileInformationByHandle, FILE_DISPOSITION_INFO,
    };
    let disposition = FILE_DISPOSITION_INFO { DeleteFile: true };
    // SAFETY: this marks only the owned temporary handle for deletion on close.
    // Failure may leave an unadvertised temp, but never deletes a replaced path.
    let _ = unsafe {
        SetFileInformationByHandle(
            file.as_raw_handle(),
            FileDispositionInfo,
            (&disposition as *const FILE_DISPOSITION_INFO).cast(),
            std::mem::size_of::<FILE_DISPOSITION_INFO>() as u32,
        )
    };
}

#[cfg(not(windows))]
fn promote_new_file(_file: &File, source: &Path, target: &Path) -> Result<()> {
    // A sibling hard link publishes the already verified bytes atomically and
    // refuses an existing name. Temporary cleanup cannot undo this success.
    fs::hard_link(source, target)?;
    Ok(())
}

pub(crate) fn read_limited(mut source: impl Read, limit: u64) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    let probe_limit = limit.checked_add(1).context("Read limit overflow")?;
    let mut limited = source.by_ref().take(probe_limit);
    let mut buffer = [0u8; 64 * 1024];
    loop {
        crate::project_open::checkpoint(0)?;
        let count = match limited.read(&mut buffer) {
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            result => result?,
        };
        if count == 0 {
            break;
        }
        bytes.extend_from_slice(&buffer[..count]);
        crate::project_open::checkpoint(count as u64)?;
    }
    ensure!(
        bytes.len() as u64 <= limit,
        "Evidence exceeds its bounded read limit of {limit} bytes ({} MiB)",
        limit as f64 / (1024.0 * 1024.0)
    );
    Ok(bytes)
}

// Check availability before opening, then bind size/policy to the opened file.
// Every caller must also bound its actual reads against subsequent growth.
fn open_limited_file(path: &Path, limit: u64, too_large: &str) -> Result<File> {
    let meta = checked_metadata(path)?;
    ensure!(
        !meta.file_type().is_symlink() && meta.is_file(),
        "Expected a regular file"
    );
    ensure!(meta.len() <= limit, "{too_large}");
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
    ensure!(opened.len() <= limit, "{too_large}");
    Ok(file)
}

/// The actual read stays bounded if an external writer grows the opened file.
pub fn read_json_bytes(path: &Path) -> Result<Vec<u8>> {
    read_evidence_bytes(path, MAX_EVIDENCE_FILE_BYTES)
}
pub(crate) fn read_evidence_bytes(path: &Path, limit: u64) -> Result<Vec<u8>> {
    ensure!(
        limit <= MAX_EVIDENCE_FILE_BYTES,
        "Evidence read exceeds the shared file bound"
    );
    let file = open_limited_file(
        path,
        limit,
        &format!(
            "Evidence exceeds its bounded read limit of {limit} bytes ({} MiB)",
            limit as f64 / (1024.0 * 1024.0)
        ),
    )?;
    let bytes = read_limited(file, limit)?;
    crate::project_open::observed_file(path, &bytes_hash(&bytes))?;
    Ok(bytes)
}

/// Explicit cooperative checks outside a project-open snapshot, while retaining
/// the same checked Windows file handle, byte limit and observed-content binding.
pub(crate) fn read_evidence_bytes_checked(
    path: &Path,
    limit: u64,
    checkpoint: &mut impl FnMut(u64) -> Result<()>,
) -> Result<Vec<u8>> {
    checkpoint(0)?;
    ensure!(
        limit <= MAX_EVIDENCE_FILE_BYTES,
        "Evidence read exceeds the shared file bound"
    );
    let file = open_limited_file(
        path,
        limit,
        &format!(
            "Evidence exceeds its bounded read limit of {limit} bytes ({} MiB)",
            limit as f64 / (1024.0 * 1024.0)
        ),
    )?;
    let bytes = read_limited(
        CheckpointReader {
            source: file,
            checkpoint,
        },
        limit,
    )?;
    checkpoint(0)?;
    crate::project_open::observed_file(path, &bytes_hash(&bytes))?;
    checkpoint(0)?;
    Ok(bytes)
}

fn hash_limited(source: &mut impl Read, limit: u64) -> Result<String> {
    let probe_limit = limit.checked_add(1).context("Hash limit overflow")?;
    let mut limited = source.take(probe_limit);
    let mut digest = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    let mut total = 0u64;
    loop {
        crate::project_open::checkpoint(0)?;
        let count = match limited.read(&mut buffer) {
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            result => result?,
        };
        if count == 0 {
            break;
        }
        total += count as u64; // bounded by the checked limit + one probe byte
        ensure!(total <= limit, "Evidence file too large");
        digest.update(&buffer[..count]);
        crate::project_open::checkpoint(count as u64)?;
    }
    Ok(format!("{:x}", digest.finalize()))
}

pub(crate) fn evidence_file_hash(path: &Path) -> Result<String> {
    let mut file = open_limited_file(path, MAX_EVIDENCE_FILE_BYTES, "Evidence file too large")?;
    let digest = hash_limited(&mut file, MAX_EVIDENCE_FILE_BYTES)?;
    crate::project_open::observed_file(path, &digest)?;
    Ok(digest)
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
/// Publish an accepted project without allowing an older client/undo snapshot
/// to remove immutable revisions or issued reports. Callers retain WriterLock.
pub fn atomic_project(root: &Path, project: &crate::Project) -> Result<String> {
    atomic_project_cancellable(root, project, &std::sync::atomic::AtomicBool::new(false))
}

pub fn atomic_project_cancellable(
    root: &Path,
    project: &crate::Project,
    cancel: &std::sync::atomic::AtomicBool,
) -> Result<String> {
    ensure!(
        !cancel.load(std::sync::atomic::Ordering::Relaxed),
        "Project publication cancelled; the previous manifest is unchanged"
    );
    // Bind the publication fence before potentially lengthy history validation.
    // An external edit during that work must not become the new baseline.
    let expected = revision(root)?;
    crate::validate_project(project)?;
    if expected.is_some() {
        let previous = crate::history_storage::read_project(root, cancel)?;
        crate::study_revisions::validate_preserved(&previous, project)?;
        crate::report_issues::validate_preserved(&previous, project)?;
        crate::design_report::validate_preserved(&previous, project)?;
    }
    crate::capacity::require_publication(root, project, &[], cancel)?;
    let data = crate::history_storage::manifest_bytes(project)?;
    crate::history_storage::stage(root, project, cancel)?;
    atomic_bytes_cancellable(root, &data, cancel, Some(expected.as_deref()))
}

pub fn atomic_json(root: &Path, value: &impl Serialize) -> Result<String> {
    atomic_json_cancellable(root, value, &std::sync::atomic::AtomicBool::new(false))
}
fn atomic_json_cancellable(
    root: &Path,
    value: &impl Serialize,
    cancel: &std::sync::atomic::AtomicBool,
) -> Result<String> {
    let mut data = serde_json::to_vec_pretty(value)?;
    data.push(b'\n');
    atomic_bytes_cancellable(root, &data, cancel, None)
}

fn atomic_bytes_cancellable(
    root: &Path,
    data: &[u8],
    cancel: &std::sync::atomic::AtomicBool,
    expected: Option<Option<&str>>,
) -> Result<String> {
    let temporary = root.join(format!(".project-{}.tmp", Uuid::new_v4()));
    ensure!(
        data.len() as u64 <= MAX_EVIDENCE_FILE_BYTES,
        "Project manifest exceeds the 64 MiB readable limit. The existing project was retained; save a project with fewer historical runs"
    );
    let _directory = DirectoryGuard::open(root)?;
    let target = fs::canonicalize(root)?.join("project.json");
    let mut options = OpenOptions::new();
    options.create_new(true).write(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        use windows_sys::Win32::Storage::FileSystem::{
            DELETE, FILE_GENERIC_WRITE, FILE_SHARE_READ,
        };
        // Keep the exact written temporary alive through the rename; deny
        // other writers and deletion/renaming of this unpublished candidate.
        options
            .access_mode(FILE_GENERIC_WRITE | DELETE)
            .share_mode(FILE_SHARE_READ);
    }
    let mut file = options.open(&temporary)?;
    file.write_all(data)?;
    file.sync_all()?;
    if cancel.load(std::sync::atomic::Ordering::Relaxed) {
        #[cfg(windows)]
        discard_owned_temporary(&file);
        drop(file);
        #[cfg(not(windows))]
        let _ = fs::remove_file(&temporary);
        bail!("Project publication cancelled; the previous manifest is unchanged");
    }
    if let Some(expected) = expected {
        if let Err(error) = check_revision(root, expected) {
            #[cfg(windows)]
            discard_owned_temporary(&file);
            drop(file);
            #[cfg(not(windows))]
            let _ = fs::remove_file(&temporary);
            return Err(error);
        }
    }
    if cancel.load(std::sync::atomic::Ordering::Relaxed) {
        #[cfg(windows)]
        discard_owned_temporary(&file);
        drop(file);
        #[cfg(not(windows))]
        let _ = fs::remove_file(&temporary);
        bail!("Project publication cancelled; the previous manifest is unchanged");
    }
    atomic_replace(&file, &temporary, &target).with_context(|| {
        format!(
            "Atomic replacement of project.json failed. The complete candidate was written to {} before publication. GreenLens has not retried or removed staged files. Check the current manifest and retained candidate before saving again; folder permissions or another application holding the manifest may block replacement",
            temporary.file_name().unwrap_or_default().to_string_lossy()
        )
    })?;
    Ok(bytes_hash(data))
}
#[cfg(windows)]
fn atomic_replace(file: &File, _source: &Path, target: &Path) -> Result<()> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Storage::FileSystem::{
        FileRenameInfoEx, SetFileInformationByHandle, FILE_RENAME_INFO,
    };
    // Windows SDK FILE_RENAME_FLAG_REPLACE_IF_EXISTS | POSIX_SEMANTICS.
    // Existing readers retain their original snapshot; new opens see the
    // replacement. Legacy MoveFileExW can reject even delete-sharing readers.
    // No ignore-readonly, copy/delete fallback, retry or filesystem weakening.
    // https://learn.microsoft.com/windows-hardware/drivers/ddi/ntifs/ns-ntifs-_file_rename_information
    const REPLACE_WITH_OPEN_READERS: u32 = 0x1 | 0x2;
    // Use the canonical absolute destination beneath the retained directory
    // guard. Win32 resolves relative names against CWD; a nonnull RootDirectory
    // is rejected by the tested FileRenameInfoEx implementation.
    ensure!(
        target.is_absolute(),
        "Manifest destination must be absolute"
    );
    let (mut storage, buffer_size) = rename_info_buffer(target)?;
    // SAFETY: the aligned buffer owns a complete FILE_RENAME_INFO plus its
    // UTF-16 name. The live DELETE-access handle is the file just synced.
    let success = unsafe {
        (*storage.as_mut_ptr().cast::<FILE_RENAME_INFO>())
            .Anonymous
            .Flags = REPLACE_WITH_OPEN_READERS;
        SetFileInformationByHandle(
            file.as_raw_handle(),
            FileRenameInfoEx,
            storage.as_ptr().cast(),
            buffer_size,
        )
    };
    if success == 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    Ok(())
}
#[cfg(not(windows))]
fn atomic_replace(_file: &File, source: &Path, target: &Path) -> Result<()> {
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
            evidence_file_hash(&regular_child(root, name)?)?,
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
            evidence_file_hash(&file)? == *digest,
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

pub(crate) fn promote_attachment(source: &Path, project: &Path, id: &str) -> Result<()> {
    promote_evidence(source, project, "attachments", id)
}
/// Reference identities come from an independently checked manifest, never a
/// seal encountered later during copying. Limits are per member, in bytes.
pub(crate) type BoundReferenceMembers = BTreeMap<String, (String, u64)>;

fn verify_bound_reference_seal(
    root: &Path,
    members: &BoundReferenceMembers,
    checkpoint: &mut impl FnMut(u64) -> Result<()>,
) -> Result<Checksums> {
    checkpoint(0)?;
    ensure!(
        !members.is_empty() && members.len() <= 124,
        "Invalid bound reference member count"
    );
    let expected: BTreeMap<_, _> = members
        .iter()
        .map(|(name, (digest, _))| (name.clone(), digest.clone()))
        .collect();
    let checksums: Checksums = serde_json::from_slice(&read_evidence_bytes_checked(
        &regular_child(root, "checksums.json")?,
        64 * 1024,
        checkpoint,
    )?)?;
    ensure!(
        checksums.schema_version == "0.1.0" && checksums.sha256 == expected,
        "Reference seal differs from bound manifest members or digests"
    );
    // Only independently declared members are opened; an unexpected seal entry
    // cannot cause unrelated file reads, even after a source replacement.
    for (name, (digest, limit)) in members {
        checkpoint(0)?;
        ensure!(
            *limit <= MAX_EVIDENCE_FILE_BYTES,
            "Bound reference member exceeds archive limit"
        );
        let path = regular_child(root, name)?;
        let file = open_limited_file(
            &path,
            *limit,
            "Bound reference member exceeds declared byte limit",
        )?;
        let actual = hash_limited(
            &mut CheckpointReader {
                source: file,
                checkpoint: &mut *checkpoint,
            },
            *limit,
        )?;
        crate::project_open::observed_file(&path, &actual)?;
        ensure!(
            actual == *digest,
            "Reference content differs from bound manifest: {name}"
        );
    }
    Ok(checksums)
}

/// Exact-bound reference promotion uses streaming 64 KiB checks and a checked
/// staging directory. The manifest of the portable project is never written.
pub(crate) fn promote_reference_bound(
    source: &Path,
    project: &Path,
    id: &str,
    members: &BoundReferenceMembers,
    checkpoint: &mut impl FnMut(u64) -> Result<()>,
) -> Result<()> {
    Uuid::parse_str(id).context("Invalid reference identifier")?;
    verify_bound_reference_seal(source, members, checkpoint)?;
    let (references, _destination_guards) =
        create_checked_directories(project, Path::new("references"))?;
    let target = references.join(id);
    if fs::symlink_metadata(&target).is_ok() {
        let _existing_guard = DirectoryGuard::open(&target)?;
        verify_bound_reference_seal(&target, members, checkpoint)?;
        return Ok(());
    }
    let temporary = references.join(format!(".pending-{}", Uuid::new_v4()));
    fs::create_dir(&temporary)?;
    let files = members
        .iter()
        .map(|(name, (_, limit))| (name.as_str(), *limit))
        .chain(std::iter::once(("checksums.json", 64 * 1024)));
    for (name, limit) in files {
        checkpoint(0)?;
        let relative = relative_path(name)?;
        let destination = temporary.join(&relative);
        let parent = relative.parent().unwrap_or_else(|| Path::new(""));
        let (_parent, _parent_guards) = create_checked_directories(&temporary, parent)?;
        let source = open_limited_file(
            &regular_child(source, name)?,
            limit,
            "Bound reference member exceeds declared byte limit",
        )?;
        let mut destination = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(destination)?;
        copy_bounded(
            &mut CheckpointReader {
                source,
                checkpoint: &mut *checkpoint,
            },
            &mut destination,
            limit,
        )?;
        destination.sync_all()?;
        checkpoint(0)?;
    }
    verify_bound_reference_seal(&temporary, members, checkpoint)?;
    checkpoint(0)?;
    fs::rename(temporary, target)?;
    Ok(())
}

/// Immutable report products use content-addressed directories. The bound
/// manifest is independently checked by report_issues before calling this
/// helper; imported seals never choose additional files to read or copy.
pub(crate) fn promote_issue_bound(
    source: &Path,
    project: &Path,
    id: &str,
    members: &BoundReferenceMembers,
    checkpoint: &mut impl FnMut(u64) -> Result<()>,
) -> Result<()> {
    ensure!(
        id.len() == 64
            && id
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
        "Invalid report issue identity"
    );
    ensure!(
        !members.is_empty() && members.len() <= 256,
        "Report issue exceeds 256 retained members"
    );
    let verify = |root: &Path, checkpoint: &mut dyn FnMut(u64) -> Result<()>| -> Result<()> {
        let mut budget = PackageBudget::default();
        let expected: BTreeMap<_, _> = members
            .iter()
            .map(|(name, (digest, _))| (name.clone(), digest.clone()))
            .collect();
        let seal: Checksums = serde_json::from_slice(&read_evidence_bytes_checked(
            &regular_child(root, "checksums.json")?,
            64 * 1024,
            &mut |n| checkpoint(n),
        )?)?;
        ensure!(
            seal.schema_version == "0.1.0" && seal.sha256 == expected,
            "Issue seal differs from immutable product manifest"
        );
        for (name, (digest, limit)) in members {
            budget.add(name, *limit)?;
            let path = regular_child(root, name)?;
            let mut file = open_limited_file(&path, *limit, "Issue product exceeds bound size")?;
            ensure!(
                file.metadata()?.len() == *limit,
                "Issue product size differs: {name}"
            );
            let actual = hash_limited(
                &mut CheckpointReader {
                    source: &mut file,
                    checkpoint: &mut |n| checkpoint(n),
                },
                *limit,
            )?;
            crate::project_open::observed_file(&path, &actual)?;
            ensure!(actual == *digest, "Issue product integrity failure: {name}");
        }
        Ok(())
    };
    verify(source, checkpoint)?;
    let (issues, _guards) = create_checked_directories(project, Path::new("report-issues"))?;
    let target = issues.join(id);
    if fs::symlink_metadata(&target).is_ok() {
        let _guard = DirectoryGuard::open(&target)?;
        verify(&target, checkpoint)?;
        return Ok(());
    }
    let temporary = issues.join(format!(".pending-{}", Uuid::new_v4()));
    fs::create_dir(&temporary)?;
    for (name, limit) in members
        .iter()
        .map(|(n, (_, s))| (n.as_str(), *s))
        .chain(std::iter::once(("checksums.json", 64 * 1024)))
    {
        checkpoint(0)?;
        let relative = relative_path(name)?;
        let (_parent, _guards) = create_checked_directories(
            &temporary,
            relative.parent().unwrap_or_else(|| Path::new("")),
        )?;
        let source = open_limited_file(
            &regular_child(source, name)?,
            limit,
            "Issue source grew during copy",
        )?;
        let mut output = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(temporary.join(relative))?;
        copy_bounded(
            &mut CheckpointReader {
                source,
                checkpoint: &mut *checkpoint,
            },
            &mut output,
            limit,
        )?;
        output.sync_all()?;
    }
    verify(&temporary, checkpoint)?;
    checkpoint(0)?;
    fs::rename(temporary, target)?;
    Ok(())
}

/// Additive design-report codec: independently bound members include its own
/// manifest. No legacy checksum seal is invented or modified.
pub(crate) fn promote_design_report_bound(
    source: &Path,
    project: &Path,
    id: &str,
    members: &BoundReferenceMembers,
    checkpoint: &mut impl FnMut(u64) -> Result<()>,
) -> Result<()> {
    ensure!(
        id.len() == 64
            && id
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
        "Invalid design-report identity"
    );
    ensure!(
        !members.is_empty() && members.len() <= 253 && members.contains_key("design-products.json"),
        "Invalid bounded design-report inventory"
    );
    let verify = |root: &Path, checkpoint: &mut dyn FnMut(u64) -> Result<()>| -> Result<()> {
        let _guard = DirectoryGuard::open(root)?;
        let mut names = std::collections::BTreeSet::new();
        for entry in fs::read_dir(root)? {
            checkpoint(0)?;
            let entry = entry?;
            ensure!(
                names.len() < 253 && entry.file_type()?.is_file(),
                "Unexpected design-report member"
            );
            names.insert(
                entry
                    .file_name()
                    .into_string()
                    .map_err(|_| anyhow::anyhow!("Invalid report filename"))?,
            );
        }
        ensure!(
            names == members.keys().cloned().collect(),
            "Design report has missing or unlisted members"
        );
        let mut budget = PackageBudget::default();
        for (name, (digest, size)) in members {
            ensure!(!name.contains('/'), "Design report products must be flat");
            budget.add(name, *size)?;
            let path = regular_child(root, name)?;
            let mut file = open_limited_file(&path, *size, "Design report member grew")?;
            ensure!(
                file.metadata()?.len() == *size,
                "Design report member size differs"
            );
            let actual = hash_limited(
                &mut CheckpointReader {
                    source: &mut file,
                    checkpoint: &mut |n| checkpoint(n),
                },
                *size,
            )?;
            crate::project_open::observed_file(&path, &actual)?;
            ensure!(
                &actual == digest,
                "Design report member identity differs: {name}"
            );
        }
        Ok(())
    };
    let _source_guard = DirectoryGuard::open(source)?;
    verify(source, checkpoint)?;
    let (reports, _guards) = create_checked_directories(project, Path::new("design-reports"))?;
    let target = reports.join(id);
    if fs::symlink_metadata(&target).is_ok() {
        return verify(&target, checkpoint);
    }
    let temporary = reports.join(format!(".pending-{}", Uuid::new_v4()));
    fs::create_dir(&temporary)?;
    let temporary_guard = DirectoryGuard::open(&temporary)?;
    for (name, (_, size)) in members {
        checkpoint(0)?;
        let relative = relative_path(name)?;
        let input = open_limited_file(
            &regular_child(source, name)?,
            *size,
            "Design source grew during copy",
        )?;
        let mut output = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(temporary.join(relative))?;
        copy_bounded(
            &mut CheckpointReader {
                source: input,
                checkpoint: &mut *checkpoint,
            },
            &mut output,
            *size,
        )?;
        output.sync_all()?;
    }
    verify(&temporary, checkpoint)?;
    checkpoint(0)?;
    ensure!(
        !target.try_exists()?,
        "Design report target appeared; preserved staged products need explicit reconciliation"
    );
    drop(temporary_guard);
    fs::rename(temporary, target)?;
    Ok(())
}

struct CheckpointReader<'a, R, F> {
    source: R,
    checkpoint: &'a mut F,
}
impl<R: Read, F: FnMut(u64) -> Result<()>> Read for CheckpointReader<'_, R, F> {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        (self.checkpoint)(0).map_err(std::io::Error::other)?;
        let length = buffer.len().min(64 * 1024);
        let count = self.source.read(&mut buffer[..length])?;
        (self.checkpoint)(count as u64).map_err(std::io::Error::other)?;
        Ok(count)
    }
}

fn verify_seal_checked(
    root: &Path,
    checkpoint: &mut impl FnMut(u64) -> Result<()>,
) -> Result<Checksums> {
    let checksums_path = regular_child(root, "checksums.json")?;
    let file = open_limited_file(
        &checksums_path,
        MAX_EVIDENCE_FILE_BYTES,
        "Evidence file too large",
    )?;
    let bytes = read_limited(
        CheckpointReader {
            source: file,
            checkpoint,
        },
        MAX_EVIDENCE_FILE_BYTES,
    )?;
    crate::project_open::observed_file(&checksums_path, &bytes_hash(&bytes))?;
    let checksums: Checksums = serde_json::from_slice(&bytes)?;
    ensure!(
        checksums.schema_version == "0.1.0" && !checksums.sha256.is_empty(),
        "Unsupported/empty checksums"
    );
    for (name, digest) in &checksums.sha256 {
        let path = regular_child(root, name)?;
        let file = open_limited_file(&path, MAX_EVIDENCE_FILE_BYTES, "Evidence file too large")?;
        let actual = hash_limited(
            &mut CheckpointReader {
                source: file,
                checkpoint,
            },
            MAX_EVIDENCE_FILE_BYTES,
        )?;
        crate::project_open::observed_file(&path, &actual)?;
        ensure!(actual == *digest, "Evidence integrity failure: {name}");
    }
    Ok(checksums)
}

/// Also bounds a source which grows after its metadata was inspected. A failed
/// destination is staging only and must never be promoted by its caller.
pub fn copy_bounded(
    source: &mut impl std::io::Read,
    destination: &mut impl std::io::Write,
    limit: u64,
) -> Result<u64> {
    let probe_limit = limit.checked_add(1).context("Copy limit overflow")?;
    let copied = std::io::copy(&mut source.take(probe_limit), destination)?;
    ensure!(
        copied <= limit,
        "Evidence source exceeded its byte limit while copying"
    );
    Ok(copied)
}

fn promote_evidence(source: &Path, project: &Path, prefix: &str, id: &str) -> Result<()> {
    promote_evidence_checked(source, project, prefix, id, &mut |_| Ok(()))
}

fn promote_evidence_checked(
    source: &Path,
    project: &Path,
    prefix: &str,
    id: &str,
    checkpoint: &mut impl FnMut(u64) -> Result<()>,
) -> Result<()> {
    Uuid::parse_str(id).context("Invalid evidence identifier")?;
    let sums = verify_seal_checked(source, checkpoint)?;
    let (runs, _destination_guards) = create_checked_directories(project, Path::new(prefix))?;
    let target = runs.join(id);
    if fs::symlink_metadata(&target).is_ok() {
        let _existing_guard = DirectoryGuard::open(&target)?;
        ensure!(
            verify_seal_checked(&target, checkpoint)?.sha256 == sums.sha256,
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
        copy_bounded(
            &mut CheckpointReader {
                source: &mut source,
                checkpoint,
            },
            &mut destination,
            MAX_EVIDENCE_FILE_BYTES,
        )?;
        destination.sync_all()?;
        checkpoint(0)?;
    }
    verify_seal_checked(&temporary, checkpoint)?;
    checkpoint(0)?;
    fs::rename(temporary, target)?;
    Ok(())
}

#[cfg(test)]
mod cancellable_reference_promotion_tests {
    use super::*;
    #[test]
    fn explicitly_checked_evidence_read_stops_after_one_64_kib_block() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("synthetic.bin");
        fs::write(&path, vec![3u8; 512 * 1024]).unwrap();
        let mut observed = 0u64;
        let error = read_evidence_bytes_checked(&path, 512 * 1024, &mut |count| {
            assert!(count <= 64 * 1024);
            observed += count;
            ensure!(count == 0, "Synthetic cancellation after first read block");
            Ok(())
        })
        .unwrap_err();
        assert!(error.to_string().contains("Synthetic cancellation"));
        assert_eq!(observed, 64 * 1024);
        let original = fs::read(&path).unwrap();
        assert_eq!(
            read_evidence_bytes_checked(&path, 512 * 1024, &mut |_| Ok(())).unwrap(),
            original
        );
    }
    #[test]
    fn cancellation_interrupts_a_sealed_member_at_a_bounded_read() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("source");
        let destination = root.path().join("project");
        fs::create_dir(&source).unwrap();
        fs::create_dir(&destination).unwrap();
        fs::write(source.join("synthetic.bin"), vec![0u8; 512 * 1024]).unwrap();
        seal(&source, &["synthetic.bin"]).unwrap();
        let mut observed = 0;
        let members = BoundReferenceMembers::from([(
            "synthetic.bin".into(),
            (
                evidence_file_hash(&source.join("synthetic.bin")).unwrap(),
                512 * 1024,
            ),
        )]);
        let result = promote_reference_bound(
            &source,
            &destination,
            &Uuid::new_v4().to_string(),
            &members,
            &mut |bytes| {
                assert!(bytes <= 64 * 1024);
                observed += bytes;
                ensure!(observed < 128 * 1024, "Synthetic cancellation checkpoint");
                Ok(())
            },
        );
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Synthetic cancellation"));
        assert!(observed < 192 * 1024);
        assert!(!destination.join("references").exists());
    }

    #[test]
    fn bound_copy_rejects_a_consistent_reseal_between_verification_and_copy() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("source");
        let destination = root.path().join("project");
        fs::create_dir(&source).unwrap();
        fs::create_dir(&destination).unwrap();
        let original = vec![3u8; 4096];
        let changed = vec![7u8; 4096];
        fs::write(source.join("synthetic.bin"), &original).unwrap();
        seal(&source, &["synthetic.bin"]).unwrap();
        let members =
            BoundReferenceMembers::from([("synthetic.bin".into(), (bytes_hash(&original), 4096))]);
        let mut read_original = false;
        let mut mutated = false;
        let id = Uuid::new_v4().to_string();
        let error = promote_reference_bound(&source, &destination, &id, &members, &mut |count| {
            if count == original.len() as u64 {
                read_original = true;
            } else if read_original && !mutated && count == 0 {
                // At EOF after the original payload's hash update, replace
                // both bytes and seal coherently. No timing/sleep race.
                fs::write(source.join("synthetic.bin"), &changed)?;
                write_json(
                    &source.join("checksums.json"),
                    &Checksums {
                        schema_version: "0.1.0".into(),
                        sha256: BTreeMap::from([("synthetic.bin".into(), bytes_hash(&changed))]),
                    },
                )?;
                mutated = true;
            }
            Ok(())
        })
        .unwrap_err();
        assert!(mutated, "Mutation checkpoint was never exercised");
        assert!(error.to_string().contains("bound manifest"), "{error:#}");
        assert!(!destination.join("references").join(id).exists());
        assert!(!destination.join("project.json").exists());
    }

    #[test]
    fn existing_reference_target_cannot_substitute_its_own_valid_seal() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("source");
        let destination = root.path().join("project");
        fs::create_dir(&source).unwrap();
        fs::create_dir(&destination).unwrap();
        let original = b"original";
        fs::write(source.join("synthetic.bin"), original).unwrap();
        seal(&source, &["synthetic.bin"]).unwrap();
        let members =
            BoundReferenceMembers::from([("synthetic.bin".into(), (bytes_hash(original), 8))]);
        let id = Uuid::new_v4().to_string();
        promote_reference_bound(&source, &destination, &id, &members, &mut |_| Ok(())).unwrap();
        let target = destination.join("references").join(&id);
        fs::write(target.join("synthetic.bin"), b"modified").unwrap();
        seal(&target, &["synthetic.bin"]).unwrap();
        let error = promote_reference_bound(&source, &destination, &id, &members, &mut |_| Ok(()))
            .unwrap_err();
        assert!(error.to_string().contains("bound manifest"), "{error:#}");
        assert!(!destination.join("project.json").exists());
        assert_eq!(fs::read(source.join("synthetic.bin")).unwrap(), original);
    }
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

    fn design_promotion_fixture(root: &Path) -> BoundReferenceMembers {
        fs::create_dir(root).unwrap();
        // Inert file-copy fixtures, not report content or simulation evidence.
        let files = [
            ("design-products.json", b"inert manifest fixture".to_vec()),
            ("inert-product.bin", vec![7u8; 192 * 1024]),
        ];
        files
            .into_iter()
            .map(|(name, bytes)| {
                fs::write(root.join(name), &bytes).unwrap();
                (name.into(), (bytes_hash(&bytes), bytes.len() as u64))
            })
            .collect()
    }

    #[test]
    fn design_product_promotion_is_exact_idempotent_and_preserves_conflicts() {
        let parent = tempfile::tempdir().unwrap();
        let source = parent.path().join("source 空間");
        let members = design_promotion_fixture(&source);
        let project = parent.path().join("project with spaces");
        fs::create_dir(&project).unwrap();
        fs::write(project.join("project.json"), b"unchanged document").unwrap();
        let id = "a".repeat(64);
        let target = project.join("design-reports").join(&id);
        promote_design_report_bound(&source, &project, &id, &members, &mut |_| Ok(())).unwrap();
        promote_design_report_bound(&source, &project, &id, &members, &mut |_| Ok(())).unwrap();
        for (name, (digest, size)) in &members {
            let copied = fs::read(target.join(name)).unwrap();
            assert_eq!(copied.len() as u64, *size);
            assert_eq!(&bytes_hash(&copied), digest);
        }
        // An equal-size mutation is not a permissible idempotent copy and must
        // never be overwritten by the apparently correct source.
        let changed = vec![8u8; 192 * 1024];
        fs::write(target.join("inert-product.bin"), &changed).unwrap();
        assert!(
            promote_design_report_bound(&source, &project, &id, &members, &mut |_| Ok(())).is_err()
        );
        assert_eq!(fs::read(target.join("inert-product.bin")).unwrap(), changed);
        assert_eq!(
            fs::read(source.join("inert-product.bin")).unwrap(),
            vec![7u8; 192 * 1024]
        );
        fs::write(source.join("undeclared.bin"), b"must not silently copy").unwrap();
        let another = "b".repeat(64);
        assert!(promote_design_report_bound(
            &source,
            &project,
            &another,
            &members,
            &mut |_| Ok(())
        )
        .is_err());
        assert!(!project.join("design-reports").join(another).exists());
        assert_eq!(
            fs::read(project.join("project.json")).unwrap(),
            b"unchanged document"
        );
    }

    #[test]
    fn cancelled_design_promotion_leaves_only_diagnostic_staging() {
        let parent = tempfile::tempdir().unwrap();
        let source = parent.path().join("source");
        let members = design_promotion_fixture(&source);
        let project = parent.path().join("project");
        fs::create_dir(&project).unwrap();
        fs::write(project.join("project.json"), b"unchanged document").unwrap();
        let id = "c".repeat(64);
        let reports = project.join("design-reports");
        let mut cancelled_during_copy = false;
        let error = promote_design_report_bound(&source, &project, &id, &members, &mut |bytes| {
            let staging_exists = reports.is_dir()
                && fs::read_dir(&reports)?.any(|e| {
                    e.is_ok_and(|e| e.file_name().to_string_lossy().starts_with(".pending-"))
                });
            if bytes > 0 && staging_exists {
                cancelled_during_copy = true;
                anyhow::bail!("controlled synthetic cancellation during copy");
            }
            Ok(())
        })
        .unwrap_err();
        assert!(cancelled_during_copy);
        assert!(format!("{error:#}").contains("controlled synthetic cancellation"));
        assert!(!reports.join(&id).exists());
        assert!(fs::read_dir(&reports).unwrap().all(|e| e
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".pending-")));
        assert_eq!(
            fs::read(project.join("project.json")).unwrap(),
            b"unchanged document"
        );
        for (name, (digest, _)) in &members {
            assert_eq!(&bytes_hash(&fs::read(source.join(name)).unwrap()), digest);
        }
    }

    #[cfg(windows)]
    #[test]
    fn rename_buffer_has_explicit_utf16_terminator_for_every_alignment_residue() {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::Storage::FileSystem::FILE_RENAME_INFO;

        let offset = std::mem::offset_of!(FILE_RENAME_INFO, FileName);
        let mut residues = std::collections::BTreeSet::new();
        for parent_padding in 0..8 {
            for leaf in ["issued.pdf", "issued 空間.pdf", "issued 🧱.pdf"] {
                let target = PathBuf::from(format!(
                    "C:\\synthetic parent{}",
                    "p".repeat(parent_padding)
                ))
                .join(leaf);
                let expected: Vec<u16> = target.as_os_str().encode_wide().collect();
                let name_bytes = expected.len() * std::mem::size_of::<u16>();
                residues.insert((offset + name_bytes) % std::mem::size_of::<u64>());
                let (storage, advertised_bytes) = rename_info_buffer(&target).unwrap();
                assert_eq!(
                    storage.as_ptr() as usize % std::mem::align_of::<FILE_RENAME_INFO>(),
                    0
                );
                assert!(advertised_bytes as usize <= std::mem::size_of_val(storage.as_slice()));
                assert!(advertised_bytes as usize >= std::mem::size_of::<FILE_RENAME_INFO>());
                assert!(advertised_bytes as usize >= offset + name_bytes + 2);
                // SAFETY: the builder's allocation and advertised bounds were
                // checked above; the complete UTF-16 slice includes the NUL.
                unsafe {
                    let info = &*storage.as_ptr().cast::<FILE_RENAME_INFO>();
                    assert_eq!(info.FileNameLength as usize, name_bytes);
                    assert!(!info.Anonymous.ReplaceIfExists);
                    assert!(info.RootDirectory.is_null());
                    let actual = std::slice::from_raw_parts(
                        storage.as_ptr().cast::<u8>().add(offset).cast::<u16>(),
                        expected.len() + 1,
                    );
                    assert_eq!(&actual[..expected.len()], expected.as_slice());
                    assert_eq!(actual[expected.len()], 0);
                }
            }
        }
        // Includes the exact-alignment case where the previous buffer had no
        // terminator at all, and each possible UTF-16 alignment-padding case.
        assert_eq!(residues, [0, 2, 4, 6].into_iter().collect());
        assert!(rename_info_buffer(Path::new("")).is_err());
        assert!(rename_info_buffer(Path::new("C:\\synthetic\\bad\0name.pdf")).is_err());
    }

    #[cfg(windows)]
    #[test]
    fn verified_copy_publishes_ascii_and_unicode_names_for_varied_parent_lengths() {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::Storage::FileSystem::FILE_RENAME_INFO;

        let directory = tempfile::tempdir().unwrap();
        let original = b"original synthetic immutable product";
        let replacement = b"must not replace the first publication";
        let mut residues = std::collections::BTreeSet::new();
        for parent_padding in 0..8 {
            let parent = directory
                .path()
                .join(format!("copies {} end", "p".repeat(parent_padding)));
            fs::create_dir(&parent).unwrap();
            for leaf in ["issued.pdf", "issued 空間.pdf", "issued 🧱.pdf"] {
                let target = parent.join(leaf);
                let name_bytes = target.as_os_str().encode_wide().count() * 2;
                residues.insert(
                    (std::mem::offset_of!(FILE_RENAME_INFO, FileName) + name_bytes)
                        % std::mem::size_of::<u64>(),
                );
                write_new_verified_evidence_bytes(
                    &parent,
                    leaf,
                    original,
                    &bytes_hash(original),
                    &mut |_| Ok(()),
                )
                .unwrap_or_else(|error| panic!("publication failed for {target:?}: {error:#}"));
                assert_eq!(fs::read(&target).unwrap(), original);
                assert!(write_new_verified_evidence_bytes(
                    &parent,
                    leaf,
                    replacement,
                    &bytes_hash(replacement),
                    &mut |_| Ok(()),
                )
                .is_err());
                assert_eq!(fs::read(&target).unwrap(), original);
            }
            assert_eq!(fs::read_dir(&parent).unwrap().count(), 3);
        }
        assert_eq!(residues, [0, 2, 4, 6].into_iter().collect());
    }

    #[test]
    fn verified_new_copy_cancellation_during_write_or_hash_never_publishes_partial_bytes() {
        let bytes = vec![17u8; 2 * 64 * 1024 + 3];
        for cancel_at in [64 * 1024, bytes.len() + 64 * 1024] {
            let directory = tempfile::tempdir().unwrap();
            let prior = directory.path().join("already open.pdf");
            fs::write(&prior, b"existing viewer bytes").unwrap();
            let mut observed = 0;
            let result = write_new_verified_evidence_bytes(
                directory.path(),
                "issued copy 空間.pdf",
                &bytes,
                &bytes_hash(&bytes),
                &mut |processed| {
                    observed = processed;
                    ensure!(processed < cancel_at as u64, "synthetic cancellation");
                    Ok(())
                },
            );
            assert!(result.unwrap_err().to_string().contains("cancellation"));
            assert_eq!(observed, cancel_at as u64);
            assert!(!directory.path().join("issued copy 空間.pdf").exists());
            assert_eq!(fs::read(&prior).unwrap(), b"existing viewer bytes");
            assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
        }
    }

    #[test]
    fn verified_new_copy_rejects_wrong_digest_and_concurrent_destination_without_clobbering() {
        let directory = tempfile::tempdir().unwrap();
        let target = directory.path().join("issued.pdf");
        let bytes = b"original synthetic retained product";
        assert!(write_new_verified_evidence_bytes(
            directory.path(),
            "issued.pdf",
            bytes,
            &"0".repeat(64),
            &mut |_| Ok(())
        )
        .is_err());
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 0);
        let mut intervened = false;
        let result = write_new_verified_evidence_bytes(
            directory.path(),
            "issued.pdf",
            bytes,
            &bytes_hash(bytes),
            &mut |processed| {
                if processed == 2 * bytes.len() as u64 && !intervened {
                    fs::write(&target, b"another writer's complete file")?;
                    intervened = true;
                }
                Ok(())
            },
        );
        assert!(result.is_err());
        assert!(intervened);
        assert_eq!(
            fs::read(&target).unwrap(),
            b"another writer's complete file"
        );
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
    }

    #[test]
    fn verified_new_copy_publishes_exact_bytes_and_has_no_late_cancellation_checkpoint() {
        let directory = tempfile::tempdir().unwrap();
        let target = directory.path().join("copies 空間/issued copy.pdf");
        let bytes = vec![23u8; 65_537];
        let mut processed_bytes = 0;
        write_new_verified_evidence_bytes(
            directory.path(),
            "copies 空間/issued copy.pdf",
            &bytes,
            &bytes_hash(&bytes),
            &mut |processed| {
                ensure!(!target.exists(), "late cancellation after publication");
                assert!(processed >= processed_bytes);
                processed_bytes = processed;
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(processed_bytes, 2 * bytes.len() as u64);
        assert_eq!(fs::read(&target).unwrap(), bytes);
        assert_eq!(fs::read_dir(target.parent().unwrap()).unwrap().count(), 1);
    }

    #[cfg(windows)]
    #[test]
    fn verified_copy_publication_and_cleanup_follow_the_verified_handle_not_a_replaced_path() {
        for cancel_after_replacement in [false, true] {
            let directory = tempfile::tempdir().unwrap();
            let moved = directory.path().join("renamed owned temporary.pdf");
            let target = directory.path().join("issued.pdf");
            let bytes = b"verified original synthetic bytes";
            let mut replacement = None;
            let result = write_new_verified_evidence_bytes(
                directory.path(),
                "issued.pdf",
                bytes,
                &bytes_hash(bytes),
                &mut |processed| {
                    if processed == 2 * bytes.len() as u64 && replacement.is_none() {
                        let temporary = fs::read_dir(directory.path())?
                            .next()
                            .context("temporary missing")??
                            .path();
                        fs::rename(&temporary, &moved)?;
                        fs::write(&temporary, b"foreign replacement must not be touched")?;
                        replacement = Some(temporary);
                        ensure!(!cancel_after_replacement, "synthetic cancellation");
                    }
                    Ok(())
                },
            );
            if cancel_after_replacement {
                assert!(result.unwrap_err().to_string().contains("cancellation"));
                assert!(!target.exists());
            } else {
                result.unwrap();
                assert_eq!(fs::read(&target).unwrap(), bytes);
            }
            assert!(!moved.exists());
            assert_eq!(
                fs::read(replacement.unwrap()).unwrap(),
                b"foreign replacement must not be touched"
            );
        }
    }

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
                    energy_result: None,
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
    fn bounded_hash_preserves_digests_and_stops_an_unending_source() {
        struct Unending {
            bytes_read: usize,
        }
        impl Read for Unending {
            fn read(&mut self, output: &mut [u8]) -> std::io::Result<usize> {
                output.fill(7);
                self.bytes_read += output.len();
                Ok(output.len())
            }
        }
        let mut growing = Unending { bytes_read: 0 };
        assert!(hash_limited(&mut growing, 31)
            .unwrap_err()
            .to_string()
            .contains("too large"));
        assert_eq!(growing.bytes_read, 32);
        // Cover empty, exact-limit and multiple fixed-buffer reads without
        // changing the historical lowercase SHA-256 encoding.
        for size in [0, 31, 65_537] {
            let bytes = vec![7u8; size];
            assert_eq!(
                hash_limited(&mut &bytes[..], size as u64).unwrap(),
                bytes_hash(&bytes)
            );
        }
    }

    #[test]
    fn bounded_hash_rejects_read_failure_instead_of_a_partial_digest() {
        struct FailsAfterPrefix {
            supplied: bool,
        }
        impl Read for FailsAfterPrefix {
            fn read(&mut self, output: &mut [u8]) -> std::io::Result<usize> {
                if self.supplied {
                    return Err(std::io::Error::other("synthetic read failure"));
                }
                self.supplied = true;
                output[0] = 7;
                Ok(1)
            }
        }
        let error = hash_limited(&mut FailsAfterPrefix { supplied: false }, 31).unwrap_err();
        assert!(error.to_string().contains("synthetic read failure"));
    }

    #[test]
    fn oversized_evidence_cannot_replace_seal_or_be_promoted() {
        let source = tempfile::tempdir().unwrap();
        let project = tempfile::tempdir().unwrap();
        let path = source.path().join("data.bin");
        fs::write(&path, b"retained evidence").unwrap();
        seal(source.path(), &["data.bin"]).unwrap();
        let original_seal = fs::read(source.path().join("checksums.json")).unwrap();
        let id = Uuid::new_v4().to_string();
        OpenOptions::new()
            .write(true)
            .open(&path)
            .unwrap()
            .set_len(MAX_EVIDENCE_FILE_BYTES + 1)
            .unwrap();
        assert!(seal(source.path(), &["data.bin"])
            .unwrap_err()
            .to_string()
            .contains("too large"));
        assert!(verify_seal(source.path())
            .unwrap_err()
            .to_string()
            .contains("too large"));
        assert!(promote_run(source.path(), project.path(), &id).is_err());
        assert_eq!(
            fs::read(source.path().join("checksums.json")).unwrap(),
            original_seal
        );
        assert!(!project.path().join("runs").exists());
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
    fn final_publication_fence_preserves_external_change_and_cleans_temporary() {
        let root = tempfile::tempdir().unwrap();
        let expected = atomic_json(root.path(), &vec![1]).unwrap();
        atomic_json(root.path(), &vec![2]).unwrap();
        let before = fs::read(root.path().join("project.json")).unwrap();
        let result = atomic_bytes_cancellable(
            root.path(),
            b"[3]\n",
            &std::sync::atomic::AtomicBool::new(false),
            Some(Some(&expected)),
        );
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("changed externally"));
        assert_eq!(fs::read(root.path().join("project.json")).unwrap(), before);
        assert_eq!(fs::read_dir(root.path()).unwrap().count(), 1);
        assert!(atomic_bytes_cancellable(
            root.path(),
            b"[3]\n",
            &std::sync::atomic::AtomicBool::new(true),
            Some(Some(&expected))
        )
        .is_err());
        assert_eq!(fs::read(root.path().join("project.json")).unwrap(), before);
        assert_eq!(fs::read_dir(root.path()).unwrap().count(), 1);
    }
    #[cfg(windows)]
    #[test]
    fn manifest_reader_without_delete_sharing_rejects_publication_without_losing_candidate() {
        use std::os::windows::fs::OpenOptionsExt;
        use windows_sys::Win32::Storage::FileSystem::{FILE_SHARE_READ, FILE_SHARE_WRITE};
        let root = tempfile::tempdir().unwrap();
        let manifest = root.path().join("project.json");
        let old = b"[1]\n";
        let candidate = b"[2]\n";
        fs::write(&manifest, old).unwrap();
        let reader = OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
            .open(&manifest)
            .unwrap();
        let error = atomic_bytes_cancellable(
            root.path(),
            candidate,
            &std::sync::atomic::AtomicBool::new(false),
            Some(Some(&bytes_hash(old))),
        )
        .unwrap_err();
        let raw = error
            .downcast_ref::<std::io::Error>()
            .unwrap()
            .raw_os_error();
        eprintln!("Synthetic denied-delete reader: Windows error {raw:?}");
        // Windows may report access denied or a sharing violation for a
        // replacement whose destination is held without delete sharing.
        assert!(matches!(raw, Some(5 | 32)), "{error:#}");
        assert!(error
            .to_string()
            .contains("Atomic replacement of project.json failed"));
        assert!(error.to_string().contains("has not retried or removed"));
        assert_eq!(fs::read(&manifest).unwrap(), old);
        let retained: Vec<_> = fs::read_dir(root.path())
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .filter(|path| path.extension().is_some_and(|ext| ext == "tmp"))
            .collect();
        assert_eq!(retained.len(), 1);
        assert_eq!(fs::read(&retained[0]).unwrap(), candidate);
        drop(reader);
        // A separate explicit caller attempt is possible after the blocker is
        // gone. The old failed candidate is not deleted or silently promoted.
        atomic_bytes_cancellable(
            root.path(),
            candidate,
            &std::sync::atomic::AtomicBool::new(false),
            Some(Some(&bytes_hash(old))),
        )
        .unwrap();
        assert_eq!(fs::read(&manifest).unwrap(), candidate);
        assert_eq!(fs::read(&retained[0]).unwrap(), candidate);
    }
    #[cfg(windows)]
    #[test]
    fn delete_sharing_observer_does_not_block_manifest_replacement() {
        use std::os::windows::fs::OpenOptionsExt;
        use windows_sys::Win32::Storage::FileSystem::{
            FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE,
        };
        let parent = tempfile::tempdir().unwrap();
        let root = parent.path().join("project with spaces 空間");
        fs::create_dir(&root).unwrap();
        let manifest = root.join("project.json");
        fs::write(&manifest, b"[1]\n").unwrap();
        let mut reader = OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
            .open(&manifest)
            .unwrap();
        atomic_bytes_cancellable(
            &root,
            b"[2]\n",
            &std::sync::atomic::AtomicBool::new(false),
            Some(Some(&bytes_hash(b"[1]\n"))),
        )
        .unwrap();
        let mut opened_bytes = Vec::new();
        reader.read_to_end(&mut opened_bytes).unwrap();
        assert_eq!(opened_bytes, b"[1]\n");
        assert_eq!(fs::read(&manifest).unwrap(), b"[2]\n");
        assert_eq!(fs::read_dir(&root).unwrap().count(), 1);
        assert_eq!(fs::read_dir(parent.path()).unwrap().count(), 1);
    }
    #[cfg(windows)]
    #[test]
    fn readonly_manifest_rejects_replacement_and_preserves_complete_candidate() {
        let root = tempfile::tempdir().unwrap();
        let manifest = root.path().join("project.json");
        fs::write(&manifest, b"[1]\n").unwrap();
        let original_permissions = fs::metadata(&manifest).unwrap().permissions();
        let mut readonly = original_permissions.clone();
        readonly.set_readonly(true);
        fs::set_permissions(&manifest, readonly).unwrap();
        let result = atomic_bytes_cancellable(
            root.path(),
            b"[2]\n",
            &std::sync::atomic::AtomicBool::new(false),
            Some(Some(&bytes_hash(b"[1]\n"))),
        );
        // Restore only this synthetic fixture's original permissions for cleanup.
        fs::set_permissions(&manifest, original_permissions).unwrap();
        let error = result.unwrap_err();
        assert_eq!(
            error
                .downcast_ref::<std::io::Error>()
                .unwrap()
                .raw_os_error(),
            Some(5)
        );
        assert!(error.to_string().contains("has not retried or removed"));
        assert_eq!(fs::read(&manifest).unwrap(), b"[1]\n");
        let retained: Vec<_> = fs::read_dir(root.path())
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .filter(|path| path.extension().is_some_and(|ext| ext == "tmp"))
            .collect();
        assert_eq!(retained.len(), 1);
        assert_eq!(fs::read(&retained[0]).unwrap(), b"[2]\n");
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
