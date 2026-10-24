import { describe, expect, it } from 'vitest';
import { supportsVmLink } from './vm';

describe('supportsVmLink', () => {
  it('butuh daemon laptop yang mendukung klaim per daemon (protokol 1.1)', () => {
    expect(supportsVmLink('1.1')).toBe(true);
    expect(supportsVmLink('1.4')).toBe(true);
    expect(supportsVmLink('1.0')).toBe(false);
    expect(supportsVmLink(undefined)).toBe(false);
    expect(supportsVmLink('2.0')).toBe(false);
    expect(supportsVmLink('x')).toBe(false);
  });
});
