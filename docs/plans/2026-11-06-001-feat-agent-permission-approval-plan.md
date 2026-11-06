---
title: Izin agent tanpa penolakan diam-diam - Plan
type: feat
date: 2026-11-06
topic: agent-permission-approval
artifact_contract: ce-unified-plan/v1
artifact_readiness: requirements-only
product_contract_source: ce-brainstorm
execution: code
---

# Izin agent tanpa penolakan diam-diam - Plan

## Goal Capsule

- **Objective:** Agent Mandor tidak lagi kena penolakan otomatis untuk perintah aman, dan setiap permintaan izin lain di giliran implementasi dan plan di laptop sampai ke dev sebagai kartu persetujuan di Chat run.
- **Product authority:** pemilik produk Mandor (handokobeni). Keputusan di dokumen ini disetujui 2026-11-06.
- **Open blockers:** tidak ada. Spike S1 dan S2 (2026-11-06) membuktikan kedua mekanisme Claude Code; hasilnya di Dependencies / Assumptions.

---

## Product Contract

### Summary

Hook Mandor mengizinkan secara eksplisit perintah yang lolos pemeriksaan Mandor dan setiap bagiannya ada di allowlist giliran itu. Permintaan izin lain dari Claude Code di giliran implementasi dan plan di laptop muncul sebagai kartu persetujuan di Chat run (Allow once, Allow for this run, Deny) dan ditolak otomatis setelah 10 menit. Larangan keras Mandor tetap ditolak tanpa ditanya.

### Problem Frame

Agent berjalan headless (`claude -p`, `--permission-prompts none`, KTD-10 plan Rilis 1). Heuristik bawaan Claude Code meminta persetujuan untuk bentuk perintah tertentu walau perintahnya ada di allowlist:

- `cd <dir>` lalu `git` dalam satu perintah;
- `git -C`, `pnpm --dir`;
- pipe dan redirect (`2>&1 | tail`);
- path berisi `( ) [ ]`, seperti folder route Next.js.

Tanpa layar persetujuan, permintaan itu otomatis ditolak ("no approval surface in this session"). Agent lalu mencari jalan lain, bekerja lebih lama, atau melewatkan verifikasi; di uji langsung PGS-1402 agent gagal menjalankan Vitest dan test integrasi Go. Perintah di luar allowlist (mis. `docker`, `jq`) juga ditolak tanpa dev pernah tahu ada yang diminta. Arahan bentuk perintah di operator prompt (commit `c4f0b2e`) mengurangi kejadian tapi tidak menghilangkannya, karena agent tetap kadang memakai bentuk yang ditolak.

### Key Decisions

- **Dua lapis: izin otomatis untuk yang aman, kartu untuk sisanya.** Kartu untuk setiap permintaan akan memunculkan `cd x && git diff` puluhan kali per run, sehingga dev lelah dan asal menyetujui. Hook Mandor yang memutuskan perintah aman; kartu hanya untuk yang tidak dikenal.
- **Izin otomatis hanya untuk yang bisa diurai penuh.** Dengan izin eksplisit, hook Mandor menggantikan pemeriksaan bawaan Claude Code untuk perintah itu, jadi hook harus tahu persis setiap program yang dijalankan. Bentuk yang menyembunyikan program lain tidak pernah diizinkan otomatis.
- **Gagal memeriksa berarti tolak, bukan kartu.** Hook tetap fail-closed (KTD-10): bug atau input rusak tidak boleh berubah menjadi kartu yang bisa di-Allow.
- **Larangan keras tidak bisa dibuka dari kartu.** Pagar KTD-10/KTD-11 dan aturan lembur tetap mutlak. Tombol Allow tidak pernah muncul untuk permintaan yang ditolak hook Mandor.
- **Input mentah adalah isi utama kartu.** Deskripsi dari agent bisa menyesatkan (prompt injection dari data Lark), jadi dev menilai dari perintah sebenarnya.
- **Izin dari kartu paling lama berlaku sampai run ditutup, untuk permintaan persis sama.** Izin permanen tetap lewat Settings › Repos › Extra agent allowlist, supaya perluasan permanen selalu disengaja.
- **Tanpa jawaban berarti tolak, dan run tidak menunggu berulang.** Batas 10 menit per kartu; setelah satu kartu expired, permintaan berikutnya langsung ditolak sampai dev kembali.
- **Deny berlaku untuk sisa run.** Permintaan persis sama yang sudah ditolak dev tidak memunculkan kartu lagi, supaya agent tidak bisa membujuk lewat kartu berulang.
- **Setiap penolakan terlihat.** Penolakan di laptop tercatat di Chat dan penolakan di VM tercatat di laporan pagi, supaya dev tahu kenapa agent mengambil jalan lain.
- **Reviewer AC dan VM tetap tanpa kartu.** Review harus selesai tanpa ditunggu, dan lembur di VM tidak ada orangnya.

### Actors

- A1. Dev: menjawab kartu persetujuan di aplikasi laptop.
- A2. Agent (Claude Code): menjalankan tool; meminta izin bila heuristiknya ragu.
- A3. Daemon Mandor: hook yang memeriksa dan mengizinkan atau menolak, serta penghubung kartu persetujuan ke dev.

### Requirements

**Izin otomatis untuk perintah aman**

- R1. Perintah yang lolos pemeriksaan Mandor dan setiap program di dalamnya cocok dengan allowlist giliran itu berjalan tanpa persetujuan, termasuk bentuk gabungan, pipe, redirect, dan path berisi `( ) [ ]`.
- R2. Izin otomatis tidak pernah berlaku untuk perintah yang ditolak hook Mandor.
- R3. Izin otomatis berlaku di giliran implementasi, plan, dan reviewer, dengan aturan hanya-baca plan dan reviewer tetap berlaku.
- R14. Perintah yang tidak bisa diurai penuh menjadi program-program sederhana tidak pernah diizinkan otomatis; ini mencakup substitusi perintah (`$(…)`, backtick), substitusi proses (`<(…)`), `eval`, `sh -c`/`bash -c`, `xargs`, `env` sebagai pembungkus, dan here-doc.
- R15. Bentuk `cd <dir> && …`, `git -C <dir>`, dan `--dir <dir>` hanya diizinkan otomatis bila foldernya, setelah symlink dan `..` diselesaikan, di dalam worktree run dan bukan repo git bersarang; folder yang tidak bisa diselesaikan tidak diizinkan otomatis.
- R16. Target redirect dan argumen path dari perintah yang diizinkan otomatis diselesaikan ke path sebenarnya lalu diperiksa terhadap path terlarang yang sama dengan R11.

**Kartu persetujuan**

- R4. Permintaan izin Claude Code di giliran implementasi dan plan di laptop muncul sebagai kartu di Chat run, untuk semua jenis tool (Bash, web, dan lainnya).
- R5. Isi utama kartu adalah input tool apa adanya (perintah, URL, atau pasangan kunci-nilai), lengkap tanpa dipotong, dengan karakter kontrol dan bidi di-escape, ditambah folder kerja; input panjang bergulir di area terbatas sehingga tombol kartu tetap terlihat.
- R17. Deskripsi langkah dari agent tampil terpisah dengan label bahwa itu klaim agent yang tidak diverifikasi Mandor.
- R6. Kartu punya tiga pilihan berurutan Deny, Allow once, dan "Allow this exact request for this run"; tidak ada tombol yang fokus atau terpicu otomatis saat kartu muncul, tombol bisa dicapai dengan keyboard, dan kartu baru diumumkan sopan ke pembaca layar tanpa mengumumkan setiap detik hitung mundur.
- R7. Allow for this run berlaku untuk tool, input persis, dan folder kerja yang sama, sampai run ditutup, termasuk giliran berikutnya dan setelah daemon restart, tetapi hanya di jenis giliran tempat izin itu diberikan; giliran reviewer tidak pernah memakai izin dari kartu.
- R18. Jawaban hanya diterima dari koneksi aplikasi dev yang terautentikasi (bukan token atau proses milik agent), hanya untuk kartu yang masih menunggu di run itu, dan izin selalu menjalankan input asli tanpa perubahan.
- R8. Kartu yang tidak dijawab dalam 10 menit ditolak otomatis dan ditandai expired; kartu yang menunggu menampilkan sisa waktunya.
- R19. Setelah satu kartu expired, permintaan izin berikutnya di run itu langsung ditolak tanpa kartu dan tercatat di Chat, sampai dev membuka tab Chat run itu; Chat mencatat saat mode tolak langsung mulai dan berakhir.
- R20. Kartu punya status menunggu, sedang dikirim, dijawab, expired, dan dibatalkan (run berhenti, ditutup, atau daemon restart saat kartu menunggu); hanya kartu menunggu yang punya tombol, dan jawaban terlambat menampilkan pemberitahuan bahwa kartu sudah tidak berlaku.
- R9. Saat kartu menunggu, run mendapat badge "Needs approval" di daftar run dan board tanpa pindah kolom, tab Chat menampilkan jumlah kartu menunggu, dan notifikasi (sesuai preferensi yang ada) menyebut tool dan perintahnya lalu membuka Chat run di kartu itu.
- R10. Kartu yang sudah dijawab, expired, atau dibatalkan tetap tercatat di Chat bersama keputusannya.
- R23. Setelah dev menekan Deny, permintaan yang persis sama di run itu ditolak langsung tanpa kartu untuk sisa run dan tercatat di Chat.
- R24. Beberapa kartu yang menunggu bersamaan tampil urut dari yang terlama, masing-masing dengan batas waktunya sendiri; notifikasi hanya dikirim untuk kartu pertama dan membuka Chat di kartu terlama yang masih menunggu.

**Batas dan keterlihatan**

- R11. Larangan keras ditolak tanpa kartu: `git push`, `git commit`, `git remote`, `gh`, berkas kredensial, penulisan ke `.claude`, `.git`, `.husky`, `.githooks`, dan perubahan status Lark yang melewati Waiting Review.
- R21. Bila hook gagal memeriksa (error internal, input rusak, batas waktu), permintaan ditolak tanpa kartu, sesuai sifat fail-closed KTD-10.
- R12. Giliran reviewer AC tidak memunculkan kartu; permintaan di luar allowlist tetap ditolak otomatis.
- R22. Setiap penolakan di laptop (larangan keras, penolakan reviewer, kegagalan pemeriksaan hook, expired, dan penolakan langsung R19 dan R23) tercatat ringkas di Chat run dengan tool, perintah, dan alasan, tanpa tombol.
- R25. Nilai yang tampak rahasia (token bearer, pasangan kunci-nilai kredensial, userinfo di URL) disamarkan di catatan Chat yang tersimpan, teks notifikasi, dan laporan pagi; input lengkap tanpa disamarkan hanya tampil di kartu yang masih menunggu.
- R26. Kartu hanya boleh muncul untuk permintaan yang sudah diperiksa hook sampai selesai tanpa keputusan; permintaan izin tanpa catatan pemeriksaan itu (mis. hook terhenti karena batas waktu) ditolak tanpa kartu.
- R13. Di daemon VM tidak ada kartu; permintaan ditolak otomatis dan tercatat di laporan pagi kartu itu dengan perintah dan alasannya.

### Key Flows

- F1. Perintah aman dengan bentuk yang biasanya ditolak
  - **Trigger:** agent menjalankan `cd kelola-web && git diff -- src/app/(app)/x.tsx`.
  - **Steps:** hook Mandor mengurai perintah, foldernya di dalam worktree, setiap program ada di allowlist, hook mengizinkan; Claude Code menjalankannya tanpa bertanya.
  - **Outcome:** tidak ada kartu dan tidak ada penolakan.
  - **Covered by:** R1, R2, R15
- F2. Perintah di luar allowlist
  - **Trigger:** agent di giliran implementasi menjalankan `docker compose up -d db`.
  - **Steps:** hook selesai memeriksa, tidak ada larangan, tapi `docker` tidak di allowlist, jadi hook tidak memberi keputusan; Claude Code meminta izin; kartu muncul di Chat run, run mendapat badge, notifikasi dikirim; dev memilih Allow this exact request for this run; agent melanjutkan; permintaan yang persis sama berikutnya di run itu berjalan tanpa kartu.
  - **Outcome:** perintah berjalan atas keputusan dev.
  - **Covered by:** R4, R5, R6, R7, R9, R18
- F3. Dev tidak menjawab
  - **Trigger:** kartu muncul saat dev tidak di depan layar.
  - **Steps:** 10 menit berlalu; kartu ditandai expired; agent menerima penolakan; permintaan berikutnya di run itu langsung ditolak dan tercatat sampai dev kembali.
  - **Outcome:** run tidak menggantung berulang; semua penolakan tercatat di Chat.
  - **Covered by:** R8, R10, R19, R22

### Acceptance Examples

- AE1. **Covers R1, R3.** Given giliran plan, when agent menjalankan `cd kelola-api && git log --oneline -5`, then perintah berjalan tanpa kartu dan tanpa penolakan.
- AE2. **Covers R2, R11, R22.** Given giliran implementasi, when agent menjalankan `cd repo && git push origin HEAD`, then perintah ditolak dengan alasan Mandor, tidak ada kartu, dan penolakannya tercatat di Chat.
- AE3. **Covers R6, R7.** Given dev memilih Allow this exact request for this run untuk `jq . package.json`, when agent menjalankan `jq . package.json` lagi di folder yang sama, then perintah berjalan tanpa kartu; when agent menjalankan `jq .name package.json`, then kartu baru muncul.
- AE4. **Covers R8, R19.** Given kartu untuk `docker ps` tidak dijawab, when 10 menit berlalu, then kartu ditandai expired dan agent menerima penolakan; when agent lalu meminta izin untuk `jq . a.json`, then permintaan itu langsung ditolak tanpa kartu dan tercatat di Chat.
- AE5. **Covers R12, R22.** Given giliran reviewer AC, when reviewer menjalankan perintah di luar allowlist, then perintah ditolak otomatis tanpa kartu dan tercatat di Chat.
- AE6. **Covers R13.** Given agent di daemon VM saat lembur, when agent meminta izin untuk perintah di luar allowlist, then permintaan ditolak otomatis tanpa kartu dan tercatat di laporan pagi kartu itu.
- AE7. **Covers R4, R5.** Given giliran implementasi, when agent memakai WebFetch ke situs dokumentasi, then kartu muncul dengan URL lengkap sebagai isi utama.
- AE8. **Covers R14.** Given giliran implementasi, when agent menjalankan `echo $(docker ps)`, then perintah tidak diizinkan otomatis dan kartu muncul.
- AE9. **Covers R15.** Given worktree run memuat folder `fixtures/repo` yang berisi `.git` sendiri, when agent menjalankan `cd fixtures/repo && git status`, then perintah tidak diizinkan otomatis dan kartu muncul.
- AE10. **Covers R16, R11.** Given giliran implementasi, when agent menjalankan `echo x > .githooks/pre-push`, then perintah ditolak dengan alasan Mandor tanpa kartu.
- AE11. **Covers R21, R26.** Given input hook rusak atau hook melewati batas waktunya, when agent menjalankan perintah apa pun, then perintah ditolak tanpa kartu dan penolakannya tercatat di Chat.
- AE12. **Covers R18, R20.** Given kartu sudah expired, when dev menekan Allow dari tampilan lama, then jawaban ditolak dan kartu menampilkan bahwa ia sudah tidak berlaku.
- AE13. **Covers R23.** Given dev menekan Deny untuk `docker compose up -d`, when agent meminta permintaan yang persis sama lagi di run itu, then permintaan ditolak langsung tanpa kartu dan tercatat di Chat.
- AE14. **Covers R7.** Given dev memberi Allow for this run untuk `jq . a.json` di giliran implementasi, when giliran reviewer menjalankan `jq . a.json`, then izin itu tidak dipakai dan perintah ditolak otomatis.
- AE15. **Covers R15.** Given `docs` di worktree adalah symlink ke folder di luar worktree, when agent menjalankan `cd docs && git log`, then perintah tidak diizinkan otomatis.
- AE16. **Covers R18.** Given kartu menunggu, when jawaban Allow dikirim dengan token kapabilitas milik agent, then jawaban ditolak dan percobaannya tercatat sebagai penolakan.

### Success Criteria

- Run laptop di workspace kelola tidak lagi menampilkan penolakan "no approval surface" untuk perintah yang ada di allowlist.
- Setiap permintaan izin di laptop terlihat oleh dev sebagai kartu, berjalan karena izin otomatis, atau tercatat sebagai penolakan di Chat.
- Larangan keras dan bentuk perintah di R14–R16 tetap ditolak atau tidak diizinkan otomatis di semua giliran, terbukti oleh test.

### Scope Boundaries

- Izin permanen dari kartu ("Always allow in this repo") tidak dibangun.
- Kartu di giliran reviewer AC tidak dibangun.
- Persetujuan untuk permintaan di VM (mis. dijawab pagi hari lalu kartu dilanjutkan) tidak dibangun.
- Allow berbasis pola atau program (mis. `docker *`) dari kartu tidak dibangun; Allow for this run hanya untuk permintaan persis sama.
- Daftar izin yang sudah diberi di run dan pencabutannya tidak dibangun di rilis ini.

### Dependencies / Assumptions

Hasil spike dengan Claude Code 2.1.318 di mode `-p` stream-json, `--permission-mode acceptEdits`, model haiku, di repo uji sementara:

- S1. Tanpa jawaban hook, `cd sub && git diff --stat` ditolak ("changes directory before a version-control command"). Dengan hook `PreToolUse` yang menjawab `permissionDecision: allow`, perintah yang sama berjalan. R1 bisa dibangun di atas hook.
- S1b. Hook yang menjawab `allow` untuk `git push` tetap ditolak bila `Bash(git push*)` ada di `--disallowedTools`. Deny list Claude Code tetap lapis kedua di bawah hook Mandor.
- S2. Dengan `--permission-prompts host` dan `--permission-prompt-tool` ke tool MCP, permintaan izin sampai ke tool berisi `tool_name`, input lengkap (termasuk `description` dari agent), dan `tool_use_id`. Jawaban `{"behavior":"allow","updatedInput":...}` menjalankan perintah; `{"behavior":"deny","message":...}` meneruskan pesan itu ke agent.
- S2b. Alasan Claude Code meminta izin tidak dikirim ke tool, sehingga kartu memakai input mentah dan deskripsi dari agent (R5, R17).
- S2c. Claude Code menunggu jawaban tool paling tidak 11 menit (jeda 150 detik dan 660 detik sama-sama berhasil). Batas 10 menit di R8 dipegang Mandor.

Keadaan kode yang mempengaruhi planning (dari review dokumen):

- Hook Mandor sekarang hanya mencari kata terlarang dan menghasilkan dua keputusan (tidak keberatan atau tolak); ia belum mengurai perintah per program dan belum menerima allowlist giliran. R1 dan R14–R16 butuh keputusan ketiga (izinkan eksplisit) dan pengurai yang lebih ketat.
- Server MCP Mandor sekarang hanya terpasang di giliran reviewer dan plan yang punya sumber desain, dengan token kapabilitas berperan Design/Verdict, dan jembatan MCP ke daemon membatasi satu panggilan 10 detik. Alat persetujuan butuh server MCP di setiap giliran implementasi dan plan di laptop, peran kapabilitas sendiri, dan panggilan yang boleh menunggu sampai 10 menit.

### Outstanding Questions

**Deferred to Planning**

- S3. Spike ulang di `--permission-mode default` (dipakai giliran plan): apakah izin dari hook dan `--permission-prompts host` berperilaku sama seperti di `acceptEdits`?
- Apakah jawaban `allow` dari hook juga melewati `--allowedTools` untuk tool selain Bash (WebFetch, MCP), dan bagaimana itu membatasi R1.
- Bagaimana kartu dan izin disimpan supaya Allow for this run bertahan saat daemon restart (R7), dan bagaimana kartu yang menunggu saat restart ditandai dibatalkan (R20).
- Bagaimana penolakan di VM dan penolakan dari deny list Claude Code (yang tidak terlihat hook maupun alat persetujuan) ditangkap untuk R13 dan R22, mis. dari laporan penolakan di pesan hasil stream-json.
- Bagaimana alat persetujuan bersikap bila panggilannya sendiri gagal atau melewati batas waktu (dikonfirmasi di spike S3).
- Apakah alat persetujuan bisa dipanggil langsung oleh agent untuk membuat kartu sendiri, dan bagaimana mencegahnya.

### Sources / Research

- KTD-10 dan KTD-11 di `docs/plans/2026-10-18-001-feat-korlap-lark-lembur-plan.md`: pagar izin agent saat ini.
- `crates/mandor-agent/src/claude/command.rs`: argumen `claude` (`--permission-mode`, `--permission-prompts none`, allowlist).
- `crates/mandor-agent/src/claude/guardrails.rs`, `crates/mandor-domain/src/hook.rs`, `crates/mandor-domain/src/hook_review.rs`: hook dan allowlist per giliran.
- Commit `c4f0b2e` di `feat/prototype-compare`: arahan bentuk perintah di operator prompt.
- `docs/progress/env-picker-prototype-compare.md` (branch `feat/prototype-compare`, PR #19): penolakan yang teramati di uji langsung PGS-1402.
