-- Kartu pertanyaan agent (plan 2026-11-07-001, KTD-Q1, KTD-Q3). Satu
-- pertanyaan per generasi giliran run; transisi keluar `pending` atomik.
CREATE TABLE questions (
    id TEXT PRIMARY KEY NOT NULL,
    run_id TEXT NOT NULL,
    generation INTEGER NOT NULL,
    question TEXT NOT NULL,
    options_json TEXT NOT NULL,
    status TEXT NOT NULL,
    answer TEXT,
    created_ms INTEGER NOT NULL,
    closed_ms INTEGER
);
CREATE UNIQUE INDEX questions_turn ON questions(run_id, generation);
CREATE INDEX questions_run ON questions(run_id, created_ms);
CREATE INDEX questions_pending ON questions(run_id) WHERE status = 'pending';
