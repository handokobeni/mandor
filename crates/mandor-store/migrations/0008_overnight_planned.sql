-- Kartu Planned dan sumber desain di antrean VM (plan 2026-10-30-001, KTD-2).
-- Baris lama: tanpa desain dan tanpa plan (No plan).
ALTER TABLE overnight_queue ADD COLUMN design_json TEXT;
ALTER TABLE overnight_queue ADD COLUMN planned_json TEXT;
ALTER TABLE overnight_queue ADD COLUMN planned_state TEXT;
ALTER TABLE overnight_queue ADD COLUMN origin_daemon TEXT;
ALTER TABLE overnight_queue ADD COLUMN planned_reason TEXT;
