CREATE TABLE libraries (
    id TEXT PRIMARY KEY NOT NULL,
    root_path TEXT NOT NULL UNIQUE,
    root_identity TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
) STRICT;

CREATE TABLE videos (
    id TEXT PRIMARY KEY NOT NULL,
    library_id TEXT NOT NULL REFERENCES libraries(id) ON DELETE RESTRICT,
    current_path TEXT NOT NULL UNIQUE,
    filename TEXT NOT NULL,
    original_filename TEXT NOT NULL,
    extension TEXT NOT NULL CHECK (lower(extension) IN ('mp4','mov','m4v','avi','mkv')),
    ingest_date TEXT NOT NULL CHECK (length(ingest_date) = 10 AND ingest_date GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]'),
    daily_sequence INTEGER NOT NULL CHECK (daily_sequence > 0),
    use_count INTEGER NOT NULL DEFAULT 0 CHECK (use_count >= 0),
    file_size INTEGER NOT NULL CHECK (file_size >= 0),
    duration_ms INTEGER CHECK (duration_ms IS NULL OR duration_ms >= 0),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    fingerprint TEXT,
    file_identity TEXT,
    modified_at_ns TEXT,
    status TEXT NOT NULL CHECK (status IN ('ingesting','ready','missing','needs_review','error')),
    thumbnail_status TEXT NOT NULL DEFAULT 'pending' CHECK (thumbnail_status IN ('pending','ready','unsupported','error')),
    last_seen_scan_id TEXT,
    UNIQUE (library_id, ingest_date, daily_sequence)
) STRICT;

CREATE TABLE operation_journal (
    id TEXT PRIMARY KEY NOT NULL,
    video_id TEXT NOT NULL REFERENCES videos(id) ON DELETE RESTRICT,
    request_id TEXT NOT NULL UNIQUE,
    operation_type TEXT NOT NULL CHECK (operation_type IN ('ingest','use')),
    old_path TEXT NOT NULL,
    new_path TEXT NOT NULL,
    old_count INTEGER NOT NULL CHECK (old_count >= 0),
    new_count INTEGER NOT NULL CHECK (new_count >= 0),
    fingerprint TEXT NOT NULL,
    file_identity TEXT,
    status TEXT NOT NULL DEFAULT 'pending' CHECK (status IN ('pending','completed','rolled_back','needs_review')),
    created_at TEXT NOT NULL,
    completed_at TEXT,
    error_message TEXT,
    CHECK ((operation_type = 'ingest' AND old_count = 0 AND new_count = 0)
        OR (operation_type = 'use' AND new_count = old_count + 1)),
    CHECK ((status IN ('pending','needs_review') AND completed_at IS NULL)
        OR (status IN ('completed','rolled_back') AND completed_at IS NOT NULL))
) STRICT;

CREATE TABLE use_events (
    id TEXT PRIMARY KEY NOT NULL,
    video_id TEXT NOT NULL REFERENCES videos(id) ON DELETE RESTRICT,
    operation_id TEXT NOT NULL UNIQUE REFERENCES operation_journal(id) ON DELETE RESTRICT,
    previous_count INTEGER NOT NULL CHECK (previous_count >= 0),
    new_count INTEGER NOT NULL CHECK (new_count = previous_count + 1),
    used_at TEXT NOT NULL
) STRICT;

CREATE TABLE settings (
    key TEXT PRIMARY KEY NOT NULL,
    value TEXT NOT NULL,
    updated_at TEXT NOT NULL
) STRICT;

CREATE TABLE daily_sequences (
    library_id TEXT NOT NULL REFERENCES libraries(id) ON DELETE RESTRICT,
    ingest_date TEXT NOT NULL,
    next_sequence INTEGER NOT NULL CHECK (next_sequence > 0),
    PRIMARY KEY (library_id, ingest_date)
) STRICT;

CREATE INDEX idx_videos_latest ON videos(library_id, ingest_date DESC, daily_sequence DESC, id);
CREATE INDEX idx_videos_usage ON videos(library_id, use_count, ingest_date DESC, daily_sequence DESC, id);
CREATE INDEX idx_videos_sequence ON videos(library_id, daily_sequence, ingest_date, id);
CREATE INDEX idx_videos_fingerprint ON videos(library_id, fingerprint, file_size);
CREATE INDEX idx_videos_identity ON videos(library_id, file_identity);
CREATE INDEX idx_videos_status ON videos(library_id, status);
CREATE INDEX idx_journal_recovery ON operation_journal(status, created_at);
CREATE UNIQUE INDEX idx_one_active_operation ON operation_journal(video_id)
    WHERE status IN ('pending','needs_review');
CREATE INDEX idx_events_video_time ON use_events(video_id, used_at DESC);
