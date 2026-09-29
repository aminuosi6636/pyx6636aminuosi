CREATE TABLE ingest_candidates (
    id TEXT PRIMARY KEY NOT NULL,
    library_id TEXT NOT NULL REFERENCES libraries(id) ON DELETE RESTRICT,
    source_path TEXT NOT NULL,
    original_filename TEXT NOT NULL,
    extension TEXT NOT NULL,
    file_size INTEGER NOT NULL CHECK(file_size >= 0),
    modified_at_ns TEXT NOT NULL,
    fingerprint TEXT,
    file_identity TEXT,
    status TEXT NOT NULL CHECK(status IN ('waiting','ready','needs_review','error')),
    error_message TEXT,
    discovered_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    UNIQUE(library_id, source_path)
) STRICT;
CREATE INDEX idx_candidates_library_status ON ingest_candidates(library_id,status,discovered_at,id);
