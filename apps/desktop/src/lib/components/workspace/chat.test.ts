import { describe, expect, it } from 'vitest';
import { startFixParams, toBlocks, toolLabel } from '#lib/chat.ts';
import { renderMarkdown } from '#lib/markdown.ts';
import type { MessageView } from '#lib/protocol/MessageView.ts';
import type { Rpc } from '#lib/rpc.ts';
import { WorkspaceStore } from '#lib/stores/workspace.svelte.ts';

function msg(id: number, role: string, body: Record<string, unknown>): MessageView {
  return { id, role, body, created_at_ms: id };
}

describe('chat workspace', () => {
  it('markdown berbahaya dirender tanpa atribut atau link berbahaya', () => {
    const html = renderMarkdown(
      '<img src=x onerror="alert(1)"> [klik](javascript:alert(1)) <a href="https://ok.test">ok</a>'
    );
    expect(html).not.toContain('onerror');
    expect(html).not.toContain('javascript:');
    expect(html).toContain('href="https://ok.test"');
    expect(html).toContain('rel="noopener noreferrer"');
  });

  it('prompt agent lama dengan blok data diringkas di chat', () => {
    const raw =
      'Brainstorm task PGS-1 with the developer.\n\n<<<MANDOR_DATA title nonce=ab>>>\nJudul\n<<<END_MANDOR_DATA nonce=ab>>>\n\nNote from the developer:\npakai komponen lama';
    const blocks = toBlocks([msg(1, 'dev', { type: 'text', text: raw })]);
    expect(blocks).toEqual([
      { kind: 'dev', id: 1, text: 'Task sent to the agent (title, description, and acceptance criteria from Lark).\n\nNote: pakai komponen lama' }
    ]);
    const plain = toBlocks([msg(2, 'dev', { type: 'text', text: 'tambahkan test' })]);
    expect(plain).toEqual([{ kind: 'dev', id: 2, text: 'tambahkan test' }]);
  });

  it('checklist markdown menjadi kotak centang tanpa elemen input', () => {
    const html = renderMarkdown('- [ ] belum\n- [x] sudah');
    expect(html).not.toContain('<input');
    expect(html).toContain('<span class="task-box" data-checked="false"');
    expect(html).toContain('<span class="task-box" data-checked="true"');
  });

  it('tool_use berurutan dikelompokkan dan tool_result ditautkan ke call-nya', () => {
    const blocks = toBlocks([
      msg(1, 'dev', { type: 'text', text: 'kerjakan' }),
      msg(2, 'agent', { type: 'assistant_message', text: 'Saya cek' }),
      msg(3, 'agent', { type: 'tool_use', id: 't1', name: 'Read', preview: 'src/a.rs' }),
      msg(4, 'agent', { type: 'tool_use', id: 't2', name: 'Bash', preview: 'git push' }),
      msg(5, 'agent', { type: 'tool_result', tool_use_id: 't1', is_error: false, preview: 'fn a()' }),
      msg(6, 'agent', { type: 'tool_result', tool_use_id: 't2', is_error: true, preview: 'diblokir' }),
      msg(7, 'agent', { type: 'permission_denied', tool_use_id: 't2', tool_name: 'Bash', reason: 'hook' }),
      msg(8, 'agent', { type: 'done', is_error: false, result: 'selesai' })
    ]);
    expect(blocks.map((b) => b.kind)).toEqual(['dev', 'assistant', 'tools', 'done']);
    const tools = blocks[2];
    if (tools.kind !== 'tools') throw new Error('bukan blok tools');
    expect(tools.calls).toHaveLength(2);
    expect(tools.calls[0].result).toEqual({ isError: false, preview: 'fn a()' });
    expect(tools.calls[1].result?.isError).toBe(true);
    expect(tools.calls[1].denied).toBe('hook');
  });

  it('draft pesan bertahan saat pindah workspace dan kembali', () => {
    const ws = new WorkspaceStore();
    const rpc: Rpc = { request: async <T>() => [] as T, on: () => () => undefined };
    ws.attach(rpc);
    ws.open('run-a');
    ws.setDraft('run-a', 'belum terkirim');
    ws.open('run-b');
    expect(ws.draft('run-b')).toBe('');
    ws.open('run-a');
    expect(ws.draft('run-a')).toBe('belum terkirim');
    ws.setDraft('run-a', '');
    expect(ws.drafts.has('run-a')).toBe(false);
  });

  it('pesan baru dimuat bertahap setelah event run.message', async () => {
    const pages: MessageView[][] = [[msg(1, 'dev', { text: 'a' })], [msg(2, 'agent', { type: 'done' })]];
    const handlers: ((p: unknown) => void)[] = [];
    const seen: unknown[] = [];
    const rpc: Rpc = {
      request: async <T>(_m: string, p?: unknown) => {
        seen.push(p);
        return (pages.shift() ?? []) as T;
      },
      on: (_m, h) => {
        handlers.push(h);
        return () => undefined;
      }
    };
    const ws = new WorkspaceStore();
    ws.attach(rpc);
    ws.open('r1');
    await new Promise((r) => setTimeout(r, 0));
    handlers[0]({ run_id: 'r1' });
    await new Promise((r) => setTimeout(r, 0));
    expect(ws.messages.get('r1')?.map((m) => m.id)).toEqual([1, 2]);
    expect((seen[1] as { after_id: number }).after_id).toBe(1);
  });

  it('Mulai perbaikan hanya mengirim instruksi dan komentar yang dipilih', () => {
    expect(startFixParams('rec1', '  Perbaiki form  ', { r1: true, r2: false, r3: true })).toEqual({
      record_id: 'rec1',
      instruction: 'Perbaiki form',
      reply_ids: ['r1', 'r3']
    });
  });
});

describe('ringkasan tool call', () => {
  it('menghitung baca, cari, edit, dan perintah', async () => {
    const { summarizeCalls } = await import('#lib/chat.ts');
    const call = (name: string) => ({ id: name + Math.random(), name, preview: '' });
    expect(summarizeCalls([call('Read'), call('Read'), call('Grep'), call('Edit'), call('Bash')])).toBe(
      'Read 2 files, searched 1 pattern, edited 1 file, ran 1 command'
    );
    expect(summarizeCalls([call('WebFetch')])).toBe('Used 1 tool');
  });
});

describe('nama tool', () => {
  it('tool MCP diringkas menjadi server dan nama tool', () => {
    expect(toolLabel('mcp__graft__graft_find_code')).toBe('graft · find_code');
    expect(toolLabel('mcp__figma__get_file')).toBe('figma · get_file');
    expect(toolLabel('Bash')).toBe('Bash');
  });
});
