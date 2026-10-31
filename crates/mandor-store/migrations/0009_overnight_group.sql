-- Grup kartu sebagai satu item antrean VM (plan 2026-10-23-004, KTD-G10).
-- Baris lama: kartu tunggal.
ALTER TABLE overnight_queue ADD COLUMN group_json TEXT;
