-- Skema awal mandord (Rilis 1). Semua waktu dalam milidetik epoch UTC.

CREATE TABLE repos (
    id              TEXT PRIMARY KEY NOT NULL,
    path            TEXT NOT NULL UNIQUE,
    gh_profile      TEXT,
    default_branch  TEXT,
    settings_json   TEXT NOT NULL DEFAULT '{}',
    created_at_ms   INTEGER NOT NULL
);

-- Cache record Lark milik pengguna; sumber kebenaran tetap Lark.
CREATE TABLE records (
    repo_id               TEXT NOT NULL REFERENCES repos(id) ON DELETE CASCADE,
    record_id             TEXT NOT NULL,
    data_json             TEXT NOT NULL,
    last_modified_ms      INTEGER NOT NULL,
    missing               INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (repo_id, record_id)
);

-- Satu task run per record (laptop pemilik). `generation` naik setiap spawn agent.
CREATE TABLE runs (
    id              TEXT PRIMARY KEY NOT NULL,
    repo_id         TEXT NOT NULL REFERENCES repos(id) ON DELETE CASCADE,
    record_id       TEXT NOT NULL,
    state           TEXT NOT NULL,
    state_json      TEXT NOT NULL DEFAULT '{}',
    generation      INTEGER NOT NULL DEFAULT 0,
    branch          TEXT,
    worktree_path   TEXT,
    session_id      TEXT,
    pid             INTEGER,
    pr_url          TEXT,
    closed          INTEGER NOT NULL DEFAULT 0,
    created_at_ms   INTEGER NOT NULL,
    updated_at_ms   INTEGER NOT NULL
);
CREATE UNIQUE INDEX runs_active_per_record ON runs(repo_id, record_id) WHERE closed = 0;

CREATE TABLE messages (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    run_id          TEXT NOT NULL REFERENCES runs(id) ON DELETE CASCADE,
    role            TEXT NOT NULL,
    body_json       TEXT NOT NULL,
    created_at_ms   INTEGER NOT NULL
);
CREATE INDEX messages_by_run ON messages(run_id, id);

-- Outbox tulis Lark: satu antrean per tabel, divalidasi ulang sebelum dikirim (KTD-7).
CREATE TABLE outbox (
    id               INTEGER PRIMARY KEY AUTOINCREMENT,
    queue            TEXT NOT NULL,
    record_id        TEXT NOT NULL,
    payload_json     TEXT NOT NULL,
    status           TEXT NOT NULL DEFAULT 'pending',
    attempts         INTEGER NOT NULL DEFAULT 0,
    next_attempt_ms  INTEGER NOT NULL,
    last_error       TEXT,
    created_at_ms    INTEGER NOT NULL
);
CREATE INDEX outbox_due ON outbox(queue, status, next_attempt_ms, id);

-- Jurnal komentar dua tahap (KTD-12): pending → delivered.
CREATE TABLE comment_journal (
    record_id       TEXT NOT NULL,
    reply_id        TEXT NOT NULL,
    run_id          TEXT NOT NULL,
    status          TEXT NOT NULL,
    created_at_ms   INTEGER NOT NULL,
    PRIMARY KEY (record_id, reply_id)
);

-- Notifikasi yang belum terkirim ke client mana pun.
CREATE TABLE notifications (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    kind            TEXT NOT NULL,
    payload_json    TEXT NOT NULL,
    delivered       INTEGER NOT NULL DEFAULT 0,
    created_at_ms   INTEGER NOT NULL
);
