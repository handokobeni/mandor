-- Base asal run (`app_token/table_id`). Base copy berbagi record_id dengan
-- Base aslinya, jadi run hanya dibaca dan dicocokkan di Base asalnya.
-- Baris lama (NULL) diadopsi Base yang aktif saat sesi Lark berikutnya mulai.
ALTER TABLE runs ADD COLUMN scope TEXT;
DROP INDEX runs_active_per_record;
CREATE UNIQUE INDEX runs_active_per_record
    ON runs(COALESCE(scope, ''), repo_id, record_id) WHERE closed = 0;
