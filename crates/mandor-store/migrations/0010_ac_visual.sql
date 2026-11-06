-- Hasil perbandingan tampilan dengan prototype per baris AC (plan
-- 2026-11-03-002, KTD-P5). Baris lama: tidak dibandingkan.
ALTER TABLE ac_rows ADD COLUMN visual TEXT NOT NULL DEFAULT 'not_compared';
ALTER TABLE ac_rows ADD COLUMN visual_json TEXT;
