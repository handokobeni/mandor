// Pengelompokan pesan chat run untuk tampilan.

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
  | { kind: 'done'; id: number; isError: boolean; text: string }
  | { kind: 'failed'; id: number; text: string };

function str(v: unknown): string {
  return typeof v === 'string' ? v : '';
}

/** Tool call berurutan digabung satu blok; hasil dan penolakan ditautkan ke call-nya. */
export function toBlocks(messages: MessageView[]): ChatBlock[] {
  const out: ChatBlock[] = [];
  const calls = new Map<string, ToolCall>();
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
        if (last?.kind === 'tools') last.calls.push(call);
        else out.push({ kind: 'tools', id: m.id, calls: [call] });
        break;
      }
      case 'tool_result': {
        const call = calls.get(str(b.tool_use_id));
        if (call) call.result = { isError: b.is_error === true, preview: str(b.preview) };
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
