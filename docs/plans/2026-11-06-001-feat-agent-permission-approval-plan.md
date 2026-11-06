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

- **Objective:** Agent Mandor tidak lagi kena penolakan otomatis untuk perintah aman, dan setiap permintaan izin lain sampai ke dev sebagai kartu persetujuan di Chat run.
- **Product authority:** pemilik produk Mandor (handokobeni). Keputusan di dokumen ini disetujui 2026-11-06.
- **Open blockers:** tidak ada. Spike S1 dan S2 (2026-11-06) membuktikan kedua mekanisme Claude Code; hasilnya di Dependencies / Assumptions.

---

## Product Contract

### Summary

Hook Mandor mengizinkan secara eksplisit perintah yang lolos pemeriksaan Mandor dan setiap bagiannya ada di allowlist sesi. Permintaan izin lain dari Claude Code muncul sebagai kartu persetujuan di Chat run (Allow once, Allow for this run, Deny) dan ditolak otomatis setelah 10 menit. Larangan keras Mandor tetap ditolak tanpa ditanya.

### Problem Frame

Agent berjalan headless (`claude -p`, `--permission-prompts none`, KTD-10 plan Rilis 1). Heuristik bawaan Claude Code meminta persetujuan untuk bentuk perintah tertentu walau perintahnya ada di allowlist:

- `cd <dir>` lalu `git` dalam satu perintah;
- `git -C`, `pnpm --dir`;
- pipe dan redirect (`2>&1 | tail`);
- path berisi `( ) [ ]`, seperti folder route Next.js.

Tanpa layar persetujuan, permintaan itu otomatis ditolak ("no approval surface in this session"). Agent lalu mencari jalan lain, bekerja lebih lama, atau melewatkan verifikasi; di uji langsung PGS-1402 agent gagal menjalankan Vitest dan test integrasi Go. Perintah di luar allowlist (mis. `docker`, `jq`) juga ditolak tanpa dev pernah tahu ada yang diminta. Arahan bentuk perintah di operator prompt (commit `c4f0b2e`) mengurangi kejadian tapi tidak menghilangkannya, karena agent tetap kadang memakai bentuk yang ditolak.

### Key Decisions

- **Dua lapis: izin otomatis untuk yang aman, kartu untuk sisanya.** Kartu untuk setiap permintaan akan memunculkan `cd x && git diff` puluhan kali per run, sehingga dev lelah dan asal menyetujui. Hook Mandor sudah memeriksa setiap perintah, jadi ia yang memutuskan perintah aman; kartu hanya untuk yang tidak dikenal.
- **Larangan keras tidak bisa dibuka dari kartu.** Pagar KTD-10/KTD-11 dan aturan lembur tetap mutlak. Tombol Allow tidak pernah muncul untuk permintaan yang ditolak hook Mandor.
- **Izin dari kartu paling lama berlaku sampai run ditutup.** Izin permanen tetap lewat Settings › Repos › Extra agent allowlist, supaya perluasan permanen selalu disengaja.
- **Tanpa jawaban berarti tolak.** Batas 10 menit menjaga run tidak menggantung saat dev tidak di depan layar.
- **Reviewer AC dan VM tetap tanpa kartu.** Review harus selesai tanpa ditunggu, dan lembur di VM tidak ada orangnya.

### Actors

- A1. Dev: menjawab kartu persetujuan di aplikasi laptop.
- A2. Agent (Claude Code): menjalankan tool; meminta izin bila heuristiknya ragu.
- A3. Daemon Mandor: hook yang memeriksa dan mengizinkan atau menolak, serta penghubung kartu persetujuan ke dev.

### Requirements

**Izin otomatis untuk perintah aman**

- R1. Perintah yang lolos pemeriksaan Mandor dan setiap bagiannya cocok dengan allowlist sesi berjalan tanpa persetujuan, termasuk bentuk gabungan, pipe, redirect, dan path berisi `( ) [ ]`.
- R2. Izin otomatis tidak pernah berlaku untuk perintah yang ditolak hook Mandor.
- R3. Izin otomatis berlaku di giliran implementasi, plan, dan reviewer, dengan aturan hanya-baca plan dan reviewer tetap berlaku.

**Kartu persetujuan**

- R4. Permintaan izin Claude Code di giliran implementasi dan plan muncul sebagai kartu di Chat run, untuk semua jenis tool (Bash, web, dan lainnya).
- R5. Kartu menampilkan tool, perintah atau target lengkap, folder kerja, dan deskripsi langkah dari agent.
- R6. Kartu punya tiga pilihan: Allow once, Allow for this run, dan Deny.
- R7. Allow for this run hanya berlaku untuk permintaan yang persis sama, sampai run ditutup, dan bertahan saat daemon restart.
- R8. Kartu yang tidak dijawab dalam 10 menit ditolak otomatis dan ditandai expired; agent menerima penolakan dan melanjutkan.
- R9. Saat kartu muncul, run tampil menunggu persetujuan di daftar run dan board, dan dev menerima notifikasi sesuai preferensi notifikasi yang ada.
- R10. Kartu yang sudah dijawab atau expired tetap tercatat di Chat bersama keputusannya.

**Batas**

- R11. Larangan keras ditolak tanpa kartu: `git push`, `git commit`, `git remote`, `gh`, berkas kredensial, penulisan ke `.claude`, `.git`, `.husky`, `.githooks`, dan perubahan status Lark melewati Waiting Review.
- R12. Giliran reviewer AC tidak memunculkan kartu; permintaan di luar allowlist tetap ditolak otomatis.
- R13. Di daemon VM tidak ada kartu; permintaan ditolak otomatis dan tercatat di laporan pagi.

### Key Flows

- F1. Perintah aman dengan bentuk yang biasanya ditolak
  - **Trigger:** agent menjalankan `cd kelola-web && git diff -- src/app/(app)/x.tsx`.
  - **Steps:** hook Mandor memeriksa setiap bagian, semua ada di allowlist, hook mengizinkan; Claude Code menjalankannya tanpa bertanya.
  - **Outcome:** tidak ada kartu dan tidak ada penolakan.
  - **Covered by:** R1, R2
- F2. Perintah di luar allowlist
  - **Trigger:** agent di giliran implementasi menjalankan `docker compose up -d db`.
  - **Steps:** hook tidak menolak dan tidak mengizinkan; Claude Code meminta izin; kartu muncul di Chat run dan notifikasi dikirim; dev memilih Allow for this run; agent melanjutkan; permintaan yang sama berikutnya di run itu berjalan tanpa kartu.
  - **Outcome:** perintah berjalan atas keputusan dev.
  - **Covered by:** R4, R5, R6, R7, R9
- F3. Dev tidak menjawab
  - **Trigger:** kartu muncul saat dev tidak di depan layar.
  - **Steps:** 10 menit berlalu; kartu ditandai expired; agent menerima penolakan.
  - **Outcome:** run tidak menggantung; kartu tercatat di Chat.
  - **Covered by:** R8, R10

### Acceptance Examples

- AE1. **Covers R1, R3.** Given giliran plan, when agent menjalankan `cd kelola-api && git log --oneline -5`, then perintah berjalan tanpa kartu dan tanpa penolakan.
- AE2. **Covers R2, R11.** Given giliran implementasi, when agent menjalankan `cd repo && git push origin HEAD`, then perintah ditolak dengan alasan Mandor dan tidak ada kartu.
- AE3. **Covers R6, R7.** Given dev memilih Allow for this run untuk `jq . package.json`, when agent menjalankan `jq . package.json` lagi di run yang sama, then perintah berjalan tanpa kartu; when agent menjalankan `jq .name package.json`, then kartu baru muncul.
- AE4. **Covers R8.** Given kartu untuk `docker ps` tidak dijawab, when 10 menit berlalu, then kartu ditandai expired dan agent menerima penolakan.
- AE5. **Covers R12.** Given giliran reviewer AC, when reviewer menjalankan perintah di luar allowlist, then perintah ditolak otomatis tanpa kartu.
- AE6. **Covers R13.** Given kartu VM saat lembur, when agent meminta izin untuk perintah di luar allowlist, then permintaan ditolak dan tercatat di laporan pagi kartu itu.
- AE7. **Covers R4.** Given giliran implementasi, when agent memakai WebFetch ke situs dokumentasi, then kartu muncul dengan URL lengkap sebagai target.

### Success Criteria

- Run laptop di workspace kelola tidak lagi menampilkan penolakan "no approval surface" untuk perintah yang ada di allowlist.
- Setiap permintaan izin di laptop terlihat oleh dev sebagai kartu atau berjalan karena izin otomatis; tidak ada penolakan yang dev tidak ketahui.
- Larangan keras tetap ditolak di semua giliran, terbukti oleh test.

### Scope Boundaries

- Izin permanen dari kartu ("Always allow in this repo") tidak dibangun.
- Kartu di giliran reviewer AC tidak dibangun.
- Persetujuan untuk kartu VM (mis. dijawab pagi hari lalu kartu dilanjutkan) tidak dibangun.
- Allow berbasis pola (mis. `docker *`) dari kartu tidak dibangun; Allow for this run hanya untuk permintaan persis sama.

### Dependencies / Assumptions

Hasil spike dengan Claude Code 2.1.318 di mode `-p` stream-json, `--permission-mode acceptEdits`, model haiku, di repo uji sementara:

- S1. Tanpa jawaban hook, `cd sub && git diff --stat` ditolak ("changes directory before a version-control command"). Dengan hook `PreToolUse` yang menjawab `permissionDecision: allow`, perintah yang sama berjalan. R1 bisa dibangun di atas hook.
- S1b. Hook yang menjawab `allow` untuk `git push` tetap ditolak bila `Bash(git push*)` ada di `--disallowedTools`. Deny list Claude Code tetap lapis kedua di bawah hook Mandor.
- S2. Dengan `--permission-prompts host` dan `--permission-prompt-tool` ke tool MCP, permintaan izin sampai ke tool berisi `tool_name`, input lengkap (termasuk `description` dari agent), dan `tool_use_id`. Jawaban `{"behavior":"allow","updatedInput":...}` menjalankan perintah; `{"behavior":"deny","message":...}` meneruskan pesan itu ke agent.
- S2b. Alasan Claude Code meminta izin tidak dikirim ke tool, sehingga kartu memakai deskripsi dari agent (R5).
- S2c. Claude Code menunggu jawaban tool paling tidak 11 menit (jeda 150 detik dan 660 detik sama-sama berhasil). Batas 10 menit di R8 dipegang Mandor.
- Server MCP per run yang sudah ada (alat ledger) diasumsikan bisa menampung alat persetujuan.

### Outstanding Questions

**Deferred to Planning**

- Bagaimana kartu persetujuan dan jawabannya disimpan supaya Allow for this run bertahan saat daemon restart (R7).
- Bagaimana permintaan di VM dicatat di laporan pagi (R13).

### Sources / Research

- KTD-10 dan KTD-11 di `docs/plans/2026-10-18-001-feat-korlap-lark-lembur-plan.md`: pagar izin agent saat ini.
- `crates/mandor-agent/src/claude/command.rs`: argumen `claude` (`--permission-mode`, `--permission-prompts none`, allowlist).
- `crates/mandor-agent/src/claude/guardrails.rs`, `crates/mandor-domain/src/hook.rs`, `crates/mandor-domain/src/hook_review.rs`: hook dan allowlist per giliran.
- Commit `c4f0b2e` di `feat/prototype-compare`: arahan bentuk perintah di operator prompt.
- `docs/progress/env-picker-prototype-compare.md`: penolakan yang teramati di uji langsung PGS-1402.
