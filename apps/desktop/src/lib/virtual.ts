// Perhitungan posisi daftar virtual (chat). Tinggi 0 berarti baris belum
// benar-benar terlihat (mis. panel tab yang tersembunyi), jadi perkiraan
// tinggi tetap dipakai; bila tidak, semua baris dianggap terlewati dan daftar
// tidak pernah merender baris lagi.

/** Offset kumulatif tiap baris; elemen terakhir adalah tinggi total. */
export function offsetsOf(ids: number[], heights: Map<number, number>, estimate: number): number[] {
  const out = new Array<number>(ids.length + 1);
  out[0] = 0;
  for (let i = 0; i < ids.length; i++) {
    const h = heights.get(ids[i]);
    out[i + 1] = out[i] + (h && h > 0 ? h : estimate);
  }
  return out;
}

/** Indeks baris yang memuat posisi `y`. */
function indexAt(offsets: number[], y: number): number {
  let lo = 0;
  let hi = offsets.length - 1;
  while (lo < hi) {
    const mid = (lo + hi) >> 1;
    if (offsets[mid + 1] <= y) lo = mid + 1;
    else hi = mid;
  }
  return lo;
}

/** Rentang baris `[start, end)` yang dirender untuk posisi gulir dan tinggi area ini. */
export function visibleRange(
  offsets: number[],
  scrollTop: number,
  viewport: number,
  overscan: number
): { start: number; end: number } {
  const count = offsets.length - 1;
  return {
    start: indexAt(offsets, Math.max(0, scrollTop - overscan)),
    end: Math.min(count, indexAt(offsets, scrollTop + viewport + overscan) + 1)
  };
}
