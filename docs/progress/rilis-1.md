# Progres Rilis 1

Catatan progres eksekusi `docs/plans/2026-10-18-001-feat-korlap-lark-lembur-plan.md`. File rencana tidak diubah untuk mencatat progres.

| Unit | Status | Catatan |
|---|---|---|
| U1 | selesai | Workspace, lint, deny, Makefile, CI, Tauri + SvelteKit 3 terbangun; check/test/build frontend hijau; clippy Tauri hijau; red-check lapisan. `make check` penuh hijau (coverage baris 92%). Red-check masih menunggu. |
| U2 | selesai | `mandor-proc`: run dengan timeout + process group, redaksi token, env bersih, kredensial git via GIT_CONFIG_*, tangkap env login shell, Supervised dengan stop bertahap. 15 test. |
| U3 | selesai | `mandor-store` (sqlx SQLite): repo, record cache, run dengan generation, pesan berpaginasi, outbox, jurnal dua tahap, notifikasi. 10 test. Cache offline `.sqlx` + `make sqlx-check`. |
| U5 | selesai (verifikasi Windows manual) | Tauri: jembatan `mandord connect`/`token` (lokal & WSL), penjaga `wsl.exe` + tray agar daemon hidup saat jendela ditutup; client JSON-RPC TS dengan reconnect/backoff, store koneksi, layar status. 3 test Rust + 6 test Vitest. |
| U6 | selesai | `mandor-git`: default branch dari ref remote, worktree dari origin terbaru dengan rollback, restore worktree, diff name-status+numstat (rename/biner/untracked), commit/push tanpa hook repo dan hanya ke branch task, `gh` token/PR create/view, ringkasan check (CheckRun+StatusContext), deteksi path sensitif. 13 test. |
| U7 | selesai (smoke test Base uji manual) | `mandor-lark`: OAuth PKCE S256 + `state`, tukar kode/refresh berotasi, `user_info` untuk open_id, token di berkas 0600 atomik; Bitable search berhalaman, batch_get per 100, update record, list/create field dengan `client_token`; komentar `file_type=bitable` + balasan lanjutan, dipetakan lewat `extra.notify_extra.record`; validasi opsi SingleSelect; pemetaan error (throttle, konflik tulis, login ulang) + retry backoff/jitter. Tauri `oauth_callback.rs`: callback loopback dengan cek state dan pesan port terpakai. 17 test Lark + 6 test callback. |
| U8 | selesai (simulasi dua laptop terhadap Base uji manual) | Domain: Task Status → kolom (+ "Tidak dikenal"), gerbang readiness, allowlist 3 transisi, klaim `open_id\|ts\|nonce` + heartbeat/stale, revalidasi outbox, penanda kartu, syarat poll komentar. Usecase: `Deps` (Lark, cache, outbox, jam, ID), `SyncService` poll inkremental/penuh, outbox dengan validasi ulang + jeda 600 ms + 3x gagal, klaim tulis→baca ulang 3 dtk, heartbeat, lepas klaim, cermin `kanban_column` hanya oleh pemilik. Adapter: `TaskStore` (cache + outbox, migrasi 0002), `LarkGateway` (pemetaan field, filter Assignee + `mandor_modified`, refresh token atomik, validasi opsi). 18 + 16 + 3 + 7 test. |
| U9 | selesai | Domain: event agent ternormalisasi + klasifikasi hook (git push/commit/remote/credential, gh, host Lark, path kredensial, tulis `.claude/.husky/.githooks/.git`). `mandor-agent`: parser stream-json (multi-event per baris, usage hanya dari `result`, baris rusak dilewati), prompt operator + blok data bernonce, perintah `claude -p` (acceptEdits, `--permission-prompts none`, `--settings` hook absolut, `--setting-sources user,project`, `--strict-mcp-config`, operator prompt di setiap spawn, pesan via stdin), env agent tanpa secret + git/gh terisolasi, `AgentRun` (event bergenerasi, Failed + stderr bila tanpa `result`, stop bertahap). `mandord hook` fail-closed sebelum config/runtime. Diverifikasi dengan Claude Code 2.1.302 nyata: hook `--settings` dihormati, `git push` ditolak walau `disableAllHooks: true` di settings project/local dan saat `--resume`. 22 + 18 + 2 test. |
| U10 | selesai (verifikasi Base uji + UI manual) | Usecase `RunService`: Start KTD-9 (gerbang → klaim → worktree → agent → In Progress + branch + repo_id lewat outbox) dengan rollback worktree/klaim; giliran bergenerasi (event basi diabaikan), antrean pesan, kirim sekarang, stop; publish (commit oleh haiku tanpa tool, push, PR, Waiting Review + kursor); pemeriksaan PR berkala (bukan saat loop perbaikan); rekonsiliasi restart (Failed + reap process group, resume); heartbeat → Orphaned, ambil alih, tutup run; notifikasi antre bila tidak ada client. Adapter: `GitAdapter`, `ClaudeAgent` (+ `reap_group` dengan penanda command line), `RunsStore`/`ReposStore`. Daemon: pengaturan Lark 0600, login PKCE via RPC, sesi + loop berkala, 25 method RPC + event, DTO ts-rs. 11 + 4 + 3 + 2 test. |
| U11 | selesai (skenario reviewer nyata di Base uji manual) | Domain: kursor `(create_time, reply_id)`, penulis tepercaya. Usecase: mesin Waiting Fix (masuk loop, menunggu komentar, jeda tenang dari komentar pertama, jurnal pending→delivered→kursor→status, kirim ulang pending setelah crash, kursor disusulkan, komentar saat agent berjalan jadi giliran berikutnya, sesi baru dengan task+AC+diff PR bila sesi lama tidak ada, worktree dipulihkan, tanpa perubahan/path sensitif ditandai, Mulai perbaikan, Fix Notes fallback dengan konfirmasi). 13 test. |
| U12 | selesai (verifikasi dengan Base uji manual) | Store papan (kolom + "Tidak dikenal", filter sprint, muat ulang pada `board.updated`/`run.updated`/`resync`), badge kartu (nilai ditimpa, gagal sinkron, dihapus/dialihkan, menunggu komentar reviewer, milik dev lain), Start nonaktif dengan alasan, Ambil alih saat klaim tidak aktif, panel agent dengan run menunggu jawaban di atas, notifikasi desktop (event `notice` + `notifications.take` saat tersambung). 7 test Vitest. |
| U13 | selesai (verifikasi dengan Base uji manual) | Workspace: chat dengan VirtualList dan markdown tersanitasi (DOMPurify, hanya http/https/mailto), tool call dikelompokkan dan hasil bisa dibuka, draft per run bertahan, antrean + kirim sekarang + stop, diff bernomor baris, PrBar (Push & buat PR, Lanjutkan, Mulai perbaikan, Tutup run), dialog Mulai perbaikan; pengaturan Lark (simpan tanpa membocorkan secret, login PKCE lewat callback Tauri, logout) dan repo (profil gh, pendamping, reviewer, allowlist tambahan yang dipakai pada spawn berikutnya). 5 test Vitest + 1 test Rust allowlist. |
| U14 | sebagian: kode dan runbook selesai, smoke langsung menunggu Lark App dan Base uji | `crates/mandor-e2e` (F1, F2, restart; `MANDOR_E2E=1`, `--ignored`), `docs/runbook/setup-lark-app.md`, `docs/runbook/setup-wsl.md`. |
| U4 | selesai | `mandor-protocol` (JSON-RPC + ts-rs, test sinkron TS), `mandor-api` (axum WS, hello token+versi, relay stdio), `mandord` (serve/connect/token, lock, daemon.json 0600). 6 + 3 test. |

## Keputusan implementasi
- SvelteKit 3 tidak lagi memakai `svelte.config.js`; konfigurasi dipindah ke plugin `sveltekit()` di `vite.config.ts`.
- TypeScript dipin ke 5.x karena TypeScript 7 (port native) tidak menyediakan `ts.sys` yang dipakai SvelteKit.
- `apps/desktop/src-tauri` adalah workspace Cargo terpisah (di-`exclude` dari root) agar dependensi Tauri tidak masuk gerbang `cargo deny` daemon; lint `rust-engineering` tetap diterapkan di sana.
- `sqlx-cli` dipasang ulang dengan fitur `sqlite` (`cargo install sqlx-cli --no-default-features --features sqlite,rustls`); build normal memakai `SQLX_OFFLINE=true` dari `.cargo/config.toml`.
- Subcommand `mandord token` ditambahkan agar client di Windows bisa memperoleh token `hello` dari daemon di WSL tanpa membaca `daemon.json` lintas sistem berkas; relay tetap tidak menyisipkan token.
- SvelteKit 3 menghapus alias `$lib`; dipakai subpath imports `#lib/*` (package.json `imports`).
- KTD-18: penjaga `wsl.exe -- sleep infinity` selalu dipakai di mode WSL (juga saat systemd aktif) karena cukup untuk menjaga VM tetap hidup; service systemd opsional dicatat di runbook.
- `Cmd::scrub_tokens` hanya membuang variabel warisan; variabel yang disetel eksplisit (mis. `GH_TOKEN` milik daemon untuk `gh`) tetap diteruskan.
- **Deviasi KTD-5:** token Lark disimpan hanya di berkas 0600 (direktori 0700), tidak di keyring OS. Store Secret Service (`zbus`) memasukkan runtime async kedua dan dependensi berstatus unmaintained sehingga gagal `cargo deny` yang diwajibkan `rust-engineering`. Bisa ditinjau ulang bila tersedia store keyring tanpa dependensi itu.
- Callback OAuth diterima oleh client Tauri (tempat browser dibuka, termasuk Windows pada mode WSL), lalu kodenya diteruskan ke daemon; verifier PKCE tetap di daemon.
- Cache task disimpan per tabel Lark (`scope`), bukan per repo (migrasi 0002 mengganti `records`), karena task milik pengguna belum tentu terhubung ke repo.
- Klaim pemilik ditulis sebagai teks `open_id|ts_ms|nonce` di field `mandor_owner` agar domain tidak bergantung pada serde.
- Filter poll inkremental memakai `mandor_modified isGreater ExactDate(since − 1 hari)` karena filter tanggal Lark berbutir hari.
- Fixture `crates/mandor-agent/tests/stream_fixtures/run_full.ndjson` direkam dari Claude Code 2.1.302 (haiku) lalu disanitasi (path, plugin, skill, socket).
- `mandord` kini membangun runtime tokio secara manual; subcommand `hook` berjalan sinkron sebelum `Config::from_env()` agar setiap kegagalan berujung exit 2.
- Env agent juga membuang `ANTHROPIC_API_KEY` agar CLI selalu memakai login langganan, serta `SSH_AUTH_SOCK` dan memaksa `GIT_SSH_COMMAND` tanpa identitas agar remote SSH tidak bisa dipakai push.
- Method RPC berada di `crates/mandord/src/dispatch.rs` (bukan `mandor-api/src/rpc/runs.rs` seperti di daftar berkas rencana) karena `mandor-api` hanya boleh bergantung pada `mandor-protocol` menurut tabel lapisan; `mandor-api` tetap generik lewat trait `Dispatch`.
- Penyusun prompt menjadi port `Prompts` (implementasi `ClaudePrompts` di `mandor-agent`) agar usecase tidak bergantung pada adapter agent.
- `mandord` mematikan semua proses agent saat shutdown (state run tidak diubah; rekonsiliasi saat start menandainya gagal dan bisa dilanjutkan), dengan batas tunggu 30 detik.
- Kursor "sejak sekarang" hanya ditulis pada publish awal; publish hasil perbaikan mempertahankan kursor balasan terakhir yang terkirim agar komentar selama perbaikan tidak terlewat.

U1 kini selesai: `scripts/red-check.sh` (`make red-check`) membuktikan `sqlx`/`reqwest` di domain dan `axum` di usecase membuat gerbang merah; CI menjalankannya beserta cek sinkron tipe protokol dan test Tauri.

## Keterlacakan requirement Rilis 1

| Req | Unit | Bukti test |
|---|---|---|
| R1 | U10, U12 | `RunService::active_runs`; `board.test.ts` panel agent; `start_flow.rs` |
| R2 | U10, U12 | `giliran_selesai_menunggu_dev_dan_notifikasi`, `crash_agent_menandai_gagal_dan_notifikasi`; `DaemonNotifier` antre |
| R3 | U13 | `chat.test.ts` (chat), `runs.changes`/`runs.file_diff` (diff), PrBar (status PR) |
| R4 | U7, U8, U12 | `pencarian_memfilter_assignee_dan_waktu_ubah`; `filter sprint` di `board.test.ts` |
| R5 | U8 | `ae2_judul_dari_pm_muncul_di_cache...`, `outbox_menulis_transisi...` |
| R6 | U8, U12 | `outbox_membatalkan_in_progress_bila_status_sudah_bukan_to_do`; badge "nilai ditimpa" |
| R6a | U8 | `tiga_kali_gagal_menandai_gagal_sinkron...`, `record_dihapus_atau_dialihkan_ditandai_tidak_hilang` |
| R7 | U7, U10 | `ac_dibaca_dari_tabel_tertaut...`, `f1_start_membuat_worktree...` (judul, deskripsi, AC) |
| R8 | U10 | `f1_...` (branch), `publish_commit_push_pr_lalu_waiting_review_dengan_link` |
| R9 | U10 | `f1_...` (In Progress ditulis terakhir) |
| R10 | U10 | `publish_...`, `pr_buatan_luar_app_memicu_waiting_review...` |
| R11 | U11 | `ae1_dua_komentar_tepercaya...` |
| R11a | U11 | `app_mati_saat_waiting_fix_membuat_ulang_worktree_dan_sesi_baru_dengan_konteks` |
| R11b | U11 | `ae7_tanpa_komentar_menunggu_lalu_jeda_tenang_sebelum_agent` |
| R11c | U11 | `ae7_...` (komentar kedua dalam jeda), `komentar_saat_agent_berjalan_dikirim_sebagai_giliran_berikutnya` |
| R11d | U11, U13 | `mulai_perbaikan_dengan_instruksi_dev_tanpa_komentar_tepercaya`; `startFixParams` |
| R11e | U9 | `env_agent_tanpa_token...`, `credential_helper_global_tidak_terlihat...`, hook `mandord` + verifikasi CLI nyata |
| R12 | U11 | `ae1_...` (kembali Waiting Review setelah push) |
| R13 | U8 | `hanya_tiga_transisi_yang_diizinkan`, `outbox_menolak_transisi_di_luar_allowlist_di_tempat` |
| R14 | U6, U10 | `port_git_siklus_worktree_perubahan_commit_push`, `f1_...` |
| R15 | U6, U10 | `publish_...`, `pembuat_pesan_commit_tanpa_tool_dan_tanpa_token` |
| R16 | U5, U2 | daemon/relay/WSL keeper (`daemon_link` test); verifikasi Windows manual |
| R16a | U8, U12 | `ae3_start_ditolak_bila_readiness_review_ac_by_pm`; Start nonaktif dengan alasan |

## Yang masih menunggu verifikasi manual

Butuh Lark Custom App yang disetujui admin, salinan uji Base, repo GitHub uji, dan Windows + WSL:
- Smoke langsung `mandor-e2e` (F1, F2, restart) dan skenario dua laptop.
- Verifikasi Windows + WSL (UI ditutup lebih lama dari `vmIdleTimeout`).
- Asumsi yang belum bisa diuji tanpa app: redirect `http://127.0.0.1:<port>` diterima Custom App, dan scope komentar Bitable tersedia di tenant.
- API run lama di `mandor-store` (`create_run`/`update_run`/`get_run`/`active_run_for`/`list_active_runs`, `journal_known`, `outbox_reschedule`) dihapus karena tergantikan port `RunsStore`/`TaskStore`; `update_run` lama memakai `COALESCE` sehingga `pid` tidak bisa dikosongkan.
- Relay WSL menjalankan `wsl.exe -- sh -lc 'exec mandord <sub>'` agar PATH dari `~/.profile` (mis. `~/.local/bin`) terbaca.
- Atas permintaan pemilik produk, semua teks yang dilihat pengguna memakai bahasa Inggris: UI desktop (tombol, label, pesan sukses dan error), pesan error daemon yang dikirim lewat RPC, alasan Start ditolak, catatan kartu, notifikasi, pesan sistem chat, alasan penolakan hook, halaman callback OAuth, dan menu tray. Komentar kode, log, dan dokumentasi tetap berbahasa Indonesia. `flag_fix` kini menerima state eksplisit, tidak lagi menebak dari isi teks catatan.

## Rename ke Mandor (atas permintaan pemilik produk)

Nama app menjadi **Mandor**. Semua kode dan konfigurasi build dipindah ke folder `mandor/`; perintah `make`, `cargo`, dan `bun` dijalankan dari sana. Dokumen (`docs/`), `CLAUDE.md`, `.github/`, dan `.githooks/` tetap di root.

| Lama (di rencana) | Baru |
|---|---|
| `crates/korlap-*`, `crates/korlapd` | `crates/mandor-*`, `crates/mandord` |
| `apps/desktop` | `apps/desktop` (ID app `dev.mandor.app`) |
| `~/.korlap`, `KORLAP_*` | `~/.mandor`, `MANDOR_*` |
| Field Lark `korlap_branch`, `korlap_pr`, `korlap_owner`, `korlap_comment_cursor`, `korlap_modified` | `mandor_branch`, `mandor_pr`, `mandor_owner`, `mandor_comment_cursor`, `mandor_modified` |
| Prefiks branch task `korlap/` | `mandor/` |

Dokumen rencana dan dokumen analisis lama tidak diubah (catatan historis). Folder referensi `korlap/` dan kredit Korlap asli (NOTICE) tetap.

## UI baru dari handoff desain (2026-10-19)

Sumber: `docs/design/mandor-design/handoff_HANDOFF.md`. Disepakati pemilik produk: state run **Stopped** dan **Discard changes** per berkas masuk Rilis 1; Open in VS Code dan Clear local cache ditunda.

| Langkah | Status | Catatan |
|---|---|---|
| 1. Fondasi shadcn-svelte | Selesai | Tailwind v4, token tema handoff (dark default), font, `theme` store (system/dark/light), 32 komponen shadcn, `scripts/fix-shadcn-imports.ts` |
| 2. Layar inti dengan data yang ada | Selesai | Shell (TitleBar, sidebar Active agents berkelompok), layar koneksi, Board (toolbar sprint/cari/Mine, skeleton, empty state, kolom Done bisa diciutkan, petunjuk tanpa drag), TaskCard, dialog Start/Take over dengan StepProgress, konfirmasi Close run, Workspace (header, action bar per state, callout, tab Chat/Changes/Task/Review comments, panel detail), StartFix, Settings Lark/Repos/General/About |
| 3. Tambahan data daemon | Selesai, kecuali nama orang dan nama sprint (menunggu Lark) | Nama orang, nama sprint, detail task (deskripsi, AC), status kirim komentar dan hitung mundur quiet period, riwayat notifikasi, status sinkron, cek prasyarat, state Stopped, RPC discard berkas, progres langkah Start per langkah |
| 4. Layar fase 2 | Selesai | Onboarding, NotificationCenter, command palette, tray dinamis, halaman callback OAuth |
| 5. Tinjauan visual | Belum | Bersama pemilik produk |

Catatan langkah 2:

- Stylesheet komponen `src/lib/styles/mandor.css` diport dari bagian 2, 4, 5, dan 6 prototipe. Bagian 3 (tiruan shadcn) diganti komponen shadcn-svelte sungguhan.
- Logika tampilan murni ada di `lib/board.ts`, `lib/diff.ts`, `lib/settings.ts`, dan `lib/chat.ts` (`summarizeCalls`). Semuanya ditulis test-first; total test frontend sekarang 35.
- **Bug yang ikut diperbaiki:** di `+page.svelte`, store di-attach di dalam `$effect`. Karena `load()` membaca `board.loading`, effect ikut melacaknya, sehingga setiap kali `loading` berubah semua store di-attach ulang. Akibatnya permintaan `board.list` berulang terus ke daemon. Attach sekarang dibungkus `untrack`.
- VirtualList chat sekarang mengukur tinggi tiap baris (ResizeObserver). Sebelumnya tinggi tetap 72px, padahal blok chat tingginya beragam.
- Progres StepProgress di dialog Start masih kasar: langkah pertama aktif sampai RPC selesai. Progres per langkah butuh event dari daemon (langkah 3).
- Kartu dan workspace menampilkan open_id, bukan nama orang. Tab Task baru memuat field dari kartu, belum deskripsi dan AC. Keduanya butuh data daemon (langkah 3).
- Mode tinjauan tanpa daemon: `bun run dev`, lalu buka `http://localhost:1420/?mock` (`lib/dev/mock-transport.ts`, hanya dimuat di mode dev). Sudah diperiksa bahwa data contoh tidak ikut ke build produksi.
- Diverifikasi di browser: papan, workspace (chat, tool call ditolak, diff), run gagal, dialog Start sampai selesai, Settings Lark, dan tema terang.

Catatan langkah 3:

- **State Stopped (selesai):**
  - Heartbeat klaim sekarang membedakan tiga alasan klaim hilang (`ClaimLoss` di domain). Record yang dihapus atau dialihkan membuat run menjadi `stopped`. Klaim yang diambil dev lain tetap `orphaned`.
  - Ini menutup celah terhadap diagram state di rencana (`Running --> Orphaned: record dihapus / dialihkan`). Sebelumnya, record yang dialihkan sementara klaimnya masih milik kita tidak menghentikan run.
  - Run yang diparkir (`RunState::is_parked`: orphaned atau stopped) menolak pesan, menolak publish, dan tidak dicek PR-nya. Penolakan publish mencegah push ke branch milik task yang sudah bukan milik kita.
- **Discard changes per berkas (selesai):**
  - RPC baru `runs.discard_file {run_id, path}`. Usecase menolak saat agent bekerja, preparing, atau publishing, dan menolak path yang tidak ada di daftar berkas berubah.
  - Adapter mengembalikan berkas ke isi merge-base `origin/<base>`: `checkout`, `rm`, atau `clean`. Untuk rename, path lamanya ikut dipulihkan.
  - UI: tombol "Discard changes" di header diff, dengan konfirmasi.
- **Test flaky yang diperbaiki:** `mandor-agent/tests/run.rs` sesekali gagal dengan `Text file busy` (ETXTBSY). Proses anak dari test lain yang sedang fork masih memegang fd tulis skrip palsu. Helper test sekarang mencoba ulang spawn khusus untuk error itu.
- **Detail task (selesai):** RPC `card.details {record_id}` → `TaskDetailsView`. Isinya deskripsi, status, readiness, assignee, reporter, sprint, dan AC sesuai urutan tautan, termasuk AC yang di-drop. Datanya dari cache record dan port AC yang sama dengan yang dipakai saat Start, jadi tidak ada panggilan Lark jenis baru. Tab Task merender deskripsi sebagai markdown yang disanitasi.
- **Waktu run (selesai):** `RunView.started_at_ms` dan `updated_at_ms` diambil dari kolom tabel `runs` yang sudah ada. Panel detail menampilkan "Started" dan "Last change".
- **Status sinkron (selesai):** `board.list` membawa `synced_at_ms` dan `sync_error` dari poll Lark terakhir. Nilainya disimpan di memori sesi (`SyncStatus`) dan terisi lagi oleh poll berikutnya setelah restart. Toolbar menampilkan "Synced 12s ago" atau "Last sync failed …".
- **Jeda tenang dan status kirim komentar (selesai):** run menyimpan `quiet_until_ms`, yaitu komentar tepercaya pertama yang belum terkirim ditambah durasi jeda. UI menghitung mundur per detik. `CommentView.sent` diambil dari journal `delivered`, dan UI menampilkan "Sent to agent", "Not sent yet", atau "Only shown to you".
- Gerbang terakhir: `make check` hijau (cakupan baris 91,26%), `make red-check` lulus, test frontend 37 lulus, build lulus. Semua fitur di atas sudah dicek di browser lewat `?mock`.

- **Riwayat notifikasi (selesai):**
  - Migrasi `0003` menambah kolom `read`. Setiap notifikasi sekarang masuk riwayat; sebelumnya hanya disimpan saat tidak ada client. Riwayat dipangkas ke 500 entri terbaru.
  - RPC `notifications.list` (50 terbaru) dan `notifications.read {ids | null}`, dengan batas 500 ID per permintaan. Event `notice` membawa `id`.
  - UI: NotificationCenter (bel di title bar, jumlah belum dibaca, tandai semua). Klik notifikasi membuka run-nya.
- **Cek prasyarat (selesai):** RPC `system.check` menjalankan `git --version`, `gh --version`, `gh auth status`, dan `claude --version` secara paralel, masing-masing dengan timeout 5 detik. Hasilnya tampil di Settings › About (System check). Onboarding akan memakai komponen yang sama.
- **Progres Start per langkah (selesai):** usecase `start_with_progress` melaporkan `StartStep` sebelum setiap langkah (klaim, worktree, agent, status Lark). Dispatcher mengirimnya sebagai event `start.progress`, dan dialog Start mengikutinya. Saat gagal, langkah yang sedang aktif ditandai error beserta pesannya.
- Dispatcher dipecah (`start`, `system_call`, `details`) supaya tetap di bawah batas `too_many_lines` dan `cognitive_complexity`.
- Gerbang: `make check` hijau (cakupan baris 91,26%), `make red-check` lulus, test frontend 41 lulus, build lulus.

Sisa langkah 3, menunggu akses Lark atau keputusan pemilik produk:

- Nama orang dari open_id: butuh API Contact Lark dan scope tambahan (mis. `contact:user.base:readonly`) pada Custom App. Sudah ditanyakan ke pemilik produk.
- Nama sprint: bergantung pada cara sprint disimpan di Base; perlu dilihat saat akses Lark tersedia.

Catatan langkah 4:

- **Onboarding:** enam langkah (Welcome, Prerequisites, Lark app, Log in, Repo, Done) dengan stepper.
  - Muncul bila Lark belum diatur, belum login, atau belum ada repo (`onboardingStart`). Langkah awalnya dihitung setelah store Lark selesai dimuat, supaya tidak salah langkah.
  - Bisa dilewati; pilihan itu disimpan per perangkat.
  - `LarkSettings` dipecah menjadi `LarkAppForm` dan `LarkLoginStatus`, dipakai bersama oleh Settings dan onboarding.
- **NotificationCenter:** bel di title bar dengan jumlah belum dibaca, daftar riwayat, dan "Mark all as read". Klik notifikasi membuka run-nya.
- **Command palette (Ctrl/⌘ K):** buka task, Start task yang siap, Go to board, Refresh, Settings, Add a repo. Ada juga pemicu di title bar.
- **Halaman callback OAuth:** desain handoff, dirender di server tanpa skrip (`src-tauri/src/callback.html`). Header CSP `default-src 'none'`, dan teks error dari Lark di-escape. Pesan khusus untuk state mismatch dan akses ditolak.
- **Tray dinamis:** menu berisi Open Mandor, maksimal 5 run yang menunggu Anda, dan Quit. Tooltip menampilkan jumlahnya. Klik run membuka jendela dan workspace-nya (event `tray://open-run`). Command `tray_update` dipanggil UI setiap daftar run berubah.
- **Settings › General:** preferensi notifikasi desktop per jenis (bawaan: semua aktif kecuali "PR created"), disimpan per perangkat. Notification center tetap mencatat semuanya.
- Mode tinjauan: `?mock&setup` menampilkan onboarding (Lark belum diatur, belum ada repo).
- Gerbang: `make check` hijau (cakupan baris 91,26%), `make red-check` lulus. Untuk `src-tauri`: fmt, clippy pedantic, dan 11 test lulus. Frontend: 43 test, check, dan build lulus. Semua layar di atas sudah dicek di browser.

## Audit akhir terhadap Definition of Done (2026-10-19)

Rencana dibaca ulang per heading: Goal Capsule, tabel unit U1–U14, U12–U14, Verification Contract, dan Definition of Done.

| Butir DoD / gate | Status |
|---|---|
| `make check` (fmt, clippy pedantic, deny, test, coverage, sqlx offline) | Hijau, cakupan baris 91,89% (2026-10-21) |
| Pembuktian lapisan (`make red-check`) | Lulus |
| Tipe protokol sinkron (`cargo test -p mandor-protocol`) | Lulus; tipe TS diregenerasi setiap perubahan protokol |
| Cek dan test frontend | Lulus, 73 test (2026-10-21) |
| Build app (`tauri build --no-bundle`) | Lulus di Linux (release) sebelum perubahan 2026-10-21; **perlu diulang** |
| Contract Lark (fixture offline) | Lulus, bagian dari `make check` |
| Kode percobaan dari pendekatan yang ditinggalkan dihapus | Dibersihkan: 12 komponen shadcn yang tidak terpakai (termasuk sidebar dan sheet shadcn yang diganti sidebar Mandor), hook `is-mobile`, `board.loadedAt`, dan paket `@fontsource/space-grotesk` |
| Runbook Lark, WSL, gh | Ada di `docs/runbook/` |
| Smoke langsung F1/F2/restart, dua laptop, Windows+WSL | **Belum.** Lark Custom App dan login sudah berjalan di Base tim; pemilik produk menunda smoke test pada 2026-10-21 dan minta diingatkan. Skenario di U14, termasuk run Plan only dan workspace dua repo |
| Tinjauan visual bersama pemilik produk (langkah 5 handoff desain) | **Menunggu pemilik produk** |
| Nama orang di UI | Selesai di kode (disetujui pemilik produk 2026-10-19); uji langsung menunggu scope Contact disetujui di tenant |
| Nama sprint di UI | Selesai di kode (2026-10-21); uji langsung bersama smoke test |

## Nama orang (Contact API), 2026-10-19

- Pemilik produk menyetujui scope `contact:user.base:readonly`. Scope ditambahkan ke `SCOPES` OAuth (test kontrak memastikan scope ada di URL otorisasi) dan ke runbook.
- `LarkClient::user_names`: `GET /open-apis/contact/v3/users/batch?user_id_type=open_id`, dipecah per 50 ID. Bila `name` kosong, dipakai `en_name`. Ada fixture kontrak `users_batch.json`.
- Port `LarkTasks::user_names` dan `SyncService::user_names` dengan cache di memori. ID yang sudah dikenal tidak diminta ulang. Bila Lark gagal (mis. scope belum disetujui), hanya nama dari cache yang dikembalikan, dan UI memakai open_id.
- RPC `people.names {open_ids}` (maks. 200 ID) → `PersonView[]`.
- UI: store `people` menggabungkan permintaan nama per render ke satu RPC. Nama dipakai di kartu (pemilik klaim), komentar, dialog Start fix, panel detail (assignee, reporter, pemegang klaim), title bar, dan footer sidebar.
- Gerbang: `make check` hijau (cakupan baris 91,17%), `red-check` lulus, test frontend 44, build lulus.

## Nama sprint, 2026-10-21

- Skema Base Pegasus dibaca (hanya baca) lewat akses Lark: field `Sprint` di Sprint Tasks adalah SingleLink ke tabel **Sprint Status** (`tbla9l5M554MIG6f`). Tabel itu berisi `Sprint` (nama, primary), `Sprint Status`, `Start Date`, dan `End Date`.
- Tidak ada pengaturan baru: gateway membaca `table_id` tujuan dari properti field link (skema di-cache per sesi), lalu mengambil record sprint dengan `batch_get` (`to_sprint`). Test gateway memakai bentuk respons asli.
- Port `LarkTasks::sprints` dan `SyncService::sprints`, dengan cache di memori dan tanpa permintaan ulang. Bila Lark gagal, hanya sprint dari cache yang dikembalikan.
- `board.list` membawa `sprints: SprintView[]`. UI: filter sprint menampilkan "Sprint 42 · Oct 13 to Oct 24" dengan sprint terbaru di atas, dan panel detail menampilkan nama sprint. Sprint tanpa data memakai ID-nya.
- **Keamanan dependensi:** `cargo deny` menemukan advisory baru RUSTSEC-2026-0382 (`rustls-webpki` 0.103.15) dan RUSTSEC-2026-0383 (`webpki-root-certs` 1.0.9) lewat `reqwest`. Keduanya dinaikkan ke versi patch (0.103.16 dan 1.0.10) dengan `cargo update`.
- Gerbang: `make check` hijau (cakupan baris 91,21%), test frontend 46, build lulus.

## Perbaikan dari uji langsung pertama, 2026-10-21

Uji langsung pertama dilakukan pemilik produk dengan Base Pegasus. Gejalanya: kartu yang dipindah di Lark tidak ikut berubah di Mandor. Log daemon menunjukkan tiga masalah:

- **Field milik app gagal dibuat** (`1254037 Invalid client token`). Lark mewajibkan `client_token` pembuatan field berformat UUIDv4, sedangkan Mandor mengirim `mandor-<tabel>-<nama>`. Test kontrak lama justru mengunci format yang salah. Perbaikan: `field_client_token` membuat UUIDv4 deterministik (SHA-256 dari tabel dan nama field) supaya tetap idempoten. Cache skema dikosongkan setelah bootstrap membuat field.
- **Poll inkremental selalu ditolak** (`1254018 InvalidFilter`), karena memfilter `mandor_modified` yang belum ada. Akibatnya hanya rekonsiliasi penuh (tiap 5 menit) yang jalan. Perbaikan: `search_assigned` memakai filter waktu-ubah hanya bila field itu ada di skema; tanpa field itu, pencarian penuh tetap benar.
- **UI menampilkan "Synced 1m ago" padahal poll gagal**, karena status sinkron hanya ikut terbaca saat papan dimuat ulang. Perbaikan: event baru `sync.status` dikirim setiap poll selesai, dan store papan memperbaruinya tanpa memuat ulang.
- **Permintaan nama orang dan sprint membanjiri Lark** saat scope Contact belum disetujui (`99991679`). Perbaikan: setelah gagal, `SyncService` tidak mencoba lagi selama 5 menit.
- UI: tombol Reset di Settings › Lark nonaktif bila form belum berubah (`larkFormDirty`).
- Diverifikasi di daemon nyata: setelah restart, tidak ada lagi `InvalidFilter` maupun error bootstrap.
- Gerbang: `make check` hijau (cakupan baris 91,18%), test frontend 48.

## Perubahan cakupan: memilih repo tanpa mengetik path, 2026-10-21

**Keputusan pemilik produk (2026-10-21):** masuk Rilis 1, memakai opsi 1 (saran repo otomatis) dan opsi 2 (penjelajah folder dari daemon). Sebelumnya fitur ini tidak ada di rencana; "file browser" di Deferred adalah workbench Korlap, bukan pemilih repo.

- **Alasan bukan dialog folder bawaan sistem:** repo harus berada di mesin daemon. Di Windows, app berjalan di Windows sedangkan repo ada di WSL, jadi dialog bawaan akan menghasilkan path Windows.
- **`mandor-git::discover_repos`:** memindai `Config.user_home` sampai kedalaman 3 dengan `tokio::fs` (async), timeout 5 detik, maksimal 50 hasil. Yang dilewati: folder tersembunyi (termasuk `~/.mandor`), `node_modules`, `target`, `vendor`, `dist`, `build`, isi repo, worktree (`.git` berupa berkas), dan repo tanpa `origin`. URL `origin` dibaca dari `.git/config`.
- **`mandor-git::list_dirs`:** subfolder terlihat (maksimal 500) dengan penanda repo; hanya path absolut.
- **RPC** `repos.discover` (menandai repo yang sudah ditambahkan) dan `fs.list_dirs {path?}` (kosong = home). Keduanya tidak butuh sesi Lark dan diuji di test integrasi daemon dengan `HOME` sementara.
- **`Config.user_home`:** dibaca sekali di `Config::from_env` (`HOME`/`USERPROFILE`).
- **UI `AddRepoDialog`:** berisi daftar "Found on this machine" (klik untuk memilih) dan **Browse…** (breadcrumb, `..`, repo ditandai "git repo", "Use this folder"). Path tetap bisa diketik. Dialog dipakai di Settings › Repos dan di onboarding.
- Gerbang: `make check` hijau (cakupan baris 91,24%), test frontend 50, dicek di browser lewat `?mock`.

## Usulan workspace multi-repo, 2026-10-21

- Kebutuhan: satu kartu mengubah beberapa repo terpisah (mis. `kelola-v2` = `kelola-api` + `kelola-web` + `kelola-app`). Folder induk bukan repo git dan tidak akan dijadikan monorepo.
- Keputusan pemilik produk: Waiting Review setelah **semua** PR dibuat, dan MCP server dari folder induk ikut dibawa.
- Usulan lengkap: `docs/plans/2026-10-21-002-multi-repo-workspace-proposal.md`. Status: **disetujui 2026-10-21, sudah dikerjakan** (bagian berikut).

## Workspace multi-repo dikerjakan, 2026-10-21

Disetujui pemilik produk ("ya setuju, kerjakan"), dengan batasan MCP: hanya server yang dicentang per workspace.

- **Store:** migrasi `0004_workspaces.sql` (`workspaces`, `workspace_repos`). Run menyimpan `workspace` (id, folder run) dan `extra` (repo tambahan beserta worktree dan link PR) di JSON `RunExtra`, jadi tabel `runs` tidak berubah.
- **Git:** worktree run workspace di `<workspaces>/<workspace>-<branch>/<repo>`, satu branch yang sama untuk semua repo. `remove_run_root` hanya menghapus folder di bawah direktori worktree daemon.
- **Usecase:** `StartTarget::{Repo, Workspace}`. Start memvalidasi target (repo harus anggota workspace), membuat worktree per repo, dan me-rollback semuanya bila satu gagal. Satu modul `run/members.rs` dipakai Start, giliran lanjutan, perbaikan Waiting Fix, publish, tutup run, dan discard berkas:
  - worktree anggota yang hilang dibuat ulang di folder run yang sama;
  - publish: commit dan push per repo yang berubah, satu PR per repo; repo tanpa perubahan dan tanpa commit di branch task dilewati; link PR disimpan per repo segera, jadi kegagalan sebagian bisa dilanjutkan tanpa PR ganda;
  - **Waiting Review hanya setelah semua PR dibuat**; `mandor_pr` berisi semua link satu per baris;
  - perbaikan Waiting Fix memeriksa perubahan dan path sensitif di semua repo;
  - pemeriksaan PR berkala melewati run workspace (Waiting Review hanya lewat publish);
  - tutup run menghapus worktree yang bersih; folder run dihapus hanya bila semua worktree terhapus;
  - workspace yang dihapus saat run berjalan: run lanjut tanpa MCP.
- **Agent:** cwd = folder run; `CLAUDE.md` folder induk disalin; `.mcp.json` disaring ke server terpilih dan ditulis di `~/.mandor/agent/mcp` (0600), di luar folder kerja agent; tetap `--strict-mcp-config`; allowlist `mcp__<server>`; hook `mandord hook --mcp a,b` menolak tool MCP lain. `.claude/settings.local.json` tidak dibawa.
- **Lark:** field URL `mandor_pr` ditulis dengan teks = semua link, link = PR pertama; dibaca kembali dari teksnya bila berisi URL.
- **Protokol:** `workspaces.list/add/update/remove/mcp_servers` (hanya nama server, tanpa isi konfigurasi); `StartParams.workspace`; `RunView.workspace_id` dan `repos`; `runs.changes`/`runs.file_diff` memakai awalan `<repo>/` untuk run workspace. Penambahan workspace memvalidasi folder induk, repo terdaftar, dan server MCP yang ada di `.mcp.json`.
- **UI:** Settings › Workspaces (tambah lewat penjelajah folder, repo di bawah folder induk tercentang otomatis, centang MCP server dengan peringatan); dialog Start memilih "One repo" atau "Workspace" dengan centang repo; tab Changes dikelompokkan per repo; kartu dan header run menampilkan semua PR (`kelola-api#1`, `kelola-web#2`). Penjelajah folder diekstrak menjadi `FolderBrowser` dan dipakai Add repo juga.
- Gerbang: `make check` hijau (cakupan baris 91,62%), `make red-check` lulus, test frontend 60, `bun run check` dan `bun run build` bersih; dicek di browser lewat `?mock`.
- **Belum diuji langsung** dengan repo `kelola-v2` nyata dan GitHub; perlu smoke test oleh pemilik produk.

## Perbaikan dari uji langsung workspace, 2026-10-21

- **Agent ditolak membaca folder kerjanya sendiri.** Pagar agent (deny `Read(~/.mandor/**)` dan penanda `.mandor` di hook) melindungi token di `~/.mandor`, padahal worktree juga diletakkan di `~/.mandor/workspaces`. Berlaku untuk run satu repo maupun workspace. Perbaikan: worktree dipindah ke `~/.local/share/mandor/workspaces` (`Config.workspaces_dir`), sehingga larangan menyeluruh atas `.mandor` tetap utuh. `MANDOR_HOME` eksplisit (test, e2e) tetap memakai `<MANDOR_HOME>/workspaces` bila lolos aturan hook yang sama; test config memeriksanya lewat `classify`. Run lama di lokasi lama harus ditutup dan dimulai ulang.
- **Add repo pada folder induk** memberi pesan menyesatkan tentang `origin/HEAD`. Sekarang `GitError::NotARepo` mengarahkan ke Settings › Workspaces.
- **UI:** nama tool MCP yang panjang menimpa kolom pratinjau. Sekarang diringkas (`graft · find_code`, nama lengkap di tooltip) dan kolomnya menyesuaikan lebar.
- Gerbang: `make check` hijau, test frontend 61.
- **Nama akun sendiri tampil sebagai open_id.** Contact API `users/batch` ternyata butuh scope `contact:contact.base:readonly` (bukan hanya `contact:user.base:readonly`), sehingga ditolak (`99991679`). Nama akun yang login kini diambil dari `authen/v1/user_info` saat login (tanpa scope baru), disimpan di token, dikirim di `LarkStatus.name`, dan dipakai sidebar serta kartu milik dev. Token lama perlu login ulang sekali. Nama orang lain tetap butuh scope tambahan itu (menunggu keputusan pemilik produk).
- Pemilik produk menambahkan permission `contact:contact.base:readonly` di Custom App (2026-10-21). Scope itu kini ikut diminta saat login (`SCOPES`, test kontrak, runbook); login ulang diperlukan agar token membawanya.
- **Perintah Bash gabungan ditolak** ("no approval surface"). Claude memeriksa setiap bagian perintah gabungan terhadap allowlist, sehingga `cd x && ...`, `...; echo`, `sed -i`, `pwd`, dan penugasan variabel ditolak; paling terasa di run workspace. Allowlist bawaan ditambah perintah shell tanpa efek di luar worktree (`cd`, `pwd`, `echo`, `printf`, `sed`, `awk`, `sort`, `uniq`, `cut`, `tr`, `diff`, `test`, `true`, `which`, `basename`, `dirname`, `jq`); hook tetap memeriksa seluruh teks perintah. Operator prompt kini menjelaskan aturan ini (tanpa variabel shell, tulis path lengkap). Berlaku untuk giliran agent berikutnya, termasuk run yang sedang terbuka.

## Fase plan sebelum implementasi, 2026-10-21

**Keputusan pemilik produk (2026-10-21):** Start bisa diawali fase plan. Plan menunggu persetujuan dev, plan hanya disimpan di Mandor (tidak ikut commit), dan skill planning dipilih di dialog Start setiap kali (tanpa nilai bawaan per repo).

- **Usecase:** `Planning::{Skip, Builtin, Skill}` dan `start_planned`. Selain `Skip`, run masuk `RunPhase::Plan`; setiap giliran di fase itu memakai mode plan, termasuk revisi lewat chat. Pesan akhir agent disimpan sebagai `run.plan`, notifikasi "Plan ready". `approve_plan` memindahkan ke `RunPhase::Implement` dan melanjutkan sesi yang sama dengan pesan persetujuan. Publish ditolak (`plan not approved`) selama fase plan; giliran Waiting Fix selalu implementasi.
- **Agent:** giliran plan memakai `--permission-mode plan`, `ExitPlanMode` ditolak (plan ditulis di pesan akhir), dan hook `--plan` menolak semua tool penulis berkas (`classify_planning`). Tool `Skill` masuk allowlist bawaan; tool yang dipakai skill tetap melewati allowlist dan hook. Pesan plan meminta pendekatan, berkas per repo, cara memenuhi dan menguji tiap AC, risiko, dan pertanyaan terbuka; skill yang ingin menulis berkas atau bertanya diarahkan ke pesan akhir.
- **Daftar skill:** `available_skills` membaca plugin yang aktif (`~/.claude/settings.json` + `installed_plugins.json`) dan `~/.claude/skills`, hanya nama dan deskripsi, dengan batas waktu 5 detik. Skill bertema plan di urutan pertama.
- **Protokol:** `StartParams.planning`, `RunView.phase`/`plan`, `runs.approve_plan`, `agent.skills`. Store menyimpan fase dan plan di `RunExtra`.
- **UI:** dropdown Planning wajib dipilih di dialog Start (bawaan Mandor, skill plugin, atau lewati); label "Planning"/"Plan ready" di kartu, sidebar, dan header; tab Plan (Markdown tersanitasi) terbuka otomatis; tombol "Approve plan & implement" menggantikan Push & PR selama fase plan.
- Gerbang: `make check` hijau (cakupan baris 91,80%), test frontend 64; dicek di browser lewat `?mock`.
- **Brainstorm sebelum plan (keputusan pemilik produk 2026-10-21):** pilihan Start menjadi "Brainstorm, then plan" (bawaan), "Plan only", atau "Skip planning", masing-masing tahap dengan skill sendiri (prompt bawaan Mandor atau skill plugin apa pun yang aktif). Pilihan terakhir diingat per mesin (`localStorage`, `#lib/planning.ts`); skill yang tidak terpasang lagi kembali ke bawaan.
  - Usecase: `Planning::{Skip, Plan { skill }, Brainstorm { brainstorm, plan }}`, `RunPhase::Brainstorm`, `run.plan_skill`, `continue_to_plan`. Brainstorm hanya baca; pesan akhirnya tidak disimpan sebagai plan, jadi Approve baru tersedia setelah tahap plan.
  - Prompt brainstorm bawaan: baca kode dulu, tanya satu hal per pesan dengan usulan jawaban, fokus pada yang tidak terjawab kode/AC, tutup dengan ringkasan requirements. Tanpa plugin, kedua tahap memakai prompt bawaan.
  - Store menyimpan fase sebagai teks (`brainstorm`/`plan`) dan tetap membaca bentuk lama. Protokol: `StartPlanning { kind, brainstorm_skill, plan_skill }`, `runs.continue_to_plan`.
  - UI: `PlanningPicker` di dialog Start; label "Brainstorming"/"Question for you"; tombol "Continue to plan" selama brainstorm.
  - Gerbang: `make check` hijau (cakupan baris 91,78%), test frontend 66; dicek lewat `?mock`.
- UI: pilihan aktif di toggle group kini jelas (latar dan garis warna primary; sebelumnya `bg-muted` nyaris tak terlihat di tema gelap), termasuk "One repo | Workspace" dan "Mine | All assigned". Dropdown "Brainstorm with" dan "Plan with" disusun atas-bawah dengan elipsis untuk nama skill panjang.
- **Tab Plan sebagai dokumen Markdown:** komponen `MarkdownDoc` dengan gaya `.prose` (hierarki judul, tabel bergaris dengan header dan baris belang serta gulir horizontal, blok kode, kutipan, garis, lebar baca 780px) dan tombol "Copy Markdown". Checklist GFM dirender sebagai span kotak centang karena sanitizer tetap membuang `<input>`. Prompt plan kini meminta dokumen Markdown berstruktur: satu judul `#`, bagian `##`, tabel pemetaan AC, langkah berupa checklist `- [ ]`.
- **Chat tidak lagi menampilkan prompt mentah agent.** Pesan pertama di chat kini ringkasan Start (`start_summary`: mode, skill tiap tahap, catatan dev), sedangkan agent tetap menerima prompt lengkap dengan blok `MANDOR_DATA` (pagar KTD-15 tidak berubah). Run lama yang menyimpan prompt mentah diringkas di UI (`devText` di `chat.ts`).
- **Perintah `/skill` di chat (permintaan pemilik produk 2026-10-21):** mengetik `/` di awal pesan menampilkan skill plugin Claude dari daemon (`agent.skills`) dengan pencarian, ↑↓, Enter/Tab, Esc (`#lib/commands.ts`, `stores/skills.svelte.ts`). Daemon mengenali `/nama argumen` (`mandor_domain::run::slash_command`; path seperti `/home/...` bukan perintah) di setiap giliran lanjutan, termasuk antrean, dan mengirim instruksi eksplisit menjalankan skill itu lewat tool `Skill` (`Prompts::command_message`); chat tetap menyimpan teks asli. Fase brainstorm/plan tetap hanya baca. Subagent plugin belum didukung (tool `Task` tidak di allowlist).
- **Toggle "Mine | All assigned" tampak tidak berfungsi.** "Mine" hanya menyembunyikan kartu yang diklaim dev lain lewat Mandor; di Base saat ini tidak ada satu pun (0 dari 394), sehingga kedua tampilan sama. Keputusan pemilik produk (2026-10-21): **Mine** = kartu yang assignee-nya hanya dev ini, ditambah yang sedang dikerjakan dev ini (klaim sendiri atau run di mesin ini); **All assigned** = semua kartu yang mencantumkan dev sebagai assignee. `CardView.assignees` ditambahkan; filter di `board.ts` (`isMine`).
- **Papan Team (keputusan pemilik produk 2026-10-21, mengubah KTD-6):** toggle menjadi **Mine | Team**. Team berisi kartu seluruh tim: semua yang belum Done ditambah Done (Waiting Deploy/Delivered) yang berubah dalam 14 hari terakhir. Kartu milik dev tetap disinkron tiap 15 detik lewat cache seperti KTD-6; kartu tim diambil terpisah tiap 60 detik (dan saat Refresh) lewat `LarkTasks::search_team` (dua pencarian: `Task Status isNot` Done, dan `mandor_modified` 14 hari, digabung tanpa duplikat), disimpan **di memori** `SyncService` (tidak masuk cache, sehingga klaim, komentar, dan rekonsiliasi tidak tersentuh) dan dikirim di `board.list` dengan `CardView.team`. Kartu rekan hanya baca: tanpa Start atau ambil alih, menampilkan nama assignee. Mine menjadi tampilan bawaan. `board.updated` dikirim hanya bila isi papan Team berubah.
- **Perpindahan Mine/Team lambat** karena papan Team merender sekitar 1.000 kartu sekaligus. Sekarang tiap kolom merender 40 kartu dulu dengan tombol "Show more"; selama perpindahan papan diredupkan dan toolbar menampilkan "Loading team cards…". Kolom Unknown disembunyikan di Team. Nama PIC (assignee) tampil di semua kartu, termasuk kartu milik dev sendiri.

## Dokumen rencana diamandemen, 2026-10-21

- `docs/plans/2026-10-18-001-feat-korlap-lark-lembur-plan.md` kini memuat R3a (brainstorm/plan), R3b (`/skill`), R4a (Mine/Team), R14a (workspace multi-repo), R14b (pemilihan repo), R8 untuk banyak PR, catatan amandemen pada KTD-6 dan KTD-10, keputusan baru KTD-21 sampai KTD-24, dan unit U15 yang memetakan semua itu ke test. Skenario F1 di U14 ditambah run Plan only dan workspace dua repo.
- Smoke test U14 ditunda atas keputusan pemilik produk; belum ada commit pertama di repo `mandor/`.

## 2026-10-22: dokumen pindah ke repo `mandor`

`docs/` dan `CLAUDE.md` dipindah dari folder induk `korlap-clone` (yang belum punya commit) ke repo `mandor`, agar rencana, progres, desain, dan aturan proyek ter-versi bersama kode. Path di dokumen yang dulu ditulis relatif terhadap `korlap-clone` (`mandor/crates/...`, `mandor/apps/...`) diubah menjadi relatif terhadap root repo (`crates/...`, `apps/...`). Catatan sejarah di atas tetap memakai tata letak lama. Folder induk menyimpan `CLAUDE.md` pendek yang mengimpor `mandor/CLAUDE.md`, agar sesi yang dibuka dari `korlap-clone` tetap memuat aturan yang sama.
