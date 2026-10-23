-- Rilis 2: buku AC per run, riwayat review, sumber desain, setelan global, dan
-- setelan workspace (KTD-25, KTD-33, KTD-37).

CREATE TABLE ac_rows (
    run_id       TEXT NOT NULL REFERENCES runs(id) ON DELETE CASCADE,
    ac_id        TEXT NOT NULL,
    position     INTEGER NOT NULL,
    label        TEXT,
    text         TEXT NOT NULL,
    design       TEXT NOT NULL,
    design_note  TEXT,
    verdict      TEXT NOT NULL,
    evidence     TEXT,
    stale        INTEGER NOT NULL DEFAULT 0,
    status       TEXT NOT NULL,
    reason       TEXT,
    decided_by   TEXT,
    PRIMARY KEY (run_id, ac_id)
);

CREATE TABLE reviews (
    id              TEXT PRIMARY KEY NOT NULL,
    run_id          TEXT NOT NULL REFERENCES runs(id) ON DELETE CASCADE,
    trigger         TEXT NOT NULL,
    status          TEXT NOT NULL,
    browser_used    INTEGER NOT NULL DEFAULT 0,
    error           TEXT,
    cost_usd        REAL,
    usage_json      TEXT NOT NULL DEFAULT '{}',
    started_at_ms   INTEGER NOT NULL,
    finished_at_ms  INTEGER
);
CREATE INDEX reviews_by_run ON reviews (run_id, started_at_ms);

CREATE TABLE review_verdicts (
    review_id  TEXT NOT NULL REFERENCES reviews(id) ON DELETE CASCADE,
    ac_id      TEXT NOT NULL,
    label      TEXT,
    verdict    TEXT NOT NULL,
    evidence   TEXT,
    PRIMARY KEY (review_id, ac_id)
);

CREATE TABLE design_sources (
    id             TEXT PRIMARY KEY NOT NULL,
    name           TEXT NOT NULL UNIQUE,
    kind           TEXT NOT NULL,
    config_json    TEXT NOT NULL DEFAULT '{}',
    created_at_ms  INTEGER NOT NULL
);

CREATE TABLE settings (
    key    TEXT PRIMARY KEY NOT NULL,
    value  TEXT NOT NULL
);

ALTER TABLE workspaces ADD COLUMN settings_json TEXT NOT NULL DEFAULT '{}';
