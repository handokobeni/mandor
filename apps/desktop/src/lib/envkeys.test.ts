import { describe, expect, it } from 'vitest';
import { applyPicks, pickerRows } from './envkeys.ts';

describe('Pick from .env', () => {
  it('lists .env keys in file order with saved keys checked', () => {
    expect(pickerRows(['A', 'KELOLA_ADDR', 'B'], ['KELOLA_ADDR'])).toEqual([
      { name: 'A', checked: false, missing: false },
      { name: 'KELOLA_ADDR', checked: true, missing: false },
      { name: 'B', checked: false, missing: false }
    ]);
  });

  it('keeps saved keys that are not in .env, checked and marked missing (AE2)', () => {
    const rows = pickerRows(['A'], ['OLD_KEY', 'A']);
    expect(rows).toEqual([
      { name: 'A', checked: true, missing: false },
      { name: 'OLD_KEY', checked: true, missing: true }
    ]);
    expect(applyPicks(rows)).toEqual(['A', 'OLD_KEY']);
  });

  it('drops a missing key once unchecked', () => {
    const rows = pickerRows(['A'], ['OLD_KEY']).map((r) => (r.name === 'OLD_KEY' ? { ...r, checked: false } : r));
    expect(applyPicks(rows)).toEqual([]);
  });

  it('works without a .env file', () => {
    expect(pickerRows([], ['X'])).toEqual([{ name: 'X', checked: true, missing: true }]);
  });
});
