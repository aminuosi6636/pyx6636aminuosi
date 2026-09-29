-- 新需求：时间精确到秒，使用次数由用户直接填写，允许增加或调低。
-- 保留旧版表与历史，不重写旧 migration。
ALTER TABLE videos ADD COLUMN naming_base TEXT;
CREATE UNIQUE INDEX idx_video_naming_base ON videos(library_id,naming_base);
CREATE TABLE rename_operations (
    id TEXT PRIMARY KEY NOT NULL,
    video_id TEXT NOT NULL REFERENCES videos(id) ON DELETE RESTRICT,
    request_id TEXT NOT NULL UNIQUE,
    kind TEXT NOT NULL CHECK(kind IN ('ingest','set_count')),
    old_path TEXT NOT NULL,
    new_path TEXT NOT NULL,
    old_count INTEGER NOT NULL CHECK(old_count>=0),
    new_count INTEGER NOT NULL CHECK(new_count>=0),
    fingerprint TEXT NOT NULL,
    file_identity TEXT NOT NULL,
    status TEXT NOT NULL CHECK(status IN ('pending','completed','rolled_back','needs_review')),
    created_at TEXT NOT NULL,
    completed_at TEXT,
    error_message TEXT
) STRICT;
CREATE UNIQUE INDEX idx_active_rename ON rename_operations(video_id) WHERE status IN ('pending','needs_review');
CREATE INDEX idx_rename_recovery ON rename_operations(status,created_at);
