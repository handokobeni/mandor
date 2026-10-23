-- Run grup: kartu-kartu satu flow di satu run (plan run grup, KTD-G1/G2).
CREATE TABLE run_cards (
    run_id TEXT NOT NULL REFERENCES runs(id),
    record_id TEXT NOT NULL,
    position INTEGER NOT NULL,
    state TEXT NOT NULL,
    data_json TEXT NOT NULL DEFAULT '{}',
    -- 1 selama kartu masih bagian run terbuka.
    open INTEGER NOT NULL DEFAULT 1,
    PRIMARY KEY (run_id, record_id)
);

-- Satu kartu hanya di satu run terbuka.
CREATE UNIQUE INDEX run_cards_open_record ON run_cards(record_id) WHERE open = 1;

-- Label kartu pada baris buku AC run grup.
ALTER TABLE ac_rows ADD COLUMN card TEXT;
