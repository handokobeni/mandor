import { describe, expect, it } from 'vitest';
import { offsetsOf, visibleRange } from './virtual.ts';

describe('virtual list', () => {
  it('memakai perkiraan tinggi untuk baris yang belum diukur', () => {
    expect(offsetsOf([1, 2, 3], new Map([[2, 30]]), 80)).toEqual([0, 80, 110, 190]);
  });

  it('tinggi 0 dari panel tersembunyi tidak membuat semua baris terlewati', () => {
    const heights = new Map([
      [1, 0],
      [2, 0],
      [3, 0],
      [4, 0]
    ]);
    const offsets = offsetsOf([1, 2, 3, 4], heights, 80);
    expect(visibleRange(offsets, 0, 0, 600)).toEqual({ start: 0, end: 4 });
  });

  it('hanya merender baris di sekitar area yang terlihat', () => {
    const offsets = offsetsOf([...Array(100).keys()], new Map(), 100);
    expect(visibleRange(offsets, 5000, 500, 200)).toEqual({ start: 48, end: 58 });
  });
});
