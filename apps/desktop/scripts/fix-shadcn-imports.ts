// CLI shadcn-svelte menulis impor alias `#lib/...` dengan akhiran `.js`, padahal
// subpath import `#lib/*` di SvelteKit 3 menunjuk berkas sumber `.ts`.
// Jalankan setelah `bunx shadcn-svelte add ...`: `bun scripts/fix-shadcn-imports.ts`.

import { readdirSync, readFileSync, statSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';

const roots = ['src/lib/components/ui', 'src/lib/hooks'];

function walk(dir: string): string[] {
  return readdirSync(dir).flatMap((name) => {
    const p = join(dir, name);
    return statSync(p).isDirectory() ? walk(p) : [p];
  });
}

let changed = 0;
for (const file of roots.flatMap(walk)) {
  if (!/\.(svelte|ts)$/.test(file)) continue;
  const before = readFileSync(file, 'utf8');
  const after = before
    .replaceAll('"#lib/utils.ts.js"', '"#lib/utils.ts"')
    .replace(/"(#lib\/[^"]+?)\.js"/g, '"$1.ts"');
  if (after !== before) {
    writeFileSync(file, after);
    changed++;
  }
}
console.log(`fix-shadcn-imports: ${changed} berkas diperbarui`);
