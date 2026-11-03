// Penutupan run kartu yang sudah selesai (plan 2026-11-03-001, R4, R5, R9).

/** Label tombol di kolom Done dan Settings; `null` = tombol disembunyikan. */
export function closeLabel(count: number): string | null {
  if (count <= 0) return null;
  return count === 1 ? 'Close 1 finished run' : `Close ${count} finished runs`;
}

/** Ukuran desimal satu angka di belakang koma; `null` = gagal diukur. */
export function formatBytes(bytes: number | null): string {
  if (bytes === null) return 'size unknown';
  const units = ['B', 'KB', 'MB', 'GB'];
  let value = bytes;
  let unit = 0;
  while (value >= 1000 && unit < units.length - 1) {
    value /= 1000;
    unit += 1;
  }
  if (unit === 0) return `${bytes} B`;
  const text = value >= 100 ? Math.round(value).toString() : value.toFixed(1);
  return `${text} ${units[unit]}`;
}

/** Jumlah ukuran; ukuran yang tidak terbaca diabaikan. */
export function totalBytes(rows: { bytes: number | null }[]): number {
  return rows.reduce((sum, r) => sum + (r.bytes ?? 0), 0);
}

const REASONS: Record<string, string> = {
  unpushed_changes: 'Unpushed changes',
  working: 'Agent is working'
};

/** Alasan run selesai tidak ditutup. */
export function reasonLabel(reason: string): string {
  return REASONS[reason] ?? reason;
}

/** Tombol konfirmasi dialog. */
export function confirmLabel(count: number, bytes: number): string {
  const runs = count === 1 ? '1 run' : `${count} runs`;
  return bytes > 0 ? `Close ${runs} · free ${formatBytes(bytes)}` : `Close ${runs}`;
}

/** Kunci run di daftar: run grup diringkas menjadi kunci pertama plus jumlah kartu lain. */
export function keyLabel(keys: string[]): string {
  if (keys.length <= 1) return keys[0] ?? '';
  return `${keys[0]} +${keys.length - 1}`;
}
