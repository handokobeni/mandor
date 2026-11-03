import { describe, expect, it } from 'vitest';
import { closeLabel, confirmLabel, formatBytes, keyLabel, reasonLabel, totalBytes } from '#lib/cleanup.ts';

describe('penutupan run selesai', () => {
  it('label tombol: tersembunyi bila tidak ada, tunggal dan jamak', () => {
    expect(closeLabel(0)).toBeNull();
    expect(closeLabel(1)).toBe('Close 1 finished run');
    expect(closeLabel(3)).toBe('Close 3 finished runs');
  });

  it('ukuran mudah dibaca; tidak terbaca ditandai', () => {
    expect(formatBytes(512)).toBe('512 B');
    expect(formatBytes(412_000)).toBe('412 KB');
    expect(formatBytes(29_100_000)).toBe('29.1 MB');
    expect(formatBytes(1_050_000_000)).toBe('1.1 GB');
    expect(formatBytes(null)).toBe('size unknown');
  });

  it('total ukuran mengabaikan yang tidak terbaca', () => {
    expect(totalBytes([{ bytes: 1000 }, { bytes: null }, { bytes: 500 }])).toBe(1500);
  });

  it('alasan dilewati', () => {
    expect(reasonLabel('unpushed_changes')).toBe('Unpushed changes');
    expect(reasonLabel('working')).toBe('Agent is working');
    expect(reasonLabel('lain')).toBe('lain');
  });

  it('tombol konfirmasi menyebut jumlah dan ukuran yang dibebaskan', () => {
    expect(confirmLabel(3, 1_000_000_000)).toBe('Close 3 runs · free 1.0 GB');
    expect(confirmLabel(1, 0)).toBe('Close 1 run');
  });

  it('kunci run grup diringkas: kunci pertama plus jumlah kartu lain', () => {
    expect(keyLabel(['PGS-1510'])).toBe('PGS-1510');
    expect(keyLabel(['PGS-1513', 'PGS-1514', 'PGS-1515', 'PGS-1516'])).toBe('PGS-1513 +3');
    expect(keyLabel([])).toBe('');
  });
});
