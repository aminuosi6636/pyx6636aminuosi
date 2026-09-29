use super::{
    library::{self, LibraryRuntime},
    scan,
};
use crate::db::Database;
use std::{
    fs,
    sync::{atomic::AtomicBool, Arc, Mutex},
    time::Duration,
};

fn fixture() -> (tempfile::TempDir, LibraryRuntime, std::path::PathBuf) {
    let directory = tempfile::tempdir().unwrap();
    let materials = directory.path().join("芷兰素材 (扫描测试)");
    fs::create_dir(&materials).unwrap();
    let database = Database::open(&directory.path().join("database.sqlite")).unwrap();
    let runtime = LibraryRuntime {
        database: Arc::new(Mutex::new(Some(database))),
        scanning: Arc::new(AtomicBool::new(false)),
        scan_issue: Arc::new(Mutex::new(None)),
    };
    library::select(&runtime, &materials).unwrap();
    (directory, runtime, materials)
}
#[test]
fn root_selection_persists_and_scan_preserves_original_files() {
    let (directory, runtime, root) = fixture();
    // 仅测试文件扫描；这些字节不是可播放视频。
    fs::write(root.join("原片 中文.MP4"), b"fixture video bytes").unwrap();
    fs::write(root.join("忽略.txt"), b"ignored").unwrap();
    scan::discover(&runtime).unwrap();
    let first = library::status(&runtime).unwrap();
    assert_eq!(
        (
            first.candidates.len(),
            first
                .candidates
                .iter()
                .filter(|c| c.status == "ready")
                .count()
        ),
        (1, 1)
    );
    assert_eq!(
        fs::read(root.join("原片 中文.MP4")).unwrap(),
        b"fixture video bytes"
    );
    let candidate_id = first.candidates[0].id.clone();
    scan::discover(&runtime).unwrap();
    assert_eq!(
        library::status(&runtime).unwrap().candidates[0].id,
        candidate_id
    );
    drop(runtime);
    let reopened = Database::open(&directory.path().join("database.sqlite")).unwrap();
    let count: i64 = reopened
        .connection
        .query_row(
            "SELECT count(*) FROM settings WHERE key='current_library_id'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(count, 1);
}
#[test]
fn copying_file_waits_and_is_ready_only_after_stabilizing() {
    let (_directory, runtime, root) = fixture();
    let path = root.join("正在复制.mp4");
    fs::write(&path, b"initial").unwrap();
    let writer_path = path.clone();
    let writer = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(500));
        fs::write(writer_path, b"finished fixture copy").unwrap();
    });
    scan::discover(&runtime).unwrap();
    writer.join().unwrap();
    assert_eq!(
        library::status(&runtime)
            .unwrap()
            .candidates
            .iter()
            .filter(|c| c.status == "ready")
            .count(),
        0
    );
    assert!(path.exists());
    scan::discover(&runtime).unwrap();
    assert_eq!(
        library::status(&runtime)
            .unwrap()
            .candidates
            .iter()
            .filter(|c| c.status == "ready")
            .count(),
        1
    );
}
#[test]
fn zero_byte_video_is_not_considered_finished() {
    let (_directory, runtime, root) = fixture();
    fs::write(root.join("空视频.mov"), b"").unwrap();
    scan::discover(&runtime).unwrap();
    assert_eq!(
        library::status(&runtime)
            .unwrap()
            .candidates
            .iter()
            .filter(|c| c.status == "ready")
            .count(),
        0
    );
}
#[test]
fn offline_root_preserves_database_and_reports_error() {
    let (directory, runtime, root) = fixture();
    fs::write(root.join("素材.mkv"), b"fixture").unwrap();
    scan::discover(&runtime).unwrap();
    fs::rename(&root, directory.path().join("临时移动素材目录")).unwrap();
    assert!(scan::discover(&runtime).is_err());
    let status = library::status(&runtime).unwrap();
    assert_eq!(
        (
            status.candidates.len(),
            status
                .candidates
                .iter()
                .filter(|c| c.status == "ready")
                .count()
        ),
        (1, 1)
    );
    assert_eq!(status.issue.unwrap().code, "FILESYSTEM");
}
#[test]
fn deleted_candidate_is_retained_for_review() {
    let (_directory, runtime, root) = fixture();
    let path = root.join("候选.avi");
    fs::write(&path, b"fixture").unwrap();
    scan::discover(&runtime).unwrap();
    fs::remove_file(path).unwrap(); // 仅本测试的临时fixture。
    scan::discover(&runtime).unwrap();
    let status = library::status(&runtime).unwrap();
    assert_eq!(status.candidates.len(), 1);
    assert_eq!(status.candidates[0].status, "needs_review");
}
#[test]
fn scan_busy_rejects_overlapping_requests() {
    let (_directory, runtime, _root) = fixture();
    runtime
        .scanning
        .store(true, std::sync::atomic::Ordering::Release);
    assert_eq!(scan::discover(&runtime).unwrap_err().code, "BUSY");
}
