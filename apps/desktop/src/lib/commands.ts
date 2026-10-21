// Perintah `/skill` di chat: saran dari skill plugin Claude di mesin daemon.

import type { SkillView } from '#lib/protocol/SkillView.ts';

/** Kata kunci perintah saat draft diawali `/` dan belum ada spasi; selain itu `null`. */
export function commandQuery(draft: string): string | null {
  if (!draft.startsWith('/') || /\s/.test(draft)) return null;
  return draft.slice(1);
}

/** Skill yang namanya memuat kata kunci; yang diawali kata kunci (setelah `plugin:`) di atas. */
export function suggestCommands(query: string, skills: SkillView[]): SkillView[] {
  const q = query.toLowerCase();
  const starts = (name: string) => {
    const short = name.slice(name.lastIndexOf(':') + 1).toLowerCase();
    return name.toLowerCase().startsWith(q) || short.startsWith(q);
  };
  const hits = skills.filter((s) => s.name.toLowerCase().includes(q));
  return [...hits.filter((s) => starts(s.name)), ...hits.filter((s) => !starts(s.name))];
}

/** Draft setelah memilih saran. */
export function applyCommand(name: string): string {
  return `/${name} `;
}
