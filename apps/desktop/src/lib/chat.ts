// Pengelompokan pesan chat run untuk tampilan.

import type { CommentView } from '#lib/protocol/CommentView.ts';
import type { MessageView } from '#lib/protocol/MessageView.ts';

export interface ToolCall {
  id: string;
  name: string;
  preview: string;
  result?: { isError: boolean; preview: string };
  denied?: string;
}

export type ChatBlock =
  | { kind: 'dev'; id: number; text: string }
  | { kind: 'system'; id: number; text: string }
  | { kind: 'assistant'; id: number; text: string }
  | { kind: 'tools'; id: number; calls: ToolCall[] }
  | { kind: 'question'; id: number; questionId: string }
  /** Pesan dev yang sedang dikirim; belum tercatat daemon. */
  | { kind: 'sending'; id: number; text: string }
  /** Agent bekerja tapi belum ada output setelah pesan dev terakhir. */
  | { kind: 'thinking'; id: number }
  | { kind: 'done'; id: number; isError: boolean; text: string }
  | { kind: 'failed'; id: number; text: string };

/** Alasan penolakan Mandor dari hasil tool (hook atau alat persetujuan). */
function hookDenial(preview: string): string | null {
  // Hanya di awal hasil: penolakan hook (`PreToolUse:… mandor: …`) atau alat
  // persetujuan (`mandor: …`). Perintah yang benar-benar berjalan dan gagal
  // diawali "Exit code N", jadi keluarannya tidak bisa menyamar.
  const hook = preview.match(/^PreToolUse:\S+ hook error: \[[^\]]*\]: (mandor: [\s\S]*)$/);
  if (hook) return hook[1].trim();
  return /^mandor: \S/.test(preview) ? preview.trim() : null;
}

/** Alat pertanyaan yang bisa diklik (plan 2026-11-07-001). */
export const ASK_TOOL = 'mcp__mandor__ask_developer';

/** Id pertanyaan di awal hasil alat yang diterima (KTD-Q2), polos atau berkutip JSON. */
export function questionIdOf(preview: string): string | null {
  return preview.match(/^"?(q-[A-Za-z0-9-]+) recorded/)?.[1] ?? null;
}

function str(v: unknown): string {
  return typeof v === 'string' ? v : '';
}

/** Tool call berurutan digabung satu blok; hasil dan penolakan ditautkan ke call-nya. */
export function toBlocks(messages: MessageView[]): ChatBlock[] {
  const out: ChatBlock[] = [];
  const calls = new Map<string, ToolCall>();
  const owner = new Map<string, ToolCall[]>();
  for (const m of messages) {
    const b = m.body as Record<string, unknown>;
    const type = str(b.type);
    if (m.role === 'dev') {
      out.push({ kind: 'dev', id: m.id, text: devText(str(b.text)) });
      continue;
    }
    if (m.role === 'system') {
      out.push({ kind: 'system', id: m.id, text: str(b.text) });
      continue;
    }
    switch (type) {
      case 'assistant_message':
        out.push({ kind: 'assistant', id: m.id, text: str(b.text) });
        break;
      case 'tool_use': {
        const call: ToolCall = { id: str(b.id), name: str(b.name), preview: str(b.preview) };
        calls.set(call.id, call);
        const last = out.at(-1);
        if (last?.kind === 'tools') {
          last.calls.push(call);
          owner.set(call.id, last.calls);
        } else {
          const group = [call];
          out.push({ kind: 'tools', id: m.id, calls: group });
          owner.set(call.id, group);
        }
        break;
      }
      case 'tool_result': {
        const call = calls.get(str(b.tool_use_id));
        if (!call) break;
        // Pertanyaan yang diterima menjadi kartu di posisi alat itu (R7).
        const qid = call.name === ASK_TOOL && b.is_error !== true ? questionIdOf(str(b.preview)) : null;
        if (qid) {
          const group = owner.get(call.id);
          group?.splice(group.indexOf(call), 1);
          out.push({ kind: 'question', id: m.id, questionId: qid });
          break;
        }
        const denial = b.is_error === true ? hookDenial(str(b.preview)) : null;
        if (denial) call.denied = denial;
        else call.result = { isError: b.is_error === true, preview: str(b.preview) };
        break;
      }
      case 'permission_denied': {
        const call = calls.get(str(b.tool_use_id));
        if (call) call.denied = str(b.reason);
        break;
      }
      case 'done':
        out.push({ kind: 'done', id: m.id, isError: b.is_error === true, text: str(b.result) });
        break;
      case 'failed':
        out.push({ kind: 'failed', id: m.id, text: str(b.message) });
        break;
      default:
        break;
    }
  }
  return out.filter((b) => b.kind !== 'tools' || b.calls.length > 0);
}

/**
 * Baris sementara di akhir chat: pesan yang sedang dikirim, lalu indikator
 * "Agent is working…" sampai output pertama agent masuk.
 */
export function withLiveRows(blocks: ChatBlock[], opts: { sending: string | null; working: boolean }): ChatBlock[] {
  const out = [...blocks];
  const next = (blocks.at(-1)?.id ?? 0) + 1;
  if (opts.sending !== null) out.push({ kind: 'sending', id: next, text: opts.sending });
  const last = out.at(-1);
  const waiting = last === undefined || last.kind === 'dev' || last.kind === 'sending';
  if ((opts.working || opts.sending !== null) && waiting) out.push({ kind: 'thinking', id: next + 1 });
  return out;
}

function plural(n: number, word: string): string {
  return `${n} ${word}${n === 1 ? '' : 's'}`;
}

/** Ringkasan satu grup tool call, mis. "Read 2 files, ran 1 command". */
export function summarizeCalls(calls: ToolCall[]): string {
  const c = { read: 0, search: 0, edit: 0, cmd: 0, other: 0 };
  for (const call of calls) {
    if (call.name === 'Read') c.read++;
    else if (call.name === 'Grep' || call.name === 'Glob') c.search++;
    else if (['Edit', 'Write', 'MultiEdit', 'NotebookEdit'].includes(call.name)) c.edit++;
    else if (call.name === 'Bash') c.cmd++;
    else c.other++;
  }
  const parts = [
    c.read ? `read ${plural(c.read, 'file')}` : '',
    c.search ? `searched ${plural(c.search, 'pattern')}` : '',
    c.edit ? `edited ${plural(c.edit, 'file')}` : '',
    c.cmd ? `ran ${plural(c.cmd, 'command')}` : '',
    c.other ? `used ${plural(c.other, 'tool')}` : ''
  ].filter(Boolean);
  const text = parts.join(', ');
  return text.charAt(0).toUpperCase() + text.slice(1);
}

/** Parameter "Mulai perbaikan": hanya komentar yang dipilih dev. */
export function startFixParams(
  recordId: string,
  instruction: string,
  selected: Record<string, boolean>
): { record_id: string; instruction: string; reply_ids: string[] } {
  return {
    record_id: recordId,
    instruction: instruction.trim(),
    reply_ids: Object.entries(selected)
      .filter(([, on]) => on)
      .map(([id]) => id)
  };
}

/**
 * Instruksi "Mulai perbaikan" wajib hanya bila tidak ada komentar yang akan dikirim:
 * komentar tepercaya yang belum terkirim, atau komentar yang dicentang dev.
 */
export function startFixNeedsInstruction(
  instruction: string,
  comments: CommentView[],
  selected: Record<string, boolean>
): boolean {
  if (instruction.trim()) return false;
  return !comments.some((c) => (c.trusted && !c.sent) || selected[c.reply_id]);
}

/** Nama tool untuk ditampilkan: `mcp__graft__graft_find_code` menjadi `graft · find_code`. */
export function toolLabel(name: string): string {
  const m = name.match(/^mcp__([^_]+(?:_[^_]+)*?)__(.+)$/);
  if (!m) return name;
  const [, server, tool] = m;
  const short = tool.startsWith(`${server}_`) ? tool.slice(server.length + 1) : tool;
  return `${server} · ${short}`;
}

/**
 * Pesan dev untuk chat. Run lama menyimpan prompt lengkap agent (berisi blok
 * data Lark); yang ditampilkan cukup ringkasan dan catatan dev.
 */
function devText(text: string): string {
  if (!text.includes('<<<MANDOR_DATA')) return text;
  const summary = 'Task sent to the agent (title, description, and acceptance criteria from Lark).';
  const note = text.split('Note from the developer:\n')[1]?.trim();
  return note ? `${summary}\n\nNote: ${note}` : summary;
}
