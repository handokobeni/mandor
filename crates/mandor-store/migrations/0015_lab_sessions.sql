-- Sesi lab data (plan 2026-11-10-001, U12): fase tahan restart, kunci
-- penyimpanan yang dipegang, salinan skrip yang dipin, dan sidik sebelum lab.
-- Transisi fase memakai `UPDATE ... WHERE phase = ?` sehingga hanya satu yang
-- menang bila bersamaan.
CREATE TABLE lab_sessions (
    id                  TEXT PRIMARY KEY,
    run_id              TEXT NOT NULL,
    repo_id             TEXT NOT NULL,
    lock_keys_json      TEXT NOT NULL,
    phase               TEXT NOT NULL,
    snapshot_dir        TEXT,
    scripts_digest      TEXT NOT NULL,
    fingerprints_json   TEXT,
    error               TEXT,
    created_at_ms       INTEGER NOT NULL,
    updated_at_ms       INTEGER NOT NULL
);
CREATE INDEX lab_sessions_by_phase ON lab_sessions (phase);
CREATE INDEX lab_sessions_by_repo ON lab_sessions (repo_id, phase);

-- Digest salinan skrip lab data yang disetujui (KTD-L18); dikosongkan bersama
-- persetujuan setiap kali perlengkapan berubah.
ALTER TABLE repos ADD COLUMN lab_scripts_digest TEXT;
