-- Kartu persetujuan izin agent (plan 2026-11-06-001, KTD-A11). `input_json`
-- mentah hanya selama `pending`; transisi keluar menimpanya dengan versi
-- tersamarkan dalam transaksi yang sama.
CREATE TABLE approvals (
    id TEXT PRIMARY KEY NOT NULL,
    run_id TEXT NOT NULL,
    tool_use_id TEXT NOT NULL,
    turn TEXT NOT NULL,
    tool TEXT NOT NULL,
    input_json TEXT NOT NULL,
    cwd TEXT NOT NULL,
    status TEXT NOT NULL,
    answer TEXT,
    created_ms INTEGER NOT NULL,
    deadline_ms INTEGER NOT NULL,
    closed_ms INTEGER
);
CREATE INDEX approvals_run ON approvals(run_id, created_ms);
CREATE INDEX approvals_pending ON approvals(status) WHERE status = 'pending';

-- Izin dan penolakan run untuk permintaan persis sama (R7, R23); hanya hash
-- kunci yang disimpan.
CREATE TABLE approval_decisions (
    run_id TEXT NOT NULL,
    key_hash TEXT NOT NULL,
    turn TEXT,
    kind TEXT NOT NULL,
    created_ms INTEGER NOT NULL,
    PRIMARY KEY (run_id, key_hash)
);

-- Mode tolak langsung setelah kartu expired (R19).
CREATE TABLE approval_deny_mode (
    run_id TEXT PRIMARY KEY NOT NULL,
    since_ms INTEGER NOT NULL
);
