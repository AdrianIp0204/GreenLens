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

pub fn read_json<T: DeserializeOwned>(path: &Path) -> Result<T> {
    let meta = checked_metadata(path)?;
    ensure!(
        !meta.file_type().is_symlink() && meta.is_file(),
        "Expected a regular file"
    );
    ensure!(
        meta.len() <= 64 * 1024 * 1024,
        "JSON exceeds 64 MiB safety limit"
    );
    Ok(serde_json::from_slice(&fs::read(path)?)?)
}
pub fn write_json(path: &Path, value: &impl Serialize) -> Result<()> {
    let mut data = serde_json::to_vec_pretty(value)?;
    data.push(b'\n');
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
    if path.exists() {
        Ok(Some(bytes_hash(&fs::read(path)?)))
    } else {
        Ok(None)
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
            fs::metadata(&file)?.len() <= 64 * 1024 * 1024,
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
    Uuid::parse_str(id).context("Invalid run identifier")?;
    let sums = verify_seal(source)?;
    let (runs, _destination_guards) = create_checked_directories(project, Path::new("runs"))?;
    let target = runs.join(id);
    if fs::symlink_metadata(&target).is_ok() {
        let _existing_guard = DirectoryGuard::open(&target)?;
        ensure!(
            verify_seal(&target)?.sha256 == sums.sha256,
            "Run identity conflict"
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
    let temporary = destination.with_extension(format!("{}.tmp", Uuid::new_v4()));
    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)?;
    let mut zip = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    for name in files {
        let path = regular_child(root, name)?;
        zip.start_file(name, options)?;
        let mut file = File::open(path)?;
        std::io::copy(&mut file, &mut zip)?;
    }
    zip.finish()?.sync_all()?;
    fs::rename(temporary, destination)?;
    Ok(())
}

pub fn extract_package(package: &Path, destination: &Path) -> Result<()> {
    ensure!(!destination.exists(), "Extraction destination must be new");
    let mut zip = zip::ZipArchive::new(File::open(package)?)?;
    ensure!(zip.len() <= 5000, "Evidence ZIP contains too many entries");
    let mut paths = std::collections::HashSet::new();
    let mut total = 0u64;
    for i in 0..zip.len() {
        let item = zip.by_index(i)?;
        relative_path(item.name())?;
        ensure!(
            !item.is_dir(),
            "Evidence ZIP contains unexpected directory entries"
        );
        ensure!(
            paths.insert(item.name().to_ascii_lowercase()),
            "Duplicate ZIP entry"
        );
        ensure!(
            item.unix_mode().is_none_or(|m| m & 0o170000 != 0o120000),
            "ZIP symlinks are forbidden"
        );
        ensure!(
            item.size() <= 64 * 1024 * 1024,
            "ZIP entry exceeds size limit"
        );
        total = total
            .checked_add(item.size())
            .context("ZIP size overflow")?;
        ensure!(total <= 256 * 1024 * 1024, "ZIP exceeds total size limit");
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
            &mut Read::by_ref(&mut item).take(64 * 1024 * 1024 + 1),
            &mut out,
        )?;
        if copied > 64 * 1024 * 1024 {
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
            actual_total <= 256 * 1024 * 1024,
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
        paths == covered,
        "Evidence checksum coverage differs from package contents"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
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
