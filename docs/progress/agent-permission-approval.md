# Progres: Izin agent tanpa penolakan diam-diam

Plan: `docs/plans/2026-11-06-001-feat-agent-permission-approval-plan.md`. Branch `feat/agent-permission-approval`, PR #21 (belum di-merge).

| Unit | Status | Catatan |
|---|---|---|
| U1 | selesai | `mandor-domain/src/permit/shell.rs`: pengurai ketat menjadi perintah sederhana (kata + redirect) atau Unparseable untuk substitusi, pembungkus (`eval`, `sh -c`, `xargs`, `env`, …), here-doc, assignment di depan, ekspansi di luar kutip tunggal, glob, brace, tilde di awal kata atau setelah `=`/`:`. `allowlist.rs`: pencocok `Bash(…)` per kata (kutipan tidak bisa menyamarkan batas kata; `Bash(cd *)` butuh argumen; `Bash(git diff:*)` batas kata, `Bash(git diff*)` awalan kata terakhir). Perbaikan dari review keamanan otomatis: pencocokan per kata, `>&1x` ditolak. Temuan "redirect tulis lolos allowlist" sesuai R1; target redirect diperiksa di U2 (R16). |
| U2 | selesai | `permit/decide.rs`: `decide(scope, call, cwd, review, facts)` dengan urutan KTD-A8 (pagar VM, akar desain, pemeriksa giliran, path tulis yang diselesaikan, pengurai, argumen berbahaya, fakta path, allowlist). Fakta filesystem lewat trait `PathFacts` (`canonical`, `repo_root`). Folder kerja, `cd`, `git -C`/`make -C`/`--dir` harus ada (bila tidak: tolak), di dalam akar run, dan bukan repo bersarang (bila tidak: tanpa keputusan). Program pembuat/pemindah path bersama pindah folder → tanpa keputusan. Target redirect dan kandidat path (KTD-A7) diselesaikan lewat leluhur terdalam yang ada; kredensial → tolak; direktori terlindung → tolak untuk tulis, tanpa keputusan untuk baca. `request_key` (KTD-A12, input kanonis tanpa `description`, varian dengan/tanpa jenis giliran). `permit/args.rs` (KTD-A3, termasuk pengurai script `sed`), `permit/redact.rs` (KTD-A18; `redact_text`, `redact_json`). Helper `node_runs_code`, `rg_runs_program`, `dangerous_git_opt`, `in_protected_dir`, `is_write_tool` dibagi dari hook. `serde_json` (sudah di allow list) ditambahkan ke `mandor-domain`. Test: 28 domain permit. |
| U3 | selesai | Berkas izin giliran (`mandor-agent/src/claude/permit.rs`): `PermitFile` (jenis giliran, allowlist, MCP, akar desain, akar run, anggota, folder bukti), digest SHA-256, `load` tanpa mengikuti symlink, catatan bukti `{tool_use_id, cwd}` sebagai berkas baru 0600 `O_NOFOLLOW`, `FsFacts`. Adapter menulis berkas di `spawn` (`write_permit`: 0600 laptop dengan folder bukti 0700 yang dikosongkan tiap giliran; 0640 dalam folder 2750 di VM tanpa bukti; reviewer tanpa bukti) dan menambah `--permit <berkas> --permit-sha <hex>` ke perintah hook. `run_pre_hook`: tiga keputusan (JSON `permissionDecision: allow`, exit 0 tanpa output, exit 2) dengan tenggat 5 detik yang mencakup pembacaan input. Hook `mandord` memakai jalur berkas izin bila `--permit` ada; tanpa `--permit` (perintah tanpa tool milik daemon) perilaku lama. `ToolCall` membawa `cwd` dan `tool_use_id`. Folder berkas izin terlarang bagi agent (`Scope.private`). `sha2` di `mandor-agent` dicatat di tabel `CLAUDE.md`. Test: 4 adapter permit, 3 guardrails, 1 adapter port, 5 integrasi hook. |
| U4 | selesai | Port `ApprovalStore`, `ApprovalWaiter` (tiket didaftarkan sebelum kartu dibuat), `PermitFacts` di bundel `Deps` dan `RunPorts`. Layanan di `mandor-usecase/src/approval/service.rs` (`impl RunService`): `permit_request` (bukti hook → pemeriksaan ulang → batas 64 KiB → penolakan run → izin run per jenis giliran → mode tolak langsung → batas 5 Pending → kartu Pending dan tunggu 10 menit), `answer_approval` (hanya kartu Pending milik run; pemenang ditentukan store), `approvals_seen`, `list_approvals`, `cancel_run_approvals` (giliran selesai/dihentikan dan run ditutup), `cancel_stale_approvals` (sesi daemon dimulai). Setiap penolakan dan transisi dicatat di Chat secara tersamarkan. Store: migrasi `0012_approvals.sql` (`approvals`, `approval_decisions` berisi hash SHA-256 kunci, `approval_deny_mode`), transisi `UPDATE … WHERE status='pending'` yang menyamarkan input di transaksi yang sama. `mandord`: `TokioWaiter` (oneshot per kartu, milik daemon), `DiskPermits` (`spawn_blocking` dengan batas 10 detik). `sha2` di `mandor-store` dicatat di `CLAUDE.md`. Test: 9 usecase, 3 store, 1 waiter, 1 permit facts. |
| U5 | selesai | Peran kapabilitas `Permit` dan registri grant per run (`PermitGrant`: jenis giliran + `PermitRef`), diterbitkan di `spawn_turn` hanya untuk giliran implementasi dan plan di laptop; dicabut bersama token run. `AgentPort::permit_ref` menghitung path dan digest berkas izin yang sama persis dengan yang ditulis `spawn`. Adapter menambah server `mandor_permit` (`mandord mcp --permit`, token di env) dan `--permission-prompts host --permission-prompt-tool mcp__mandor_permit__permit`; reviewer dan VM tetap `none`. `mandor-api/src/permit.rs`: alat `permit` dengan `JoinSet` per panggilan, satu task penulis stdout, koneksi daemon per panggilan dengan batas 11 menit (`DaemonClient::request_with_timeout`); izin selalu memakai input asli, error apa pun menjadi error alat. Protokol `permit.request` (`PermitRequestParams` → `PermitDecision`). Dispatch: token `Permit` hanya untuk `permit.request`, token ledger tidak bisa memanggilnya (`scoped_route`). Test: 2 usecase, 1 adapter, 2 server permit, 1 dispatch. Uji langsung S2 dengan Mandor sungguhan masuk U9. |
| U6 | selesai | Protokol: `ApprovalView`, `ApprovalAnswerParams`, method `approvals.list/answer/seen`, event `approvals.updated`, `RunView.pending_approvals` (satu query `pending_approval_counts` untuk daftar run dan papan). `NoticeKind::ApprovalNeeded` (`approval_needed`) hanya untuk kartu Pending pertama run, isi tersamarkan dan dipotong 200 karakter. `BroadcastApprovals` menyiarkan `approvals.updated` dan `run.updated` setelah setiap perubahan. Dispatch `dispatch/approvals.rs`: input lengkap hanya untuk kartu Pending; jawaban terlambat → `Rejected` "no longer pending". Koneksi terbatas yang memanggil `approvals.*` ditolak dan dicatat di Chat dengan peran pencobanya (AE16). Frontend: jenis notifikasi `approval_needed` di Settings, binding TS. Test: 1 usecase, 2 dispatch, 1 routing, Vitest settings. |
| U7 | selesai | `approvals.ts` (escape karakter kontrol/bidi, isi kartu, sisa waktu, urutan, label keputusan), store `approvals.svelte.ts` (muat ulang per `approvals.updated` run ini, status "sedang dikirim" UI, jawaban terlambat "This request is no longer pending.", `approvals.seen`), `ApprovalCard.svelte` (blok input bergulir, folder, deskripsi agent berlabel tidak diverifikasi, tiga tombol tanpa fokus otomatis, live region sopan tanpa hitung mundur). Workspace: kartu menunggu di atas composer, riwayat kartu di bawah "N earlier approval requests", hitungan di tab Chat, `approvals.seen` saat tab Chat terlihat, notifikasi `approval_needed` membuka tab Chat dan menggulir ke kartu terlama (`nav.runApprovals`). Badge "Needs approval" di kartu papan, daftar run, dan header workspace tanpa mengubah kolom. Penolakan hook (`tool_result` `PreToolUse … mandor: …`) tampil sebagai satu entri Denied dengan alasan. Mock: dua kartu di PGS-1402; dicek di `/?mock` (kartu, jawaban Allow once, riwayat). Test: 7 logika, 4 store/nav, 1 chat, 1 badge. |
| U8 | selesai | `AgentEvent::Done.denied: Vec<Denial>` (tool, `tool_use_id`, ringkasan input tersamarkan dan dipotong); parser membaca `permission_denials`, event lama tanpa daftar tetap terbaca. Codec chat menyimpan `denied`. Usecase `record_denials`: penolakan yang belum tampil (bukan kartu, bukan event `permission_denied`, bukan hasil hook Mandor yang tercatat) menjadi satu item Chat per penolakan di laptop, termasuk giliran reviewer yang event-nya tidak masuk Chat (AE5); di VM masuk `RunRecord.denials` (maks 50) lalu `ReportCard.denials` → `ReportCardView.denials` → `ReportSection` ("N requests were denied overnight", AE6). Mock VM memuat dua penolakan. Test: 2 stream, 2 usecase. |
| U9 | sebagian | Operator prompt: bentuk gabungan, pipe, redirect, dan path berkurung kini diizinkan otomatis bila setiap bagian ada di allowlist; perintah lain memakai kartu bila sesi bisa, selain itu ditolak; bentuk yang tidak pernah otomatis disebut (test prompt diperbarui). Runbook baru `docs/runbook/agent-permissions.md`, tautan di `README.md`, plan masuk daftar rencana aktif di `CLAUDE.md`, amandemen KTD-10 di plan Rilis 1. `make check` hijau (coverage baris 90,18%). **Belum:** uji langsung laptop dan VM (butuh pemilik produk). |

## Keputusan pelaksanaan

- Hook tanpa `--permit` tetap memakai klasifikasi lama (tolak atau tanpa output). Argumen hook ditulis daemon, jadi jalur ini hanya dipakai perintah milik daemon (pesan commit, deskripsi PR, cek ekstensi Chrome) yang tidak punya giliran.
- Tenggat hook diuji dengan `--deadline-ms` (hanya memperpendek tenggat) dan input yang tidak selesai dibaca.
- Baris perintah gabungan yang memuat pembuat atau pengubah path (termasuk `git checkout`, `git apply`, `rm`, `tee`) tanpa keputusan, bukan hanya bila ada pindah folder; temuan review keamanan.
- Opsi `awk` yang diizinkan otomatis hanya `-F` dan `-v` bentuk pendek; temuan review keamanan.
- Izin otomatis hanya menulis di dalam worktree run: target redirect tulis di luar run, dan kandidat path di luar run untuk program yang bukan baca-saja, menjadi tanpa keputusan; temuan review keamanan.
- Penunggu kartu milik daemon (bukan sesi Lark) agar tidak hilang saat sesi dibuat ulang.

- Kandidat path yang tidak bisa diselesaikan (mis. `..` setelah komponen yang belum ada) menjadi tanpa keputusan, bukan tolak: kandidat sering bukan path (pola grep, nama branch). Folder kerja, target `cd`/`-C`/`--dir`, dan target redirect yang tidak bisa diselesaikan tetap ditolak (KTD-A6, R26).
- Kandidat path di luar run tidak membatalkan izin otomatis; R16 hanya memeriksa path terlarang R11. Batas run berlaku untuk folder kerja dan target pindah folder (R15).

## Bukti contoh penerimaan

| AE | Bukti otomatis |
|---|---|
| AE1 | `permit::decide::tests::ae1_…`, integrasi hook `tiga_keputusan_dan_bukti_pemeriksaan` |
| AE2 | `decide::tests::ae2_dan_r11_…`, usecase `ae2_larangan_keras_…` |
| AE3 | usecase `ae3_izin_run_berlaku_untuk_permintaan_persis_sama` |
| AE4 | usecase `ae4_expired_menyalakan_tolak_langsung_sampai_chat_dibuka` |
| AE5 | usecase `review_flow::covers_ae5_penolakan_reviewer_tercatat_satu_item_chat_tanpa_kartu`; adapter `alat_persetujuan_hanya_untuk_giliran_laptop` (reviewer `none`) |
| AE6 | usecase `ae6_penolakan_vm_masuk_laporan_pagi`, adapter VM `none` |
| AE7 | `decide::tests::tool_selain_bash_tanpa_keputusan`; kartu WebFetch menampilkan URL (`approvals.test.ts`); uji langsung belum |
| AE8 | `decide::tests::argumen_berbahaya_tanpa_keputusan` (`echo $(docker ps)`) |
| AE9, AE15 | `decide::tests::ae9_ae15_…` |
| AE10 | `decide::tests::ae10_r16_…` |
| AE11 | integrasi hook `tenggat_internal_terlewati_menolak`, input rusak → exit 2 |
| AE12 | usecase `ae12_…`, store `transisi_keluar_pending_atomik…`, UI "This request is no longer pending." |
| AE13 | usecase `ae13_deny_berlaku_lintas_giliran` |
| AE14 | usecase `ae14_izin_giliran_implementasi_tidak_berlaku_di_plan` |
| AE16 | dispatch `token_permit_dan_ledger_terpisah`, usecase `approval_answer_refused` |

## Catatan kestabilan

Satu push gagal di pre-push (cover-check) pada test yang tidak terekam; enam putaran test coverage berikutnya lulus. Penantian test server permit dilonggarkan (10 detik) karena paling sensitif terhadap beban instrumentasi coverage. Test login flaky yang dikenal diperbaiki di PR #20.

## Uji langsung hook dengan Claude Code asli (2026-11-06)

Repo git sementara, berkas izin giliran implementasi buatan tangan, `mandord hook --permit … --permit-sha …` dari branch ini, `claude -p … --permission-prompts none` (model haiku), tanpa daemon dan app.

| Perintah | Hasil | AE |
|---|---|---|
| `cd sub && git diff --stat` | Berjalan tanpa bertanya (izin eksplisit hook) | AE1 |
| `git log --oneline -1 2>&1 \| tail -5` | Berjalan tanpa bertanya | AE1, R1 |
| `git push origin HEAD` | Ditolak hook: "mandor: `git push` is run only by the daemon …" | AE2 |
| `echo x > .githooks/pre-push` | Ditolak hook: "mandor: changing agent config or git hooks is denied" | AE10 |
| `curl -sI https://example.com` | Hook tanpa keputusan (catatan bukti tertulis), Claude Code menolak karena sesi tanpa jalur persetujuan; dengan alat persetujuan ini menjadi kartu | R4 |
| `echo $(date)` | Hook tanpa keputusan; Claude Code menolak "A nested command … can't be checked" | AE8 (sebagian; kartu perlu dicek dengan app) |
| `docker ps`, `touch newfile.txt` | Berjalan: Claude Code sendiri mengizinkan perintah baca-saja dan perintah berkas di mode `acceptEdits` | — |

Catatan: karena Claude Code mengizinkan `docker ps` sendiri, uji kartu di app memakai perintah lain (mis. `curl -sI https://example.com`) sebagai pengganti contoh `docker ps` di AE4/AE13.

## Uji langsung dengan app (belum)

Butuh daemon dan app versi branch ini di laptop, dan biner `mandord` baru di VM:

1. Laptop, run kelola: `cd kelola-web && git diff`, Vitest dengan path `(app)` → berjalan tanpa kartu.
2. Perintah di luar allowlist (mis. `curl -sI https://example.com`; `docker ps` diizinkan Claude Code sendiri) → kartu: Allow once, Allow for this run (ulang perintah sama → tanpa kartu), Deny (ulang → ditolak langsung), biarkan 10 menit (expired, mode tolak langsung, buka tab Chat → pulih), 6 kartu bersamaan.
3. Notifikasi dan badge "Needs approval"; klik notifikasi membuka Chat di kartu terlama.
4. VM: satu kartu smoke dengan `cd x && git diff` (izin eksplisit hook) dan satu perintah di luar allowlist → laporan pagi memuat penolakan.
