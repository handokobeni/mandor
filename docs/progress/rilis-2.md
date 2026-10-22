# Progres Rilis 2 (AC review)

Plan: `docs/plans/2026-10-21-003-feat-ac-review-plan.md`.

## S0

### Spike A1: `--chrome` bersama `--strict-mcp-config` dan `--mcp-config` (2026-10-21, lulus)

- Lingkungan: Claude Code 2.1.306, daemon di WSL, Chrome harian di Windows dengan ekstensi Claude in Chrome.
- Perintah: `claude -p --chrome --strict-mcp-config --mcp-config <berkas> --model haiku --max-budget-usd 0.5 --output-format stream-json --verbose --allowedTools "mcp__mandor__ledger_ping,mcp__claude-in-chrome__tabs_context_mcp"`, dengan prompt lewat stdin. Berkas MCP berisi satu server stdio dummy (`mandor`, satu tool `ledger_ping`).
- Hasil:
  - Event `init` melaporkan dua server `connected`: `mandor` dan `claude-in-chrome` (keduanya `source: dynamic`). Jumlah tool MCP: 1 milik `mandor` dan 22 milik `claude-in-chrome`.
  - `mcp__mandor__ledger_ping` dipanggil dan mengembalikan `pong`; server dummy mencatat panggilan itu.
  - `mcp__claude-in-chrome__tabs_context_mcp` berhasil dipanggil.
  - Biaya: 0,008 USD.
- Temuan untuk implementasi:
  - Tool MCP bersifat deferred. Agent memanggil `ToolSearch` lebih dulu untuk memuat skema tool, jadi allowlist dan hook mode review (U8) wajib mengizinkan `ToolSearch`.
  - `--allowedTools` menerima banyak nilai dan menelan argumen prompt berikutnya. Prompt harus dikirim lewat stdin, seperti yang sudah dilakukan `mandor-agent`.
  - Uji ini berjalan saat sesi Claude lain (sesi perencanaan ini) juga tersambung ke ekstensi, dan keduanya tetap berfungsi. Ini indikasi awal untuk A5. A5 tetap dibuktikan penuh di U9 dengan dua sesi yang aktif memakai tab bersamaan.

### A3: respons tool Claude in Chrome di `PostToolUse` (2026-10-22, terbukti dengan catatan)

- Lingkungan: Claude Code 2.1.307, `claude -p --chrome --model haiku`, hook `PreToolUse` dan `PostToolUse` perekam, dua server lokal (`localhost:47811` app, `localhost:47812` origin lain). Biaya 0,017 USD. Fixture: `crates/mandor-agent/tests/stream_fixtures/chrome_hooks/{pre,post}.jsonl` (path pribadi disamarkan, data gambar dibuang).
- Input `PreToolUse` memuat `tool_input.tabId`, `url` (navigate), dan `action` (computer).
- `tool_response` adalah daftar bagian konten. Setiap respons tool pada tab memuat bagian "Tab Context" berisi `Executed on tabId: N` dan daftar `• tabId N: "judul" ("url")`; `tabs_create_mcp` memuat `Tab ID: N`. Screenshot menambah bagian gambar (dibuang parser).
- Catatan penting: alamat di respons klik bisa masih alamat sebelum navigasi (klik "Go to page two" melaporkan `index.html`; respons berikutnya baru `page2.html`). Redirect JavaScript tertunda juga baru terlihat di respons tool berikutnya. Karena itu pagar U8 menganggap tab belum terkonfirmasi setelah navigate, klik, atau form_input, dan pembaca halaman menunggu `computer wait` atau `tabs_context_mcp`. Ini penerapan gagal-tertutup KTD-31; satu bacaan masih bisa terjadi bila redirect tertunda terpicu setelah konfirmasi, dan tertangkap oleh post berikutnya (sesuai skenario plan).
- Tool `browser_batch` ada di ekstensi dan disarankan oleh ekstensi itu sendiri; pagar menolaknya karena isinya banyak aksi yang tidak bisa diperiksa satu per satu.

### Smoke test Rilis 1

Persiapan (2026-10-21):
- Repo uji privat `handokobeni/mandor-smoke` (README saja), di-clone ke `~/work-smoke/mandor-smoke`. PR smoke dibuat ke repo ini, bukan ke repo tim.
- Kartu uji `PGS-1510` "TEST Mandor – abaikan" (record `reczz28Q6yTH5xZK`) di Sprint Tasks: To Do, Ready To Dev, assignee dan reporter Beni Handoko. Satu AC (`reczz28Q6yNmRQKM`): README memuat bagian "Smoke test". Dibuat lewat `lark-cli` atas nama pengguna; daemon Mandor tetap memakai app miliknya sendiri.

| Skenario | Status | Catatan |
|---|---|---|
| F1: Start → approve plan → Push & create PR → Waiting Review | lulus | Plan only → approve → PR `handokobeni/mandor-smoke#1` (1 commit di README). Lark: Task Status Waiting Review, `mandor_pr`, `mandor_branch`, `kanban_column`, `mandor_owner`, `repo_id` terisi. Kartu tanpa Sprint tidak tampil di view Lark yang difilter per sprint (bukan bug Mandor). |
| F2: komentar tepercaya + Waiting Fix → perbaikan otomatis → Waiting Review | lulus, dengan 3 temuan | Komentar Beni → Waiting Fix → jeda tenang → agent menambah satu kalimat → commit kedua di PR #1 → Waiting Review; `mandor_comment_cursor` terisi. Temuan di bawah. |
| F2b: Feedback QA per AC + Waiting Fix (R11f, U16 Rilis 1) | lulus (2026-10-22) | Beni mengisi Feedback QA di AC lalu memindahkan task ke Waiting Fix tanpa komentar. Mandor mencatat `fix_requested_by`, mengenali Feedback QA sebagai tepercaya (penulis = pemindah), jeda tenang, agent mengganti judul menjadi "Checklist uji", commit keempat di PR #1, Lark kembali ke Waiting Review. Aturan kepercayaan diperketat setelah review keamanan: Feedback QA hanya dipercaya bila penulisnya juga yang memindahkan ke Waiting Fix. |
| Restart daemon saat run aktif → Failed → Resume | lulus (mati saat publishing) | `kill -9` ke `mandord serve` pukul 23:11:16 saat run `publishing` (pesan commit haiku). Tidak ada push setengah jadi. App menyambung ulang dan menyalakan daemon baru; run menjadi Failed dengan pesan restart, judul tetap PGS-1510. Resume melanjutkan sesi yang sama; karena run berada di loop perbaikan, daemon langsung commit dan push (commit ketiga di PR #1), lalu Lark kembali ke Waiting Review dan worktree bersih. Varian "mati di tengah giliran agent" belum diuji. |
| Dua laptop: klaim, read-only, ambil alih | belum | |
| Windows + WSL dan `tauri build` | belum | |

### Temuan smoke (2026-10-21)

1. **[Diperbaiki 2026-10-21]** **Judul run menjadi id record setelah transisi status.** Header workspace menampilkan `reczz28Q6yTH5xZK` tanpa judul. `runs.list` mengambil key dan judul dari cache task, dan cache saat dicek kemudian sudah benar (PGS-1510, judul lengkap). Dugaan kuat: `write_one` di `mandor-usecase/src/sync/outbox.rs` menulis ke cache record hasil `lark.write`. Respons update Bitable hanya memuat field yang ditulis, jadi `ID` (formula) dan `Task name` kosong sampai poll berikutnya menimpanya. Penyebab terkonfirmasi. Perbaikan: `Task::apply_write` di domain menerapkan tulisan ke record hasil baca segar sebelum tulis, dan hasil gabungan itulah yang disimpan ke cache. `FakeLark` di test kini meniru respons parsial Bitable, dan test `cache_mempertahankan_key_judul_dan_assignee_setelah_outbox_menulis` melindungi perilaku ini. Bug yang sama juga membuat assignee dan AC hilang sementara dari cache.
2. **[Diperbaiki 2026-10-21]** **Dialog Start fix mewajibkan instruksi walau sudah ada komentar tepercaya.** Saat kartu dalam jeda tenang dan sudah ada komentar tepercaya yang tercentang, tombol Start fix tetap ditolak tanpa instruksi. Dev yang menutup dialog lalu melihat perbaikan tetap berjalan, karena jeda tenang habis dan jalur otomatis berjalan sesuai R11c. Perilakunya benar, tetapi membingungkan. Perbaikan: instruksi opsional bila ada komentar tepercaya yang belum terkirim atau komentar yang dicentang (`startFixNeedsInstruction` di `chat.ts`, label berubah menjadi "optional, trusted comments are sent"). Daemon memperlakukan instruksi kosong sebagai tidak ada instruksi dan menolak Start fix tanpa instruksi maupun komentar. Test: dua skenario baru di `tests/waiting_fix.rs` dan satu di `chat.test.ts`.
3. **`git -C <worktree> diff` butuh izin.** Agent mencoba `git -C`, ditolak karena tidak ada di allowlist, lalu memakai `git diff` dari dalam worktree. Tidak memblokir. Bisa ditambah ke allowlist baca git bila `-C` menunjuk ke worktree run.

## Implementasi Rilis 2.0

Branch `feat/rilis2-ac-review`. Satu commit per unit setelah `make check` hijau.

| Unit | Status | Commit | Catatan |
|---|---|---|---|
| U1 Model buku AC dan gerbang | selesai | `6bcc8a0` | `ac_ledger.rs`, `push_gate.rs`; teks AC disimpan utuh (bukan hash) untuk deteksi Changed |
| U2 Store | selesai | `3abbeeb` | Migrasi 0005 diuji pada salinan database Rilis 1 asli; trait port ditambahkan bersama unit pemakainya (U3, U6, U9) |
| U3 Sinkron buku AC | selesai | (lihat log) | `refresh_ledger` membaca ulang task lalu AC; `ledger.get` + event `ledger.updated`; refresh berkala di rekonsiliasi penuh. Waktu sinkron disimpan di memori daemon (dibangun ulang pada refresh berikutnya). |

### A2: edit AC tidak mengubah waktu ubah task (2026-10-22, terbukti)

Deskripsi AC `reczz28Q6yNmRQKM` milik PGS-1510 diubah lewat `lark-cli`; `mandor_modified` task tetap `2026-10-22T10:11:04+07:00` setelah 5 detik. Refresh buku AC terpisah dari poll task (KTD-27) memang diperlukan.
| U4 Snapshot dan penanda usang | selesai | (lihat log) | `snapshot_tree` memakai salinan index di direktori git worktree; verdict ditandai usang setelah giliran yang mengubah kode; snapshot gagal = usang |
| U5 Server MCP ledger | selesai | (lihat log) | `mandord mcp` (stdio) memakai token kapabilitas per giliran dari env berkas mcp-config 0600, bukan argv; koneksi kapabilitas hanya tool `ledger.tool.*`, tanpa event, token diverifikasi ulang per request; connect 5 dtk, panggilan 10 dtk; daemon tak terjangkau = error tool; giliran plan mendapat peran Design, token dicabut saat giliran selesai. Peran Verdict dipakai U8 |
| U6 Sumber desain dan kolom Design | selesai | (lihat log) | Sumber berjenis folder; uji: ada, berupa folder, bukan path state/kredensial (sebelum dan sesudah kanonisasi). Start memeriksa ulang dan menolak dengan nama sumber; pilihan disimpan di `RunExtra.design`. Akar menjadi `--add-dir` + deny Edit/Write, hook `--design` mengkanonisasi path (symlink/`../` keluar ditolak). Token Design hanya diberikan bila run punya sumber; tanpa sumber, prompt tidak meminta Design dan `AcLedgerView.design_sources` kosong ("No design source"). UI di U14 |
| U7 Follow design dan selisih desain | selesai | (lihat log) | `runs.approve_plan` menerima `follow_design` (hanya baris Missing/Partly/Conflicts milik run; lainnya ditolak tanpa mengubah state). AC follow design dikirim ke agent sebagai blok data; ledger disimpan dan `mandor_design_gaps` ditulis lewat outbox `WriteOp::Fields` setelah agent berjalan (null = kosong). Run tanpa sumber desain tidak menulis field. `ensure_app_fields` membuat field teks `mandor_design_gaps` |
| U8 Hook mode review dan pagar browser | selesai | (lihat log) | `mandord hook --review <state> [--post]`; state 0600 + lock 5 dtk, hilang/rusak/terkunci = tolak. Origin ketat (skema, host, port). Tab milik reviewer dicatat dari respons `tabs_create_mcp`. Setelah navigate/klik/form_input tab "belum terkonfirmasi" (ditandai di Pre, gagal-tertutup) dan pembaca halaman/screenshot ditolak sampai `computer wait` atau `tabs_context_mcp` mencatat alamat yang diizinkan. Tool berbahaya (JS, network, console, upload, gif, shortcuts, pindah browser, `browser_batch`, tool tak dikenal) selalu ditolak. Git selain baca ditolak. Fixture A3 direkam dan diputar ulang di `mandor-agent/tests/guardrails.rs` |
| U10 Run app | selesai | (lihat log) | Domain `app_run` (state, `AppSettings` per repo di `settings_json.app`, parser `.env`, penanda manifest/lockfile/skrip). `mandor-proc` `AppProcess`: `sh -c '<cmd>; :' mandor-app-<run>`, log ring 256 KB yang diredaksi, probe port. Usecase: `app_changes` (dibanding snapshot app terakhir atau `origin/<base>`), `app_run` menolak snapshot basi, menghentikan app run lain (StoppedByOtherRun), anggota berurutan (berhenti saat gagal), setup saat hash lockfile berubah, port yang sudah dipakai program lain = Failed; `app_confirm`, `app_stop`, `reap_apps` saat sesi start. State per run di tabel `settings` (`app_run:<run>`, `app_owner`). Adapter proses hidup sepanjang daemon dan dihentikan saat shutdown. Env app = env login bersih (`clean_env`) + kunci `.env` terpilih. `app_run` ditolak saat agent/reviewer bekerja, dan snapshot dicek ulang sebelum setup dan spawn setiap repo. Risiko sisa: snapshot mengikuti `.gitignore`, jadi berkas ignored yang ditulis agent (mis. di `node_modules`) tidak masuk konfirmasi. UI di U13 |
