import { describe, expect, it } from 'vitest';
import { applyCommand, commandQuery, suggestCommands } from '#lib/commands.ts';

const skills = [
  { name: 'compound-engineering:ce-plan', description: 'Plan' },
  { name: 'compound-engineering:ce-brainstorm', description: 'Brainstorm' },
  { name: 'tdd', description: 'Test-driven' }
];

describe('perintah garis miring di chat', () => {
  it('mode perintah hanya saat draft diawali / dan belum ada spasi', () => {
    expect(commandQuery('/')).toBe('');
    expect(commandQuery('/ce-pl')).toBe('ce-pl');
    expect(commandQuery('/tdd lanjut')).toBeNull();
    expect(commandQuery('halo /tdd')).toBeNull();
  });

  it('saran cocok sebagian nama, yang diawali kata kunci di atas', () => {
    expect(suggestCommands('', skills).map((s) => s.name)).toHaveLength(3);
    expect(suggestCommands('PLAN', skills).map((s) => s.name)).toEqual(['compound-engineering:ce-plan']);
    expect(suggestCommands('t', skills)[0].name).toBe('tdd');
  });

  it('memilih saran mengganti draft dengan nama perintah dan spasi', () => {
    expect(applyCommand('compound-engineering:ce-plan')).toBe('/compound-engineering:ce-plan ');
  });
});
