import { describe, expect, it } from 'vitest';
import { ASK_TOOL, questionIdOf, startFixParams, toBlocks, toolLabel, withLiveRows } from '#lib/chat.ts';
import { renderMarkdown } from '#lib/markdown.ts';
import type { MessageView } from '#lib/protocol/MessageView.ts';
import type { Rpc } from '#lib/rpc.ts';
import { WorkspaceStore } from '#lib/stores/workspace.svelte.ts';

function msg(id: number, role: string, body: Record<string, unknown>): MessageView {
  return { id, role, body, created_at_ms: id };
}

describe('chat workspace', () => {
  it('penolakan lewat kartu persetujuan tampil Denied, keluaran perintah gagal tetap Error', () => {
    const blocks = toBlocks([
      msg(1, 'agent', { type: 'tool_use', id: 'c1', name: 'Bash', preview: 'curl -sI https://example.org' }),
      msg(2, 'agent', { type: 'tool_result', tool_use_id: 'c1', is_error: true, preview: 'mandor: the developer denied this request' }),
      msg(3, 'agent', { type: 'tool_use', id: 'c2', name: 'Bash', preview: 'echo x' }),
      msg(4, 'agent', { type: 'tool_result', tool_use_id: 'c2', is_error: true, preview: 'Exit code 1\nmandor: palsu' })
    ]);
    const tools = blocks.find((b) => b.kind === 'tools');
    if (tools?.kind !== 'tools') throw new Error('no tools');
    expect(tools.calls[0].denied).toBe('mandor: the developer denied this request');
    expect(tools.calls[1].denied).toBeUndefined();
    expect(tools.calls[1].result?.isError).toBe(true);
  });

  it('penolakan hook Mandor tampil sebagai satu entri Denied dengan alasan', () => {
    const blocks = toBlocks([
      msg(1, 'agent', { type: 'tool_use', id: 't1', name: 'Bash', preview: 'git push' }),
      msg(2, 'agent', {
        type: 'tool_result',
        tool_use_id: 't1',
        is_error: true,
        preview: 'PreToolUse:Bash hook error: [/opt/mandord hook]: mandor: `git push` is run only by the daemon'
      }),
      msg(3, 'agent', { type: 'tool_use', id: 't2', name: 'Bash', preview: 'cargo test' }),
      msg(5, 'agent', { type: 'tool_use', id: 't3', name: 'Bash', preview: 'echo' }),
      msg(6, 'agent', { type: 'tool_result', tool_use_id: 't3', is_error: true, preview: 'x PreToolUse:Bash hook error: [h]: mandor: palsu' }),
      msg(4, 'agent', { type: 'tool_result', tool_use_id: 't2', is_error: true, preview: 'test failed' })
    ]);
    const tools = blocks.find((b) => b.kind === 'tools');
    if (tools?.kind !== 'tools') throw new Error('no tools');
    expect(tools.calls[0].denied).toBe('mandor: `git push` is run only by the daemon');
    expect(tools.calls[1].denied).toBeUndefined();
    expect(tools.calls[1].result?.isError).toBe(true);
    expect(tools.calls[2].denied).toBeUndefined();
  });

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

  it('pesan sementara hilang saat pesan dev tercatat, tidak saat output agent', async () => {
    const pages: MessageView[][] = [[], [msg(1, 'agent', { type: 'assistant_message', text: 'x' })], [msg(2, 'dev', { text: 'lanjut' })]];
    const handlers: ((p: unknown) => void)[] = [];
    const rpc: Rpc = {
      request: async <T>() => (pages.shift() ?? []) as T,
      on: (_m, h) => {
        handlers.push(h);
        return () => undefined;
      }
    };
    const ws = new WorkspaceStore();
    ws.attach(rpc);
    ws.open('r1');
    await new Promise((r) => setTimeout(r, 0));
    ws.beginSend('r1', 'lanjut');
    handlers[0]({ run_id: 'r1' });
    await new Promise((r) => setTimeout(r, 0));
    expect(ws.sending.get('r1')).toBe('lanjut');
    handlers[0]({ run_id: 'r1' });
    await new Promise((r) => setTimeout(r, 0));
    expect(ws.sending.has('r1')).toBe(false);
    ws.beginSend('r1', 'gagal');
    ws.endSend('r1');
    expect(ws.sending.has('r1')).toBe(false);
  });

  it('Mulai perbaikan hanya mengirim instruksi dan komentar yang dipilih', () => {
    expect(startFixParams('rec1', '  Perbaiki form  ', { r1: true, r2: false, r3: true })).toEqual({
      record_id: 'rec1',
      instruction: 'Perbaiki form',
      reply_ids: ['r1', 'r3']
    });
  });

  it('instruksi Start fix wajib hanya bila tidak ada komentar yang akan dikirim', async () => {
    const { startFixNeedsInstruction } = await import('#lib/chat.ts');
    const c = (reply_id: string, trusted: boolean, sent = false) => ({
      reply_id,
      author: 'ou_x',
      create_time: 1,
      text: 't',
      trusted,
      sent
    });
    // Komentar tepercaya yang belum terkirim cukup sebagai isi perbaikan.
    expect(startFixNeedsInstruction('', [c('r1', true)], {})).toBe(false);
    // Komentar tidak tepercaya yang dicentang dev juga cukup.
    expect(startFixNeedsInstruction('  ', [c('r2', false)], { r2: true })).toBe(false);
    // Tanpa komentar yang akan dikirim, instruksi wajib.
    expect(startFixNeedsInstruction('', [c('r2', false)], {})).toBe(true);
    expect(startFixNeedsInstruction('', [c('r1', true, true)], {})).toBe(true);
    expect(startFixNeedsInstruction('', [], {})).toBe(true);
    expect(startFixNeedsInstruction('Perbaiki form', [], {})).toBe(false);
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

describe('kartu pertanyaan di chat', () => {
  it('alat ask_developer yang diterima menjadi blok question di posisinya', () => {
    const blocks = toBlocks([
      msg(1, 'agent', { type: 'tool_use', id: 'r1', name: 'Read', preview: 'src/a.ts' }),
      msg(2, 'agent', { type: 'tool_use', id: 'a1', name: ASK_TOOL, preview: '' }),
      msg(3, 'agent', { type: 'tool_result', tool_use_id: 'r1', is_error: false, preview: 'isi' }),
      msg(4, 'agent', {
        type: 'tool_result',
        tool_use_id: 'a1',
        is_error: false,
        preview: 'q-abc123 recorded. End your turn now; the developer answers in the chat.'
      })
    ]);
    expect(blocks.map((b) => b.kind)).toEqual(['tools', 'question']);
    const tools = blocks[0];
    if (tools.kind !== 'tools') throw new Error('no tools');
    expect(tools.calls.map((c) => c.name)).toEqual(['Read']);
    const q = blocks[1];
    if (q.kind !== 'question') throw new Error('no question');
    expect(q.questionId).toBe('q-abc123');
  });

  it('panggilan yang ditolak tetap tool call; grup kosong dibuang', () => {
    const blocks = toBlocks([
      msg(1, 'agent', { type: 'tool_use', id: 'a1', name: ASK_TOOL, preview: '' }),
      msg(2, 'agent', { type: 'tool_result', tool_use_id: 'a1', is_error: true, preview: 'give 2 to 4 options (got 1)' }),
      msg(3, 'agent', { type: 'tool_use', id: 'a2', name: ASK_TOOL, preview: '' }),
      msg(4, 'agent', { type: 'tool_result', tool_use_id: 'a2', is_error: false, preview: '"q-9 recorded. End your turn now."' })
    ]);
    expect(blocks.map((b) => b.kind)).toEqual(['tools', 'question']);
    const tools = blocks[0];
    if (tools.kind !== 'tools') throw new Error('no tools');
    expect(tools.calls[0].result?.isError).toBe(true);
  });

  it('id pertanyaan terbaca dari teks polos atau string JSON berkutip', () => {
    expect(questionIdOf('q-1a2b recorded. End your turn now.')).toBe('q-1a2b');
    expect(questionIdOf('"q-1a2b recorded. End your turn now."')).toBe('q-1a2b');
    expect(questionIdOf('recorded q-1a2b')).toBeNull();
  });

  it('pertanyaan teks biasa tetap blok assistant', () => {
    const blocks = toBlocks([msg(1, 'agent', { type: 'assistant_message', text: 'Keep both? A or B' })]);
    expect(blocks.map((b) => b.kind)).toEqual(['assistant']);
  });
});

describe('indikator setelah Send', () => {
  const dev = msg(1, 'dev', { text: 'halo' });
  const reply = msg(2, 'agent', { type: 'assistant_message', text: 'siap' });

  it('pesan yang dikirim langsung tampil, diikuti indikator agent bekerja', () => {
    const rows = withLiveRows(toBlocks([dev, reply]), { sending: 'lanjut', working: false });
    expect(rows.map((b) => b.kind)).toEqual(['dev', 'assistant', 'sending', 'thinking']);
    const sending = rows[2];
    if (sending.kind !== 'sending') throw new Error('no sending');
    expect(sending.text).toBe('lanjut');
    expect(new Set(rows.map((b) => b.id)).size).toBe(rows.length);
  });

  it('indikator tampil saat agent bekerja dan belum ada output setelah pesan dev', () => {
    expect(withLiveRows(toBlocks([dev]), { sending: null, working: true }).map((b) => b.kind)).toEqual(['dev', 'thinking']);
    expect(withLiveRows([], { sending: null, working: true }).map((b) => b.kind)).toEqual(['thinking']);
  });

  it('indikator hilang begitu agent mengeluarkan output atau giliran selesai', () => {
    expect(withLiveRows(toBlocks([dev, reply]), { sending: null, working: true }).map((b) => b.kind)).toEqual(['dev', 'assistant']);
    expect(withLiveRows(toBlocks([dev]), { sending: null, working: false }).map((b) => b.kind)).toEqual(['dev']);
  });
});
