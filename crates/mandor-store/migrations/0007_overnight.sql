-- Antrean, siklus, dan setelan lembur VM (KTD-57 plan VM).
CREATE TABLE overnight_queue (
    record_id    TEXT PRIMARY KEY NOT NULL,
    position     INTEGER NOT NULL,
    target_json  TEXT NOT NULL,
    note         TEXT,
    start_now    INTEGER NOT NULL DEFAULT 0,
    added_at_ms  INTEGER NOT NULL
);

-- Kartu yang keluar dari antrean tanpa dikerjakan, untuk laporan pagi (R9).
CREATE TABLE overnight_dequeued (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    record_id  TEXT NOT NULL,
    reason     TEXT NOT NULL,
    at_ms      INTEGER NOT NULL
);
CREATE INDEX overnight_dequeued_at ON overnight_dequeued(at_ms);

-- Kartu yang diserahkan ke laptop dan tidak boleh diantre lagi (KTD-63).
CREATE TABLE overnight_laptop_only (
    record_id  TEXT PRIMARY KEY NOT NULL,
    at_ms      INTEGER NOT NULL
);

-- Pemakaian per siklus lembur; cache dicatat terpisah (KTD-59).
CREATE TABLE overnight_cycles (
    cycle_start_ms      INTEGER PRIMARY KEY NOT NULL,
    input_tokens        INTEGER NOT NULL DEFAULT 0,
    output_tokens       INTEGER NOT NULL DEFAULT 0,
    cache_read_tokens   INTEGER NOT NULL DEFAULT 0,
    cache_write_tokens  INTEGER NOT NULL DEFAULT 0,
    report_sent_at_ms   INTEGER
);
