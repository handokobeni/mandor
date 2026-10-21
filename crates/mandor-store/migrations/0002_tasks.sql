-- Cache task Lark per tabel (scope), bukan per repo: task milik pengguna
-- belum tentu terhubung ke repo. Penanda kartu (R6, R6a) disimpan terpisah
-- dari data agar upsert dari Lark tidak menghapusnya.
DROP TABLE records;

CREATE TABLE tasks (
    scope             TEXT NOT NULL,
    record_id         TEXT NOT NULL,
    data_json         TEXT NOT NULL,
    last_modified_ms  INTEGER NOT NULL,
    flag_kind         TEXT,
    flag_detail       TEXT,
    PRIMARY KEY (scope, record_id)
);
