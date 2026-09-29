use super::{
    count,
    ingest::{self, Candidate},
    library::{self, LibraryRuntime},
    rename::{self, Operation},
    scan,
};
use crate::{
    db::{with_database, Database},
    filesystem::{safe_rename::LockedSource, source},
};
use rusqlite::params;
use std::{
    fs,
    path::PathBuf,
    sync::{atomic::AtomicBool, Arc, Mutex},
};

fn fixture() -> (tempfile::TempDir, LibraryRuntime, PathBuf) {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("中文视频 (改名验收)");
    fs::create_dir(&root).unwrap();
    let database = Database::open(&directory.path().join("database.sqlite")).unwrap();
    let runtime = LibraryRuntime {
        database: Arc::new(Mutex::new(Some(database))),
        scanning: Arc::new(AtomicBool::new(false)),
        scan_issue: Arc::new(Mutex::new(None)),
    };
    library::select(&runtime, &root).unwrap();
    let root = root.canonicalize().unwrap();
    (directory, runtime, root)
}
fn candidate(root: &std::path::Path, name: &str) -> Candidate {
    let path = root.join(name);
    fs::write(&path, b"scanner fixture, not a playable video").unwrap();
    let file = fs::File::open(&path).unwrap();
    let identity = source::identity(&file).unwrap().unwrap();
    drop(file);
    let size = fs::metadata(&path).unwrap().len();
    Candidate {
        id: uuid::Uuid::new_v4().to_string(),
        path,
        original: name.into(),
        extension: "mp4".into(),
        size,
        hash: blake3::hash(b"scanner fixture, not a playable video")
            .to_hex()
            .to_string(),
        identity,
    }
}
fn prepare(runtime: &LibraryRuntime, root: &std::path::Path, name: &str, stamp: &str) -> Operation {
    let candidate = candidate(root, name);
    let id = library::active(runtime).unwrap().unwrap().0;
    ingest::prepare(runtime, &id, root, &candidate, stamp).unwrap()
}
fn complete(runtime: &LibraryRuntime, op: &Operation) {
    let file = LockedSource::open(
        &op.old_path,
        &op.root,
        &op.fingerprint,
        Some(&op.identity),
        op.size,
    )
    .unwrap();
    rename::execute(runtime, op, file).unwrap();
}
fn video_state(runtime: &LibraryRuntime, id: &str) -> (String, u32, String) {
    with_database(&runtime.database, |c| {
        c.query_row(
            "SELECT current_path,use_count,status FROM videos WHERE id=?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .map_err(crate::domain::error::AppError::database)
    })
    .unwrap()
}
fn set(
    runtime: &LibraryRuntime,
    id: &str,
    old: u32,
    new: u32,
) -> Result<(), crate::domain::error::AppError> {
    count::set(runtime, id, old, new, &uuid::Uuid::new_v4().to_string())
}

#[test]
fn same_second_files_are_unique_and_next_day_uses_new_timestamp() {
    let (_directory, runtime, root) = fixture();
    let first = prepare(&runtime, &root, "甲.mp4", "2026-09-28_14-35-08");
    complete(&runtime, &first);
    let second = prepare(&runtime, &root, "乙.mp4", "2026-09-28_14-35-08");
    complete(&runtime, &second);
    let third = prepare(&runtime, &root, "丙.mp4", "2026-09-29_00-00-01");
    complete(&runtime, &third);
    assert_eq!(
        first.new_path.file_name().unwrap(),
        "2026-09-28_14-35-08.mp4"
    );
    assert_eq!(
        second.new_path.file_name().unwrap(),
        "2026-09-28_14-35-08_001.mp4"
    );
    assert_eq!(
        third.new_path.file_name().unwrap(),
        "2026-09-29_00-00-01.mp4"
    );
    assert_eq!(
        library::status(&runtime).unwrap().videos[0].id,
        third.video_id
    );
    assert!(first.new_path.exists() && second.new_path.exists());
}
#[test]
fn absolute_counts_can_increase_decrease_and_return_to_zero() {
    let (_directory, runtime, root) = fixture();
    let op = prepare(&runtime, &root, "素材.mp4", "2026-09-28_14-35-08");
    complete(&runtime, &op);
    for (old, new) in [(0, 1), (1, 5), (5, 2), (2, 0)] {
        set(&runtime, &op.video_id, old, new).unwrap();
        let (path, count, status) = video_state(&runtime, &op.video_id);
        assert_eq!(count, new);
        assert_eq!(status, "ready");
        assert!(PathBuf::from(&path).exists());
        assert!(path.ends_with(&format!("2026-09-28_14-35-08_使用次数：{new}.mp4")));
        assert_eq!(
            fs::read(path).unwrap(),
            b"scanner fixture, not a playable video"
        );
    }
}
#[test]
fn duplicate_request_and_stale_count_cannot_repeat_or_overwrite() {
    let (_directory, runtime, root) = fixture();
    let op = prepare(&runtime, &root, "素材.mp4", "2026-09-28_14-35-08");
    complete(&runtime, &op);
    let request = uuid::Uuid::new_v4().to_string();
    count::set(&runtime, &op.video_id, 0, 3, &request).unwrap();
    count::set(&runtime, &op.video_id, 0, 3, &request).unwrap();
    assert!(set(&runtime, &op.video_id, 0, 4).is_err());
    assert_eq!(video_state(&runtime, &op.video_id).1, 3);
    let n: u32 = with_database(&runtime.database, |c| {
        c.query_row(
            "SELECT count(*) FROM rename_operations WHERE kind='set_count'",
            [],
            |r| r.get(0),
        )
        .map_err(crate::domain::error::AppError::database)
    })
    .unwrap();
    assert_eq!(n, 1);
}
#[test]
fn target_conflict_preserves_both_files_and_old_count() {
    let (_directory, runtime, root) = fixture();
    let op = prepare(&runtime, &root, "素材.mp4", "2026-09-28_14-35-08");
    complete(&runtime, &op);
    let target = root.join("2026-09-28_14-35-08_使用次数：1.mp4");
    fs::write(&target, b"do not overwrite").unwrap();
    assert!(set(&runtime, &op.video_id, 0, 1).is_err());
    assert_eq!(fs::read(target).unwrap(), b"do not overwrite");
    assert!(op.new_path.exists());
    assert_eq!(video_state(&runtime, &op.video_id).1, 0);
}
#[test]
fn missing_source_and_replaced_content_do_not_update_database() {
    let (_directory, runtime, root) = fixture();
    let op = prepare(&runtime, &root, "素材.mp4", "2026-09-28_14-35-08");
    complete(&runtime, &op);
    fs::remove_file(&op.new_path).unwrap();
    assert!(set(&runtime, &op.video_id, 0, 1).is_err());
    assert_eq!(video_state(&runtime, &op.video_id).1, 0);
    fs::write(&op.new_path, b"replaced file").unwrap();
    assert!(set(&runtime, &op.video_id, 0, 1).is_err());
    assert_eq!(fs::read(&op.new_path).unwrap(), b"replaced file");
    assert_eq!(video_state(&runtime, &op.video_id).1, 0);
}
#[test]
fn failed_database_commit_rolls_back_filename_and_count() {
    let (_directory, runtime, root) = fixture();
    let op = prepare(&runtime, &root, "素材.mp4", "2026-09-28_14-35-08");
    complete(&runtime, &op);
    with_database(&runtime.database,|c|c.execute_batch("CREATE TRIGGER fail_commit BEFORE UPDATE OF current_path ON videos BEGIN SELECT RAISE(ABORT,'simulated database failure'); END;").map_err(crate::domain::error::AppError::database)).unwrap();
    assert!(set(&runtime, &op.video_id, 0, 7).is_err());
    assert!(op.new_path.exists());
    assert_eq!(video_state(&runtime, &op.video_id).1, 0);
    let state: String = with_database(&runtime.database, |c| {
        c.query_row(
            "SELECT status FROM rename_operations WHERE kind='set_count'",
            [],
            |r| r.get(0),
        )
        .map_err(crate::domain::error::AppError::database)
    })
    .unwrap();
    assert_eq!(state, "rolled_back");
}
#[test]
fn journal_failure_leaves_source_untouched() {
    let (_directory, runtime, root) = fixture();
    let op = prepare(&runtime, &root, "素材.mp4", "2026-09-28_14-35-08");
    complete(&runtime, &op);
    with_database(&runtime.database,|c|c.execute_batch("CREATE TRIGGER fail_journal BEFORE INSERT ON rename_operations BEGIN SELECT RAISE(ABORT,'simulated journal failure'); END;").map_err(crate::domain::error::AppError::database)).unwrap();
    assert!(set(&runtime, &op.video_id, 0, 1).is_err());
    assert!(op.new_path.exists());
    assert_eq!(video_state(&runtime, &op.video_id).1, 0);
}
#[test]
fn startup_recovers_interruption_before_and_after_rename() {
    let (_directory, runtime, root) = fixture();
    let before = prepare(&runtime, &root, "未改名.mp4", "2026-09-28_14-35-08");
    rename::recover(&runtime).unwrap();
    assert!(before.old_path.exists());
    assert_eq!(video_state(&runtime, &before.video_id).2, "error");
    let after = prepare(&runtime, &root, "已改名.mp4", "2026-09-28_14-35-09");
    let mut file = LockedSource::open(
        &after.old_path,
        &root,
        &after.fingerprint,
        Some(&after.identity),
        after.size,
    )
    .unwrap();
    file.rename_to(&after.new_path).unwrap();
    drop(file);
    {
        let mut database = runtime.database.lock().unwrap();
        *database = None;
        *database = Some(Database::open(&_directory.path().join("database.sqlite")).unwrap());
    }
    rename::recover(&runtime).unwrap();
    assert_eq!(
        video_state(&runtime, &after.video_id).0,
        source::path_text(&after.new_path).unwrap()
    );
    assert_eq!(video_state(&runtime, &after.video_id).2, "ready");
    rename::recover(&runtime).unwrap();
}
#[test]
fn ambiguous_recovery_keeps_both_paths_for_review() {
    let (_directory, runtime, root) = fixture();
    let op = prepare(&runtime, &root, "原片.mp4", "2026-09-28_14-35-08");
    fs::write(&op.new_path, b"different target").unwrap();
    assert!(rename::recover(&runtime).is_err());
    assert!(op.old_path.exists());
    assert_eq!(fs::read(&op.new_path).unwrap(), b"different target");
    assert_eq!(video_state(&runtime, &op.video_id).2, "needs_review");
}
#[test]
fn complete_scan_is_automatic_and_does_not_reingest_renamed_files() {
    let (_directory, runtime, root) = fixture();
    candidate(&root, "新片.mp4");
    scan::scan(&runtime).unwrap();
    let first = library::status(&runtime).unwrap();
    assert_eq!(first.total, 1);
    assert_eq!(first.candidates.len(), 0);
    scan::scan(&runtime).unwrap();
    assert_eq!(library::status(&runtime).unwrap().total, 1);
    // 已完成日志中的原路径可被新视频再次使用。
    candidate(&root, "新片.mp4");
    scan::scan(&runtime).unwrap();
    assert_eq!(library::status(&runtime).unwrap().total, 2);
}
#[cfg(windows)]
#[test]
fn occupied_video_cannot_be_renamed() {
    use std::os::windows::fs::OpenOptionsExt;
    let (_directory, runtime, root) = fixture();
    let op = prepare(&runtime, &root, "素材.mp4", "2026-09-28_14-35-08");
    complete(&runtime, &op);
    let occupied = fs::OpenOptions::new()
        .read(true)
        .share_mode(1)
        .open(&op.new_path)
        .unwrap();
    assert!(set(&runtime, &op.video_id, 0, 1).is_err());
    assert_eq!(video_state(&runtime, &op.video_id).1, 0);
    drop(occupied);
    set(&runtime, &op.video_id, 0, 1).unwrap();
}
#[test]
fn busy_backend_rejects_overlapping_count_submission() {
    let (_directory, runtime, root) = fixture();
    let op = prepare(&runtime, &root, "素材.mp4", "2026-09-28_14-35-08");
    complete(&runtime, &op);
    let _guard = library::acquire(&runtime).unwrap();
    assert_eq!(set(&runtime, &op.video_id, 0, 1).unwrap_err().code, "BUSY");
}
#[test]
fn pagination_can_select_videos_beyond_first_hundred() {
    let (_directory, runtime, root) = fixture();
    let op = prepare(&runtime, &root, "素材.mp4", "2026-09-28_14-35-08");
    complete(&runtime, &op);
    with_database(&runtime.database,|c|{for n in 1..=100 {c.execute("INSERT INTO videos(id,library_id,current_path,filename,original_filename,extension,ingest_date,daily_sequence,use_count,file_size,created_at,updated_at,status,naming_base) SELECT ?1,library_id,?2,?1,?1,extension,ingest_date,?3,0,1,created_at,updated_at,'ready',?4 FROM videos WHERE id=?5",params![format!("test-{n}"),source::path_text(&root.join(format!("test-{n}.mp4")))?,n+1,format!("2026-09-28_14-35-08_{n:03}"),op.video_id]).map_err(crate::domain::error::AppError::database)?;}Ok(())}).unwrap();
    assert_eq!(library::status_page(&runtime, 0).unwrap().videos.len(), 100);
    assert_eq!(library::status_page(&runtime, 1).unwrap().videos.len(), 1);
}

#[cfg(windows)]
#[test]
fn automatic_scan_retries_after_copy_handle_closes() {
    use std::os::windows::fs::OpenOptionsExt;
    let (_directory, runtime, root) = fixture();
    let incoming = candidate(&root, "复制途中.mp4");
    let writer = fs::OpenOptions::new()
        .write(true)
        .share_mode(7)
        .open(&incoming.path)
        .unwrap();
    assert!(scan::scan(&runtime).is_err());
    assert!(incoming.path.exists());
    assert_eq!(
        library::status(&runtime).unwrap().candidates[0].status,
        "waiting"
    );
    drop(writer);
    scan::scan(&runtime).unwrap();
    assert!(!incoming.path.exists());
    assert_eq!(library::status(&runtime).unwrap().ready, 1);
}
