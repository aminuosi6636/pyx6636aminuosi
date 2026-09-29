// 使用 Node 开发工具的原生 SQLite 验证同一份 SQL；产品运行时仅使用 Rust SQLite。
import { DatabaseSync } from 'node:sqlite';
import { readFileSync, mkdtempSync, mkdirSync, rmSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import path from 'node:path';
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { randomUUID } from 'node:crypto';

const schema = readFileSync(new URL('../src-tauri/migrations/0001_initial.sql', import.meta.url), 'utf8');
const time = '2026-09-27T12:00:00Z';
function fixture() {
  const db = new DatabaseSync(':memory:');
  db.exec('PRAGMA foreign_keys=ON;');
  db.exec(schema);
  const library = randomUUID();
  db.prepare('INSERT INTO libraries VALUES (?,?,?,?,?)').run(library, '/素材库/测试 (中文)', null, time, time);
  return { db, library };
}
function video(db, library, overrides = {}) {
  const data = { id: randomUUID(), path: `/素材库/${randomUUID()}.mp4`, sequence: 1, count: 0, date: '2026-09-27', fingerprint: 'same-content', ...overrides };
  db.prepare(`INSERT INTO videos(id,library_id,current_path,filename,original_filename,extension,ingest_date,daily_sequence,use_count,file_size,created_at,updated_at,fingerprint,status)
    VALUES(?,?,?,?,?,'mp4',?,?,?,?,?,?,?,'ready')`)
    .run(data.id, library, data.path, 'test.mp4', 'original.mp4', data.date, data.sequence, data.count, 100, time, time, data.fingerprint);
  return data.id;
}
function operation(db, videoId, overrides = {}) {
  const data = { id: randomUUID(), request: randomUUID(), oldCount: 0, newCount: 1, status: 'pending', completedAt: null, ...overrides };
  db.prepare(`INSERT INTO operation_journal(id,video_id,request_id,operation_type,old_path,new_path,old_count,new_count,fingerprint,status,created_at,completed_at)
    VALUES(?,?,?,'use','old.mp4','new.mp4',?,?,'hash',?,?,?)`)
    .run(data.id, videoId, data.request, data.oldCount, data.newCount, data.status, time, data.completedAt);
  return data.id;
}

test('schema contains all required tables and query indexes', () => {
  const { db } = fixture();
  try {
    const tables = db.prepare("SELECT name FROM sqlite_master WHERE type='table'").all().map(x => x.name);
    for (const name of ['libraries','videos','use_events','operation_journal','settings','daily_sequences']) assert.ok(tables.includes(name));
    const plan = db.prepare("EXPLAIN QUERY PLAN SELECT id FROM videos WHERE library_id=? ORDER BY ingest_date DESC,daily_sequence DESC,id LIMIT 100").all('library');
    assert.ok(plan.some(row => row.detail.includes('idx_videos_latest')));
  } finally { db.close(); }
});
test('same date sequence is unique while the next day can have sequence 1', () => {
  const { db, library } = fixture();
  try {
    video(db, library);
    assert.throws(() => video(db, library));
    video(db, library, { date: '2026-09-28' });
    assert.equal(db.prepare('SELECT count(*) AS n FROM videos').get().n, 2);
  } finally { db.close(); }
});
test('default ordering uses latest date and descending sequence', () => {
  const { db, library } = fixture();
  try {
    video(db, library, { sequence: 1 }); video(db, library, { sequence: 2 }); video(db, library, { sequence: 3 });
    video(db, library, { date: '2026-09-26', sequence: 9 });
    assert.deepEqual(db.prepare('SELECT daily_sequence FROM videos ORDER BY ingest_date DESC,daily_sequence DESC').all().map(r => r.daily_sequence), [3,2,1,9]);
  } finally { db.close(); }
});
test('identical file content does not merge independent videos', () => {
  const { db, library } = fixture();
  try {
    video(db, library); video(db, library, { sequence: 2 });
    assert.equal(db.prepare('SELECT count(*) AS n FROM videos WHERE fingerprint=?').get('same-content').n, 2);
  } finally { db.close(); }
});
test('negative counts and unsupported video status are rejected', () => {
  const { db, library } = fixture();
  try {
    assert.throws(() => video(db, library, { count: -1 }));
    const id = video(db, library);
    assert.throws(() => db.prepare('UPDATE videos SET status=? WHERE id=?').run('deleted', id));
  } finally { db.close(); }
});
test('one pending or needs_review operation per video prevents conflicting writes', () => {
  const { db, library } = fixture();
  try {
    const id = video(db, library); const op = operation(db, id);
    assert.throws(() => operation(db, id));
    db.prepare("UPDATE operation_journal SET status='needs_review' WHERE id=?").run(op);
    assert.throws(() => operation(db, id));
    db.prepare("UPDATE operation_journal SET status='rolled_back',completed_at=? WHERE id=?").run(time, op);
    operation(db, id);
  } finally { db.close(); }
});
test('idempotent request and event identifiers cannot be duplicated', () => {
  const { db, library } = fixture();
  try {
    const id = video(db, library); const request = randomUUID();
    const op = operation(db, id, { request, status: 'completed', completedAt: time });
    assert.throws(() => operation(db, id, { request }));
    db.prepare('INSERT INTO use_events VALUES (?,?,?,?,?,?)').run(randomUUID(), id, op, 0, 1, time);
    assert.throws(() => db.prepare('INSERT INTO use_events VALUES (?,?,?,?,?,?)').run(randomUUID(), id, op, 0, 1, time));
  } finally { db.close(); }
});
test('journal must increment exactly once and completed status must have a timestamp', () => {
  const { db, library } = fixture();
  try {
    const id = video(db, library);
    assert.throws(() => operation(db, id, { newCount: 4 }));
    assert.throws(() => operation(db, id, { status: 'completed' }));
  } finally { db.close(); }
});
test('foreign keys prevent deleting video history', () => {
  const { db, library } = fixture();
  try {
    const id = video(db, library); operation(db, id);
    assert.throws(() => db.prepare('DELETE FROM videos WHERE id=?').run(id));
    assert.throws(() => db.prepare('DELETE FROM libraries WHERE id=?').run(library));
  } finally { db.close(); }
});
test('database transaction rollback preserves old counts and paths', () => {
  const { db, library } = fixture();
  try {
    const id = video(db, library); const before = db.prepare('SELECT current_path,use_count FROM videos WHERE id=?').get(id);
    db.exec('BEGIN IMMEDIATE;');
    db.prepare("UPDATE videos SET use_count=1,current_path='new-path.mp4' WHERE id=?").run(id);
    assert.throws(() => operation(db, id, { newCount: 9 }));
    db.exec('ROLLBACK;');
    assert.deepEqual(db.prepare('SELECT current_path,use_count FROM videos WHERE id=?').get(id), before);
  } finally { db.close(); }
});
test('migration is transactional and cannot leave a partially applied schema', () => {
  const db = new DatabaseSync(':memory:');
  try {
    db.exec('BEGIN IMMEDIATE;');
    assert.throws(() => db.exec(`${schema}\nINVALID SQL;`));
    db.exec('ROLLBACK;');
    assert.equal(db.prepare("SELECT count(*) AS n FROM sqlite_master WHERE type='table'").get().n, 0);
  } finally { db.close(); }
});
test('Chinese and spaced database path persists correctly after reopening', () => {
  const scratch = fileURLToPath(new URL('../../../work/schema-tests/', import.meta.url));
  mkdirSync(scratch, { recursive: true });
  const directory = mkdtempSync(path.join(scratch, '芷兰素材 (测试)-'));
  const filename = path.join(directory, '本地数据库.sqlite');
  let db;
  try {
    db = new DatabaseSync(filename); db.exec(schema);
    db.prepare('INSERT INTO settings VALUES (?,?,?)').run('root_directory', '/中文路径/新素材 (1)', time);
    db.close(); db = new DatabaseSync(filename);
    assert.equal(db.prepare('SELECT value FROM settings WHERE key=?').get('root_directory').value, '/中文路径/新素材 (1)');
  } finally {
    db?.close();
    // 只清理本测试自己创建的临时目录；不触碰素材或应用数据。
    const resolved = path.resolve(directory);
    if (resolved.startsWith(`${path.resolve(scratch)}${path.sep}`)) rmSync(resolved, { recursive: true, force: true });
  }
});

// 当前简化需求的 migration 验证；旧测试保留用于旧 schema 历史。
const simpleMigration = readFileSync(new URL('../src-tauri/migrations/0003_simple_renamer.sql', import.meta.url), 'utf8');
function currentFixture() {
  const result = fixture();
  result.db.exec(readFileSync(new URL('../src-tauri/migrations/0002_candidates.sql', import.meta.url), 'utf8'));
  result.db.exec(simpleMigration);
  return result;
}
test('current journal accepts absolute counts including decreases', () => {
  const {db,library}=currentFixture();
  try {
    const id=video(db,library);
    for(const [oldCount,newCount] of [[0,5],[5,2],[2,0]]) {
      db.prepare("INSERT INTO rename_operations(id,video_id,request_id,kind,old_path,new_path,old_count,new_count,fingerprint,file_identity,status,created_at,completed_at) VALUES(?,?,?,'set_count','old','new',?,?,'hash','identity','completed',?,?)")
        .run(randomUUID(),id,randomUUID(),oldCount,newCount,time,time);
    }
    assert.equal(db.prepare('SELECT count(*) AS n FROM rename_operations').get().n,3);
  } finally {db.close();}
});
test('current journal rejects overlapping unfinished operations', () => {
  const {db,library}=currentFixture();
  try {
    const id=video(db,library);
    const insert=db.prepare("INSERT INTO rename_operations(id,video_id,request_id,kind,old_path,new_path,old_count,new_count,fingerprint,file_identity,status,created_at) VALUES(?,?,?,'set_count','old','new',0,5,'hash','identity','pending',?)");
    insert.run(randomUUID(),id,randomUUID(),time);
    assert.throws(()=>insert.run(randomUUID(),id,randomUUID(),time));
  } finally {db.close();}
});
test('timestamp naming base is unique and does not depend on usage count', () => {
  const {db,library}=currentFixture();
  try {
    const first=video(db,library);const second=video(db,library,{sequence:2});
    db.prepare('UPDATE videos SET naming_base=? WHERE id=?').run('2026-09-28_14-35-08',first);
    assert.throws(()=>db.prepare('UPDATE videos SET naming_base=? WHERE id=?').run('2026-09-28_14-35-08',second));
    db.prepare('UPDATE videos SET use_count=5 WHERE id=?').run(first);
    assert.equal(db.prepare('SELECT naming_base FROM videos WHERE id=?').get(first).naming_base,'2026-09-28_14-35-08');
  } finally {db.close();}
});
