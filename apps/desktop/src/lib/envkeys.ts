/** Satu baris dialog Pick from .env (plan 2026-11-03-002, R1–R3). */
export interface EnvKeyRow {
  name: string;
  checked: boolean;
  /** Tersimpan di setelan tapi tidak ada di `.env` (R2). */
  missing: boolean;
}

/** Kunci `.env` sesuai urutan berkas, lalu kunci tersimpan yang tidak ada di berkas. */
export function pickerRows(fileKeys: string[], saved: string[]): EnvKeyRow[] {
  const rows: EnvKeyRow[] = fileKeys.map((name) => ({ name, checked: saved.includes(name), missing: false }));
  for (const name of saved) {
    if (!fileKeys.includes(name)) rows.push({ name, checked: true, missing: true });
  }
  return rows;
}

/** Kunci yang dicentang, dengan urutan baris. */
export function applyPicks(rows: EnvKeyRow[]): string[] {
  return rows.filter((r) => r.checked).map((r) => r.name);
}
