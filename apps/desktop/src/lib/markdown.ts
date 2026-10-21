// Markdown ke HTML yang disanitasi. Konten agent dan Lark tidak tepercaya.

import DOMPurify from 'dompurify';
import { Marked } from 'marked';

const marked = new Marked({
  gfm: true,
  breaks: false,
  renderer: {
    // Elemen <input> dibuang sanitizer; checklist GFM memakai span bergaya kotak centang.
    checkbox({ checked }) {
      return `<span class="task-box" data-checked="${checked ? 'true' : 'false'}" aria-hidden="true"></span>`;
    }
  }
});
const cache = new Map<string, string>();
const CACHE_MAX = 2000;

let hooked = false;
function ensureHooks(): void {
  if (hooked) return;
  hooked = true;
  // Link selalu dibuka di luar app dan tanpa akses ke window pembuka.
  DOMPurify.addHook('afterSanitizeAttributes', (node) => {
    if (node.tagName === 'A') {
      node.setAttribute('target', '_blank');
      node.setAttribute('rel', 'noopener noreferrer');
    }
  });
}

export function renderMarkdown(raw: string): string {
  const hit = cache.get(raw);
  if (hit !== undefined) return hit;
  ensureHooks();
  const html = marked.parse(raw, { async: false });
  const safe = DOMPurify.sanitize(html, {
    FORBID_TAGS: ['style', 'iframe', 'form', 'input', 'object', 'embed'],
    FORBID_ATTR: ['style'],
    ALLOWED_URI_REGEXP: /^(?:https?:|mailto:|#)/i
  });
  if (cache.size >= CACHE_MAX) cache.clear();
  cache.set(raw, safe);
  return safe;
}
