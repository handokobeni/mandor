-- Review lab (plan 2026-11-10-001, U2): kebutuhan lab, bukti terstruktur, dan
-- rekomendasi tampilan per baris AC; perlengkapan lab per repo; temuan di luar
-- AC per review.
ALTER TABLE ac_rows ADD COLUMN lab_need TEXT;
ALTER TABLE ac_rows ADD COLUMN evidence_json TEXT;
ALTER TABLE ac_rows ADD COLUMN look_json TEXT;

ALTER TABLE review_verdicts ADD COLUMN lab_need TEXT;
ALTER TABLE review_verdicts ADD COLUMN evidence_json TEXT;

-- Perlengkapan tanpa password (KTD-L2, KTD-L3). Persetujuan dikosongkan setiap
-- kali perlengkapan berubah.
ALTER TABLE repos ADD COLUMN lab_json TEXT;
ALTER TABLE repos ADD COLUMN lab_approved_digest TEXT;
ALTER TABLE repos ADD COLUMN lab_approved_at_ms INTEGER;

CREATE TABLE review_findings (
    id             INTEGER PRIMARY KEY AUTOINCREMENT,
    review_id      TEXT NOT NULL REFERENCES reviews(id) ON DELETE CASCADE,
    card           TEXT,
    title          TEXT NOT NULL,
    detail         TEXT NOT NULL,
    evidence_json  TEXT,
    created_at_ms  INTEGER NOT NULL
);
CREATE INDEX review_findings_by_review ON review_findings (review_id, id);
