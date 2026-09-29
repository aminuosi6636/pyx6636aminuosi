use serde::Serialize;
use std::fmt;

#[derive(Debug, Clone, Serialize)]
pub struct AppError {
    pub code: &'static str,
    pub message: String,
    pub retryable: bool,
}

impl AppError {
    pub fn review(detail: impl fmt::Display) -> Self {
        log::error!("manual review: {detail}");
        Self { code: "NEEDS_REVIEW", message: "素材状态无法安全确认，已停止改名。请检查文件是否被替换、移动或存在同名文件，再联系维护人员检查日志。".into(), retryable: false }
    }
    pub fn file_operation(error: std::io::Error) -> Self {
        log::error!("file operation: {error:?}");
        let (code, message) = match error.kind() {
            std::io::ErrorKind::NotFound => ("FILE_MISSING", "视频或素材目录已不存在，请重新扫描并检查磁盘连接。"),
            std::io::ErrorKind::AlreadyExists => ("NAME_CONFLICT", "目标文件名已经存在，软件已停止改名，不会覆盖已有视频。"),
            std::io::ErrorKind::PermissionDenied => ("FILE_PERMISSION", "视频可能正在被剪映或播放器使用，或没有改名权限。请关闭相关程序并检查文件夹权限后重试。"),
            _ if matches!(error.raw_os_error(), Some(32 | 33)) => ("FILE_BUSY", "视频当前正在被其他程序使用，请关闭剪映或播放器后重试。"),
            _ => ("FILE_OPERATION", "视频改名未完成，请检查文件占用、磁盘空间和文件夹权限后重试。"),
        };
        Self {
            code,
            message: message.into(),
            retryable: true,
        }
    }
    pub fn filesystem(detail: impl fmt::Display) -> Self {
        log::error!("filesystem: {detail}");
        Self {
            code: "FILESYSTEM",
            message: "素材目录或文件暂时无法读取。请检查路径、磁盘连接和文件夹权限后重新扫描。"
                .into(),
            retryable: true,
        }
    }
    pub fn busy() -> Self {
        Self {
            code: "BUSY",
            message: "扫描正在进行，请稍候。".into(),
            retryable: true,
        }
    }
    pub fn database(detail: impl fmt::Display) -> Self {
        log::error!("database: {detail}");
        Self {
            code: "DATABASE",
            message: "本地数据库未能打开或校验失败。请联系维护人员检查日志；软件已停止数据库操作。"
                .into(),
            retryable: false,
        }
    }

    pub fn unsupported_version() -> Self {
        Self {
            code: "DATABASE_VERSION",
            message: "这个数据库由较新版本的软件创建，请使用对应的新版本打开。".into(),
            retryable: false,
        }
    }

    pub fn storage(detail: impl fmt::Display) -> Self {
        log::error!("storage: {detail}");
        Self {
            code: "STORAGE",
            message: "软件无法访问本机数据目录。请检查磁盘空间和文件夹权限，再重新打开软件。"
                .into(),
            retryable: false,
        }
    }
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}
impl std::error::Error for AppError {}
