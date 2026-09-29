use crate::domain::error::AppError;
use std::{
    fs::{File, Metadata},
    io::Read,
    path::{Path, PathBuf},
    time::UNIX_EPOCH,
};

#[derive(Clone)]
pub struct Source {
    pub path: PathBuf,
    pub size: u64,
    pub modified: String,
    pub filename: String,
    pub extension: String,
}

pub fn path_text(path: &Path) -> Result<String, AppError> {
    path.to_str()
        .map(str::to_owned)
        .ok_or_else(|| AppError::filesystem("path is not valid Unicode"))
}
pub fn modified(metadata: &Metadata) -> Result<String, AppError> {
    Ok(metadata
        .modified()
        .map_err(AppError::filesystem)?
        .duration_since(UNIX_EPOCH)
        .map_err(AppError::filesystem)?
        .as_nanos()
        .to_string())
}
pub fn collect(root: &Path) -> Result<Vec<Source>, AppError> {
    if !root.is_dir() {
        return Err(AppError::filesystem("root directory is unavailable"));
    }
    let mut files = Vec::new();
    for entry in walkdir::WalkDir::new(root).follow_links(false) {
        let entry = entry.map_err(AppError::filesystem)?;
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path();
        let extension = path
            .extension()
            .and_then(|x| x.to_str())
            .unwrap_or("")
            .to_lowercase();
        if !["mp4", "mov", "m4v", "avi", "mkv"].contains(&extension.as_str()) {
            continue;
        }
        let metadata = entry.metadata().map_err(AppError::filesystem)?;
        files.push(Source {
            path: path.to_owned(),
            size: metadata.len(),
            modified: modified(&metadata)?,
            filename: path
                .file_name()
                .and_then(|x| x.to_str())
                .ok_or_else(|| AppError::filesystem("invalid filename"))?
                .into(),
            extension,
        });
    }
    files.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(files)
}
pub fn unchanged(source: &Source) -> bool {
    std::fs::symlink_metadata(&source.path)
        .ok()
        .filter(|m| m.is_file() && !m.file_type().is_symlink())
        .is_some_and(|metadata| {
            source.size > 0
                && metadata.len() == source.size
                && modified(&metadata).ok().as_deref() == Some(source.modified.as_str())
        })
}
pub fn fingerprint(source: &Source) -> Result<(String, Option<String>), AppError> {
    let mut file = File::open(&source.path).map_err(AppError::filesystem)?;
    let identity = identity(&file)?;
    let mut hasher = blake3::Hasher::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let count = file.read(&mut buffer).map_err(AppError::filesystem)?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    if !unchanged(source) {
        return Err(AppError::filesystem("file changed during fingerprinting"));
    }
    Ok((hasher.finalize().to_hex().to_string(), identity))
}
#[cfg(windows)]
pub fn identity(file: &File) -> Result<Option<String>, AppError> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Storage::FileSystem::{
        GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION,
    };
    let mut info: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
    if unsafe { GetFileInformationByHandle(file.as_raw_handle().cast(), &mut info) } == 0 {
        return Err(AppError::filesystem(std::io::Error::last_os_error()));
    }
    Ok(Some(format!(
        "{}:{}:{}",
        info.dwVolumeSerialNumber, info.nFileIndexHigh, info.nFileIndexLow
    )))
}
#[cfg(unix)]
pub fn identity(file: &File) -> Result<Option<String>, AppError> {
    use std::os::unix::fs::MetadataExt;
    let metadata = file.metadata().map_err(AppError::filesystem)?;
    Ok(Some(format!("{}:{}", metadata.dev(), metadata.ino())))
}
