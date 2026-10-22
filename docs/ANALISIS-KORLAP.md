# Analisis Menyeluruh Korlap — Panduan Clone

> Sumber: `korlap/` (github.com/ariaghora/korlap, 248 commit, 17 Mar – 10 Apr 2026, lisensi **MIT** → boleh di-clone/dimodifikasi asal menyertakan notice lisensi & copyright).
> Spesifikasi detail (dengan referensi `file:line`) ada di:
> - [`spec/01-backend-core.md`](spec/01-backend-core.md) — Rust core: state, worktree, git/gh, PTY, scripts, files, persistensi
> - [`spec/02-agent-pipeline.md`](spec/02-agent-pipeline.md) — spawn `claude`, parsing stream-json, **teks prompt lengkap**, MCP, LSP, knowledge base, Jira, OAuth, autopilot
> - [`spec/03-frontend.md`](spec/03-frontend.md) — komponen, state, `ipc.ts`, kanban, chat, shortcut, tema, RepoSettings

---

## 1. Apa itu Korlap (dalam satu paragraf)

Aplikasi desktop (Tauri v2 + Svelte 5 + Rust) yang menjadikan developer **orkestrator** banyak agent Claude Code yang berjalan paralel. Setiap task di papan kanban → satu **workspace** = satu `git worktree` di branch terisolasi (`korlap/<adj-noun>`) + satu sesi `claude -p --output-format stream-json`. Developer melihat chat terstruktur, diff, file, terminal, menjalankan script, me-review (prompt multi-agent ala `/code-review`), lalu commit → push → PR → merge — semuanya dari satu UI. Ada **Autopilot** yang mengambil todo otomatis, menjawab pertanyaan agent, me-review, dan membuat PR; serta **Knowledge Base** per repo (invariants/facts/context) yang disuntikkan ke system prompt.

Ukuran: ~40k LOC (Rust ~13.7k, Svelte/TS ~26.4k), ~100 Tauri command, 13 tool MCP.

---

## 2. Arsitektur

```
┌──────────────────────────── Svelte 5 SPA (adapter-static, ssr=false) ────────────────────────────┐
│ +page.svelte (3.5k baris: SEMUA state & orkestrasi: queue, PR state machine, review, autopilot)  │
│   TitleBar · Sidebar · KanbanBoard · WorkspacePanel(Diff/Files/Terminal/Chat overlay) · Settings │
│ stores: messages (SvelteMap<ws, SvelteMap<msg>>), toasts, theme · ipc.ts (typed invoke wrapper)  │
└───────────────┬──────────────────────────────────────────────▲───────────────────────────────────┘
        invoke()│                         Channel<AgentEvent>  │  emit: agent-status, workspace-updated,
                ▼                         Channel<Vec<u8>> PTY │        todos-changed, lsp-status, gh-auth-code
┌──────────────────────────── Rust (Tauri v2) ─────────────────┴───────────────────────────────────┐
│ AppState (Arc<Mutex>) · LspServerPool (Arc<Mutex>) · GitProviderRegistry (GitHub)               │
│ commands/: repo · workspace · git · github · files · terminal · scripts · persistence · staging  │
│            agent · agent_backend(Claude|Codex) · context(KB) · lsp · jira · oauth · system       │
│ mcp_api.rs: HTTP 127.0.0.1:<acak> (tanpa auth) ◄──────────────┐                                  │
└──────┬──────────────┬──────────────┬──────────────────────────┼──────────────────────────────────┘
       │spawn         │spawn         │stdio JSON-RPC            │HTTP
       ▼              ▼              ▼                          │
  claude -p …     git / gh       rust-analyzer, tsserver,   bun run src-mcp/server.ts (MCP stdio,
  (cwd=worktree)  (token per     gopls, pyright             13 tool) ◄── di-spawn oleh claude
                  proses)                                        via --mcp-config
```

**Penyimpanan** (semua di app-data dir `net.ghora.korlap`, nol file di repo yang dikelola):
`repos.json`, `workspaces.json`, `sessions.json`, `repo_settings.json`, `context_meta.json`, `messages/<ws>.json`, `todos/<repo>.json`, `images/<ws>/…`, `mcp/<ws>.json`, `context/<repo>/*.md`, `workspaces/<nama-acak>/` (worktree).

---

## 3. Inventaris fitur & prioritas untuk clone

| # | Fitur | Inti mekanisme | Prioritas clone |
|---|---|---|---|
| 1 | Repo management | add lokal / clone dari GitHub / create repo; deteksi default branch dari `origin/HEAD\|main\|master` | **MVP** |
| 2 | Profil `gh` per repo | `gh auth token --user <p>` → `GH_TOKEN` + rewrite `insteadOf` per proses | **MVP** |
| 3 | Workspace = worktree | fetch → `git worktree add -b korlap/<adj-noun> <data>/workspaces/<dir> origin/<base>`; setup/remove script + env `KORLAP_*` | **MVP** |
| 4 | Chat agent terstruktur | `claude -p --output-format stream-json --verbose`, `--resume <sid>`, parse NDJSON → `AgentEvent` via Channel | **MVP** |
| 5 | Diff viewer | merge-base vs working tree + untracked; quote baris ke chat | **MVP** |
| 6 | Commit/push/PR/merge | Haiku commit message → commit → push → agent diminta `gh pr create`; merge `--squash` | **MVP** |
| 7 | Kanban (Todo/In Progress/Review/Done) | kolom diturunkan dari status PR (bukan drag-drop) | **MVP** |
| 8 | Terminal PTY | portable-pty → Channel bytes → xterm.js; `display:none` saat ganti workspace | MVP+ |
| 9 | Script runner | `zsh -c` di process group, stream output | MVP+ |
| 10 | File browser + CodeMirror editor + fuzzy/grep search | `ignore` walker, SkimMatcher, grep-searcher | MVP+ |
| 11 | Review flow | prompt multi-agent (`review-prompt.ts`) di sesi yang sama → `[CLEAN]`/`[ISSUES]` | Fase 2 |
| 12 | Plan mode / thinking / model picker / AskUserQuestion UI | `--permission-mode plan`, `--effort`, `--model` | Fase 2 |
| 13 | Suggested replies, auto-rename branch | Haiku one-shot (15–30s timeout) | Fase 2 |
| 14 | MCP server Korlap + MCP user | `--mcp-config` per workspace; HTTP API lokal | Fase 2 |
| 15 | LSP (untuk editor & agent) | pool per (repo, bahasa), JSON-RPC stdio | Fase 3 |
| 16 | PR checkout / combo PR / staging | fetch `pull/N/head`, topo-sort Kahn, merge berantai | Fase 3 |
| 17 | Autopilot | evaluator single-flight: auto-answer → finalize review → spawn todo → auto-review → fix PR | Fase 3 |
| 18 | Knowledge base | agent build (5 subagent) → invariants/facts/context/index/contradictions; pre-check invariants sebelum review | Fase 4 |
| 19 | Jira import, MCP OAuth, Codex backend | `jira issue list --raw`; delegasi OAuth ke claude CLI | Opsional |
| 20 | Tema (amber/indigo/peach/slate), tooltip, traffic-light macOS | CSS custom properties | Kosmetik |

---

## 4. Alur inti yang wajib direplikasi

### 4.1 Siklus hidup task
```
Todo ──[Start / ⌘⇧Enter / autopilot]──► create_workspace ──► send_message (prompt tersembunyi: title+desc+@files+gambar)
  │                                           │
  │                                   agent bekerja (stream) ──► done ──► refresh diff/PR, suggested replies, drain queue
  ▼                                           │
In Progress (PR none) ──[⌘M "Push & create PR"]──► commit(Haiku msg) → push → agent: `gh pr create --base <default>`
  ▼
Review (PR open) ──[poll 5s]── conflict? → prompt agent resolve · checks fail? → prompt agent `gh pr checks`
  ▼                             ada perubahan lokal? → commit+push · bersih → [⌘M] merge --squash
Done (PR merged) ──► regenerate hot.md + update KB after merge
```

### 4.2 Pemanggilan agent (inti "IP" aplikasi)
```
claude -p "<prompt>" --output-format stream-json --verbose
  work: --permission-mode bypassPermissions --disallowedTools "EnterWorktree,ExitWorktree,LSP"
  plan: --permission-mode plan --allowedTools "<mcp korlap lsp/rename + WebSearch/WebFetch>"
  --add-dir <data>/images  [--model sonnet|opus|haiku] [--effort high]
  turn 1: --system-prompt "<prompt isolasi worktree + LSP + token-efficiency + KB + PR ctx + preferensi user>"
  turn n: --resume <session_id>
  [--mcp-config <data>/mcp/<ws>.json]
cwd = worktree; env = shell-env login + GH_TOKEN + GIT_CONFIG_PARAMETERS
```
Parsing: `system`→session id, `assistant`→{text, tool_uses (dengan input_preview), thinking}, usage (input = input + cache_creation + cache_read), `result`→done + usage kumulatif. Teks semua prompt (system, branch namer, commit, suggest, prioritize, dependencies, autopilot, review, KB build/update, invariants check) **dikutip lengkap** di `spec/02-agent-pipeline.md §3 & §6`.

### 4.3 Queue pesan per workspace
Agent sibuk → antre. Drain dua fase: event Channel `done` set `pendingDrain`, lalu event global `agent-status: waiting` yang benar-benar mengirim. "Send now" = stop + kirim.

### 4.4 Autopilot (evaluasi satu aksi per putaran, berurutan)
1. Auto-jawab (execute plan / `suggestReplies[0]` / "Yes, proceed.")
2. Finalisasi review (clean → buat PR; issues → "Fix them all")
3. Spawn todo berikutnya yang ready & tidak terblokir, selama < `max(2, min(cpu/2, memGB/4))` agent
4. Auto-review workspace idle yang punya diff (maks 5 siklus)
5. PR conflict / checks gagal → minta agent memperbaiki
Tidak pernah merge otomatis. Dependensi ditentukan Haiku; prioritas oleh Haiku.

---

## 5. Pola desain yang layak DIPERTAHANKAN

1. Data app di luar repo; worktree di app-data; semua branch `korlap/*` dari `origin/<default>` setelah fetch segar.
2. Token per proses (`gh auth token --user`) — tidak pernah `gh auth switch`.
3. Shell env ditangkap sekali dari login shell (penting untuk app yang diluncurkan dari GUI launcher).
4. `SvelteMap` per pesan + VirtualScroller tinggi variabel → chat panjang tetap cepat.
5. PTY bytes lewat Tauri Channel langsung ke xterm, tidak pernah lewat state Svelte; terminal disembunyikan, tidak di-unmount.
6. CodeMirror dengan compartments (tidak pernah rebuild editor).
7. Error dengan hint remediasi (mis. "token expired → `gh auth refresh`").
8. Haiku untuk semua utilitas murah (commit msg, rename, suggest, prioritize) dengan timeout.
9. MCP server sendiri sebagai jembatan agent → app (rename branch, todo CRUD, LSP).

---

## 6. Masalah di Korlap asli yang HARUS diperbaiki di clone

### Kritis (kehilangan data / freeze / keamanan)
| Masalah | Lokasi | Perbaikan |
|---|---|---|
| `sync_main` pakai `update-ref` tanpa cek fast-forward → commit lokal di main hilang | `git.rs:483-497` | cek `merge-base --is-ancestor` dulu |
| Race stop→send: reader lama menghapus handle agent BARU lalu `wait()` sambil memegang lock global → app freeze | `agent.rs:487-499` | generation id / cek pid |
| Lock `AppState` ditahan saat menjalankan remove-script, git, PTY write | `workspace.rs:892`, `staging.rs:46`, `terminal.rs:185` | ambil data → lepas lock → kerja |
| `--system-prompt` hanya turn 1; `--resume` tidak membawanya → aturan isolasi hilang mulai turn 2 | `agent.rs:264` | `--append-system-prompt` tiap turn |
| MCP server tidak ikut di release build (path compile-time) | `agent.rs:236-252` | bundel sebagai sidecar (bun compile) |
| HTTP API MCP tanpa auth; JSON dibangun dengan `format!` tanpa escape | `mcp_api.rs` | bearer token per run + serde_json |
| LSP rename bisa menulis ke repo utama; offset UTF-16 diperlakukan byte → panic | `lsp/mod.rs:318-423` | clamp ke worktree, konversi UTF-16 |
| Slice UTF-8 `[..4000]`, `[..30000]`, `truncate(500)` → panic | `workspace.rs:356`, `context.rs:763`, `files.rs:661` | potong di char boundary |
| `csp: null` + IPC kuat (`write_file`, `run_script`) → XSS markdown = RCE | `tauri.conf.json` | CSP ketat |
| Token muncul di argv & pesan error | `github.rs:216`, `workspace.rs:115` | `GIT_CONFIG_COUNT/KEY/VALUE` env + redaksi |

### Penting
- Tidak ada timeout pada git/gh/clone/script/agent utama; pekerjaan blocking di command `async` & sync di main thread.
- `remove_repo` tidak menyimpan `workspaces.json` → orphan muncul lagi; branch workspace tidak pernah dihapus.
- Stop hanya membunuh `claude`, bukan `bun` MCP/child Bash; tidak ada cleanup saat app keluar.
- `generate_commit_message` men-`git add -A` sebagai efek samping & bocor proses saat timeout.
- Status PR: `StatusContext` tidak terbaca → "pending" selamanya.
- Frontend: ChatPanel sebenarnya di-unmount saat ganti workspace (draft hilang); PR closed hilang dari kanban; line number mention terbuang; "Esc to stop" tidak ada; test Playwright usang semua; `+page.svelte` 3.5k baris god-component.
- Hardcode `zsh`, `open` (macOS), path `~/Developer` → masalah di Linux/WSL.
- Docs drift: CLAUDE.md bilang "jangan buat Codex" tapi backend Codex sudah ada.

Daftar lengkap: `spec/01 §7` (backend, ~30 item), `spec/02 §9` (pipeline, 23 item), `spec/03 §10.4` (frontend, 28 item).

---

## 7. Rekomendasi arsitektur clone

> **Keputusan (16 Okt 2026):** daemon Rust headless (`korlapd`) + client **Tauri v2 + Svelte 5**; tulis ulang bertahap; multi-platform termasuk WSL (daemon di WSL, UI native Windows); semua fitur ikut, **Jira diganti Lark Base** dengan sinkron dua arah. Detail: [`PLAN-FASE-0.md`](PLAN-FASE-0.md).

Perubahan struktural dari Korlap asli:

1. **Backend**
   - Satu helper `run_cmd(program, args, cwd, env, timeout)` → selalu inject shell env, timeout, redaksi token. Pakai `tokio::process` atau `spawn_blocking`.
   - Pisahkan state per domain (repos, workspaces, agents, terminals) dengan lock sendiri, atau actor/channel per workspace.
   - Tulis JSON secara atomik (tmp + rename); load toleran per-entry.
   - `AgentRun { generation, pgid }` → stop membunuh process group; hook `on_exit` membunuh semuanya.
   - MCP server dibundel sebagai binary sidecar; HTTP API pakai `axum`/`tiny_http` + bearer token.
2. **Frontend**
   - Pecah `+page.svelte` menjadi store/service: `repoStore`, `workspaceStore`, `todoStore`, `sendQueue`, `prService`, `reviewService`, `autopilotEngine` (fungsi murni → bisa di-unit-test), `shortcutRegistry` tunggal.
   - ChatPanel benar-benar tetap mounted (`{#each}` + visibility).
   - Tambah event `tool_result` & biaya jika diinginkan.
3. **Platform**: Anda di **WSL2/Linux**. Ganti `zsh` hardcode → `$SHELL`/`sh`, `open` → plugin opener, path data lewat `app_data_dir()` (sudah), traffic-light hanya `cfg(macos)`. Tauri di WSL butuh WSLg + `webkit2gtk-4.1`.
4. **Testing**: unit test Rust untuk parser stream-json, topo-sort combo PR, nama workspace, diff parsing; unit test autopilot engine; Playwright dengan mock fungsi (`page.exposeFunction`).

---

## 8. Roadmap pembangunan (perkiraan untuk 1 developer + Claude Code)

| Fase | Isi | Kriteria selesai | Estimasi |
|---|---|---|---|
| 0. Fondasi | Scaffold Tauri+SvelteKit+Bun, tema/token CSS, `run_cmd`, shell env, persistensi atomik, AppState | app terbuka, `cargo check` & `bun run check` hijau | 2–3 hari |
| 1. MVP orkestrasi | repo add/clone + profil gh, create/remove workspace (worktree), chat agent (stream-json + resume + queue), diff, commit/push/PR/merge, kanban 4 kolom + poll PR | 1 task end-to-end: todo → agent → PR → merged | 1.5–2 minggu |
| 2. Workbench | terminal PTY, script runner, file browser + CodeMirror, search/grep, mention `@file`, gambar, plan/thinking/model, AskUserQuestion, suggested replies, auto-rename, review flow | kerja harian tanpa keluar app | 1.5–2 minggu |
| 3. Integrasi | MCP server Korlap (sidecar) + MCP user + test/OAuth, LSP pool (editor & agent), PR checkout, combo PR, staging | agent bisa pakai LSP & todo tools | 1.5 minggu |
| 4. Otomasi | dependencies/prioritize, autopilot engine + pill orkestrator, dependency graph | autopilot menjalankan 3 todo paralel sampai PR | 1 minggu |
| 5. Knowledge base | build/incremental/after-merge, pre-check invariants, resolusi kontradiksi, UI Knowledge | invariants disuntikkan & dicek sebelum review | 1 minggu |
| 6. Polish | Jira, shortcut lengkap, Codex (opsional), packaging, e2e tests | release build berfungsi di Linux | 1 minggu |

**Total realistis: ~8–10 minggu** untuk paritas fitur; MVP yang sudah berguna ±2.5 minggu.

---

## 9. Keputusan yang sudah diambil

| # | Topik | Keputusan |
|---|---|---|
| 1 | Stack | Daemon Rust `korlapd` + client Tauri v2 + Svelte 5 (GPUI dipertimbangkan, ditolak: pre-1.0, komponen harus dibuat sendiri) |
| 2 | Strategi | Tulis ulang bertahap, menyalin modul yang bagus dari Korlap asli |
| 3 | Platform | macOS, Linux, Windows; WSL = daemon di WSL + UI native Windows |
| 4 | Cakupan | Semua fitur termasuk Codex & Knowledge Base; **Jira → Lark Base** (impor + sinkron status/link PR balik) |
| 5 | Lisensi | Pertahankan notice MIT asli |
