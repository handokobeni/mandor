-- Riwayat notifikasi untuk notification center: status baca per notifikasi.
ALTER TABLE notifications ADD COLUMN read INTEGER NOT NULL DEFAULT 0;
CREATE INDEX notifications_pending ON notifications(delivered) WHERE delivered = 0;
