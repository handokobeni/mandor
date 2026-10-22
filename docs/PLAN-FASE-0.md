# Rencana Arsitektur & Fase 0 — Korlap Clone

Keputusan: daemon Rust headless **`korlapd`** + client **Tauri v2 + Svelte 5**, multi-platform (macOS, Linux, Windows, WSL), tulis ulang bertahap, Jira diganti **Lark Base** dua arah. Referensi perilaku: [`ANALISIS-KORLAP.md`](ANALISIS-KORLAP.md) dan `spec/`.

---

## 1. Topologi

```
                ┌──────────────────────── korlapd (Rust, tokio) ─────────────────────────┐
                │ api (WebSocket JSON-RPC 2.0 + bearer token)                              │
 Client Tauri ◄─┤ services: repos · workspaces · agents · git · pr · terminals · scripts   │
 (Svelte 5)     │           files · lsp · mcp-bridge · kb · autopilot · lark              │
                │ store: SQLite (sqlx) · event bus (tokio::broadcast) · job supervisor     │
                └─────────┬───────────────┬───────────────┬───────────────┬───────────────┘
                          │spawn          │spawn          │stdio          │HTTPS
                       claude/codex     git/gh        LSP servers     Lark OpenAPI
```

**Mode operasi**

| Platform | Daemon | UI | Transport |
|---|---|---|---|
| macOS / Linux | proses lokal, di-spawn otomatis oleh client (atau systemd/launchd user service) | Tauri native | Unix socket / `ws://127.0.0.1` |
| Windows + WSL | `korlapd` di dalam distro WSL | Tauri native Windows | `ws://localhost:<port>` (WSL2 localhost forwarding) |
| Windows native | `korlapd.exe` | Tauri | `ws://127.0.0.1` |
| Remote (kelak) | server Linux | Tauri | `wss://` via SSH tunnel |

Client menemukan daemon lewat file `~/.korlap/daemon.json` (`{port, token, pid, version}`); di Windows+WSL client menjalankan `wsl.exe -d <distro> -- korlapd --print-endpoint` untuk mem-boot & membaca endpoint.

---

## 2. Struktur repo (Cargo workspace + Bun)

```
korlap-clone/
├─ crates/
│  ├─ korlap-protocol/   # tipe request/response/event (serde) + ts-rs → generate TS
│  ├─ korlap-core/       # domain: model, service traits, state machine workspace/PR, autopilot engine (murni, testable)
│  ├─ korlap-proc/       # run_cmd (timeout, env shell, redaksi token, process group, kill tree) — lintas OS
│  ├─ korlap-git/        # worktree, diff, fetch/merge, auth injection via GIT_CONFIG_COUNT/KEY/VALUE
│  ├─ korlap-agent/      # AgentBackend (Claude, Codex), parser stream-json → Vec<AgentEvent>, prompts/
│  ├─ korlap-pty/        # portable-pty, multiplex per terminal
│  ├─ korlap-lsp/        # pool, JSON-RPC framing, UTF-16 aware edits, clamp ke worktree
│  ├─ korlap-lark/       # client OpenAPI Lark Base (auth, records search/update, field mapping)
│  ├─ korlap-store/      # SQLite migrations + repo pattern
│  ├─ korlapd/           # binary daemon: api ws, wiring services, supervisor
│  └─ korlap-mcp/        # binary MCP stdio server (rmcp) → panggil daemon; dibundel, bukan bun script
├─ apps/
│  └─ desktop/           # Tauri v2 + SvelteKit (adapter-static) — client tipis
│     ├─ src-tauri/      # hanya: lifecycle daemon, discovery, window, deep-link
│     └─ src/lib/{api,stores,services,components}
├─ docs/
└─ Cargo.toml / package.json (bun workspaces)
```

Prinsip: **Tauri tidak berisi logika bisnis.** Semua command di Korlap asli pindah ke daemon; client bicara ke daemon via WebSocket (bukan `invoke`). Stream (chat agent, PTY, script) = notifikasi JSON-RPC per subscription id; PTY bytes dikirim sebagai frame biner WS.

---

## 3. Protokol (ringkas)

- JSON-RPC 2.0 di atas WebSocket; handshake `hello {client_version, token}` → `{daemon_version, capabilities}`.
- Namespace method: `repo.*`, `workspace.*`, `agent.*`, `git.*`, `pr.*`, `term.*`, `script.*`, `fs.*`, `search.*`, `lsp.*`, `kb.*`, `autopilot.*`, `lark.*`, `settings.*`.
- Event (server → client): `workspace.updated`, `agent.status`, `agent.event{ws, run_id, event}`, `todo.changed`, `lsp.status`, `pr.status`, `job.progress`, `lark.sync`.
- Semua tipe di `korlap-protocol`, TS di-generate (`ts-rs`) → tak ada drift tipe Rust/TS.
- Setiap agent run punya `run_id` (generation) → menghapus race stop→send Korlap asli.

---

## 4. Penyimpanan (SQLite, satu file `~/.korlap/korlap.db`)

Tabel inti: `repos`, `repo_settings`, `workspaces`, `agent_sessions`, `messages` (per baris, bukan satu JSON besar), `todos`, `todo_deps`, `pr_cache`, `kb_meta`, `lark_bindings`, `lark_sync_log`. Gambar & worktree tetap di filesystem (`~/.korlap/images`, `~/.korlap/workspaces`). Migrasi via `sqlx migrate`.

---

## 5. Integrasi Lark Base (pengganti Jira)

**Auth**: Custom App Lark (app_id + app_secret) → `tenant_access_token` (`POST /open-apis/auth/v3/tenant_access_token/internal`), opsional OAuth user token bila perlu bertindak atas nama user. Domain dapat dikonfigurasi (`open.larksuite.com` / `open.feishu.cn`). Secret disimpan di keychain OS (crate `keyring`), bukan di DB.

**Binding per repo** (`lark_bindings`):
```
app_token, table_id, view_id?,
filter (mis. Status = "To Do" AND Assignee = me),
field_map: { title, description, status, assignee?, pr_link, branch?, key? },
status_map: { todo: "To Do", in_progress: "In Progress", review: "In Review", done: "Done" }
```

**Alur**
1. **Impor**: `records/search` (paginasi, filter) → popover pilih → jadi todo dengan `lark_record_id`. Rich text Lark diratakan ke markdown.
2. **Sinkron balik** (dipicu event domain, bukan polling):
   - todo → workspace dibuat ⇒ status `in_progress`, isi `branch`
   - PR dibuat ⇒ status `review`, isi `pr_link`
   - PR merged ⇒ status `done`
   - via `PUT records/{record_id}`; gagal ⇒ retry dengan backoff, dicatat di `lark_sync_log`, tampil di UI.
3. **(Opsional) tarik perubahan**: polling ringan tiap N menit untuk record yang di-bind (judul/deskripsi berubah).

UI: halaman Settings → "Lark" (kredensial, pilih Base/Table/View, mapping field & status dengan dropdown dari `fields.list`), tombol "Import from Lark" di kanban, badge sync di kartu.

---

## 6. Fase 0 — Fondasi (target ±1 minggu)

| # | Tugas | Kriteria selesai |
|---|---|---|
| 0.1 | Scaffold Cargo workspace + bun workspace + CI (fmt, clippy, test di Linux/macOS/Windows) | CI hijau di 3 OS |
| 0.2 | `korlap-proc::run_cmd` (timeout, env shell login, redaksi token, process group / Job Object di Windows, kill tree) | unit test timeout & kill tree |
| 0.3 | `korlap-store` + migrasi awal (repos, workspaces, settings) | test migrasi |
| 0.4 | `korlap-protocol` + generate TS | `bun run check` membaca tipe hasil generate |
| 0.5 | `korlapd`: WS server, handshake token, `daemon.json`, `repo.add/list/remove`, event bus | `websocat` bisa memanggil `repo.list` |
| 0.6 | Client Tauri: discovery/spawn daemon (lokal & WSL), koneksi WS + reconnect, store repo, Home screen, tema/token CSS | tambah repo dari UI di Linux & dari Windows → daemon WSL |
| 0.7 | Port helper Korlap asli yang bagus: detect default branch, nama `adj-noun`, shell env capture (bash/zsh/fish) | unit test |

**Fase 1 (MVP)** menyusul: workspace/worktree, agent chat (Claude) + queue, diff, commit/push/PR/merge, kanban + poll PR — lalu Fase 2–6 sesuai roadmap di `ANALISIS-KORLAP.md §8`, dengan **Lark** masuk di Fase 3 (bersama integrasi lain).

---

## 7. Risiko & mitigasi

| Risiko | Mitigasi |
|---|---|
| WebKitGTK di Linux lambat untuk chat panjang | VirtualScroller + SvelteMap (pola Korlap), batasi re-render markdown |
| Jaringan Windows↔WSL (mirrored vs NAT) | default localhost forwarding; fallback `wsl.exe` relay via stdio |
| Path Windows vs Linux | semua path milik daemon; client hanya menampilkan |
| Perbedaan shell (zsh/bash/fish/pwsh) | `run_cmd` tidak bergantung zsh; deteksi `$SHELL` |
| Flag CLI `claude` berubah | backend agent terisolasi + test parser dengan fixture NDJSON nyata |
| Rate limit / izin Lark | backoff, log sinkron, validasi scope saat setup |
