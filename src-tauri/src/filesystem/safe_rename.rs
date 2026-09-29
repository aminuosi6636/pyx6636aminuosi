//! 本模块从不删除素材，也绝不降级为可能覆盖目标的 std::fs::rename。
use crate::{domain::error::AppError, filesystem::source};
use std::{
    fs::{File, OpenOptions},
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
};

pub struct LockedSource {
    file: File,
    path: PathBuf,
    identity: String,
}

impl LockedSource {
    pub fn path(&self) -> &Path {
        &self.path
    }
    pub fn open(
        path: &Path,
        root: &Path,
        expected_hash: &str,
        expected_identity: Option<&str>,
        expected_size: u64,
    ) -> Result<Self, AppError> {
        let canonical = path.canonicalize().map_err(AppError::file_operation)?;
        let root = root.canonicalize().map_err(AppError::file_operation)?;
        if !canonical.starts_with(&root)
            || canonical == root
            || std::fs::symlink_metadata(path)
                .map_err(AppError::file_operation)?
                .file_type()
                .is_symlink()
        {
            return Err(AppError::review("source outside material root or symlink"));
        }
        let mut options = OpenOptions::new();
        options.read(true);
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            use windows_sys::Win32::{
                Foundation::GENERIC_READ,
                Storage::FileSystem::{DELETE, FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_READ},
            };
            // 持有 READ + DELETE，拒绝共享写入/删除：复制进程未关闭时不会取得句柄。
            options
                .access_mode(GENERIC_READ | DELETE)
                .share_mode(FILE_SHARE_READ)
                .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.custom_flags(libc::O_NOFOLLOW);
        }
        let mut file = options.open(&canonical).map_err(AppError::file_operation)?;
        #[cfg(unix)]
        {
            use std::os::fd::AsRawFd;
            if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
                return Err(AppError::file_operation(std::io::Error::last_os_error()));
            }
        }
        let identity = source::identity(&file)?
            .ok_or_else(|| AppError::review("filesystem identity unavailable"))?;
        if expected_identity.is_some_and(|expected| expected != identity) {
            return Err(AppError::review("file identity changed"));
        }
        let metadata = file.metadata().map_err(AppError::file_operation)?;
        if !metadata.is_file() || metadata.len() != expected_size {
            return Err(AppError::review("file size changed"));
        }
        if hash(&mut file)? != expected_hash {
            return Err(AppError::review("file fingerprint changed"));
        }
        let after = file.metadata().map_err(AppError::file_operation)?;
        if after.len() != metadata.len()
            || source::modified(&after)? != source::modified(&metadata)?
        {
            return Err(AppError::review("source changed during validation"));
        }
        Ok(Self {
            file,
            path: canonical,
            identity,
        })
    }

    pub fn rename_to(&mut self, destination: &Path) -> Result<(), AppError> {
        let parent = self
            .path
            .parent()
            .ok_or_else(|| AppError::review("missing parent"))?
            .to_owned();
        if destination
            .parent()
            .ok_or_else(|| AppError::review("missing destination parent"))?
            .canonicalize()
            .map_err(AppError::file_operation)?
            != parent
        {
            return Err(AppError::review("rename must stay in same directory"));
        }
        let destination = parent.join(
            destination
                .file_name()
                .ok_or_else(|| AppError::review("missing filename"))?,
        );
        if destination == self.path {
            return Err(AppError::review("source equals destination"));
        }
        self.verify_path()?;
        rename_exclusive(&self.file, &self.path, &destination).map_err(AppError::file_operation)?;
        self.path = destination;
        self.verify_path()?;
        #[cfg(unix)]
        File::open(&parent)
            .and_then(|file| file.sync_all())
            .map_err(AppError::file_operation)?;
        log::info!("exclusive rename completed: {}", self.path.display());
        Ok(())
    }

    fn verify_path(&self) -> Result<(), AppError> {
        // Unix 锁是协作锁；在系统调用前后重新检查路径与打开句柄是否指向同一文件。
        let metadata = std::fs::symlink_metadata(&self.path).map_err(AppError::file_operation)?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(AppError::review("source type changed"));
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            if format!("{}:{}", metadata.dev(), metadata.ino()) != self.identity {
                return Err(AppError::review("path no longer matches opened source"));
            }
        }
        #[cfg(windows)]
        if source::identity(&self.file)?.as_deref() != Some(&self.identity) {
            return Err(AppError::review("opened source identity changed"));
        }
        Ok(())
    }
}

fn hash(file: &mut File) -> Result<String, AppError> {
    file.seek(SeekFrom::Start(0))
        .map_err(AppError::file_operation)?;
    let mut hasher = blake3::Hasher::new();
    let mut buffer = [0_u8; 65536];
    loop {
        let count = file.read(&mut buffer).map_err(AppError::file_operation)?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    Ok(hasher.finalize().to_hex().to_string())
}

#[cfg(windows)]
fn rename_exclusive(file: &File, _old: &Path, new: &Path) -> std::io::Result<()> {
    use std::os::windows::{ffi::OsStrExt, io::AsRawHandle};
    use windows_sys::Win32::Storage::FileSystem::{
        FileRenameInfo, SetFileInformationByHandle, FILE_RENAME_INFO,
    };
    let wide: Vec<u16> = new.as_os_str().encode_wide().chain(Some(0)).collect();
    let bytes = std::mem::offset_of!(FILE_RENAME_INFO, FileName) + wide.len() * 2;
    // usize buffer 保证 FILE_RENAME_INFO 对齐；全零初始化保证 ReplaceIfExists=false。
    let mut buffer = vec![0_usize; bytes.div_ceil(std::mem::size_of::<usize>())];
    let info = buffer.as_mut_ptr().cast::<FILE_RENAME_INFO>();
    unsafe {
        (*info).FileNameLength = ((wide.len() - 1) * 2)
            .try_into()
            .map_err(|_| std::io::Error::other("filename too long"))?;
        std::ptr::copy_nonoverlapping(
            wide.as_ptr(),
            std::ptr::addr_of_mut!((*info).FileName).cast::<u16>(),
            wide.len(),
        );
        if SetFileInformationByHandle(
            file.as_raw_handle().cast(),
            FileRenameInfo,
            info.cast(),
            bytes
                .try_into()
                .map_err(|_| std::io::Error::other("rename buffer too large"))?,
        ) == 0
        {
            return Err(std::io::Error::last_os_error());
        }
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn rename_exclusive(_file: &File, old: &Path, new: &Path) -> std::io::Result<()> {
    use std::{ffi::CString, os::unix::ffi::OsStrExt};
    let old = CString::new(old.as_os_str().as_bytes())?;
    let new = CString::new(new.as_os_str().as_bytes())?;
    // 不支持 RENAME_EXCL 的文件系统直接报错，不使用普通 rename 作为替代。
    if unsafe { libc::renamex_np(old.as_ptr(), new.as_ptr(), libc::RENAME_EXCL) } != 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(not(any(windows, target_os = "macos")))]
fn rename_exclusive(_file: &File, _old: &Path, _new: &Path) -> std::io::Result<()> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "only Windows/macOS supported",
    ))
}
