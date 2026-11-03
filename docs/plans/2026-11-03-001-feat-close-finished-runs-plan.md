---
title: Tutup run kartu yang sudah selesai - Plan
type: feat
date: 2026-11-03
topic: close-finished-runs
artifact_contract: ce-unified-plan/v1
artifact_readiness: implementation-ready
product_contract_source: ce-plan-bootstrap
execution: code
---

# Tutup run kartu yang sudah selesai - Plan

## Goal Capsule

- **Objective:** Run yang kartunya sudah selesai (Delivered atau Waiting Deploy) ditutup dan worktree-nya dihapus, supaya storage laptop dan VM tidak penuh. Dev bisa menutupnya sekaligus dengan satu tombol, atau menyalakan penutupan otomatis.
- **Product authority:** pemilik produk, 2026-11-03, setelah uji langsung plan 004 meninggalkan 44 run lama dan 1,1 GB worktree di laptop (sekitar 900 MB di antaranya `node_modules` satu worktree kelola-v2). Keputusan: tombol manual dan otomatis (opsional); status selesai = Delivered dan Waiting Deploy; run dengan perubahan belum di-push dilewati; tombol di kolom Done dan di Settings → Storage; Settings → Storage juga mendaftar semua run terbuka urut ukuran (R9).
- **Open blockers:** tidak ada.
- **Execution profile:** test-first lewat antarmuka publik (domain, usecase dengan fake, store); frontend Vitest; uji langsung dengan kartu uji di Base salinan.
- **Authority hierarchy:** `CLAUDE.md` (standar `rust-engineering` mengikat) > Product Contract > Planning Contract > unit.
- **Stop conditions:** berhenti dan tanya bila perilaku produk harus berubah, atau bila unit tidak bisa selesai tanpa melanggar `rust-engineering`.
- **Tail ownership:** branch fitur, satu PR ke `main`, tanpa merge sendiri.

---

## Product Contract

### Summary

Setiap run membuat worktree sendiri (satu per repo). Saat ini worktree baru dihapus bila dev menutup run satu per satu. Fitur ini menambah penutupan sekaligus untuk run yang kartunya sudah selesai, dan opsi menutupnya otomatis.

### Requirements

- R1. Kartu dianggap **selesai** bila statusnya di Lark Delivered atau Waiting Deploy. Run grup dianggap selesai bila semua kartu yang masih disertakan (tidak dikeluarkan) selesai.
- R2. Run **bisa ditutup** bila kartunya selesai (R1), run tidak sedang bekerja (agent berjalan, review, publish, atau giliran perbaikan), dan setiap worktree anggotanya bersih dan tidak punya commit yang belum di-push. Run yang tidak memenuhi syarat kebersihan dilewati dengan alasan "Unpushed changes"; worktree-nya tidak pernah dihapus.
- R3. Menutup run memakai perilaku Close run yang ada: klaim dilepas, worktree dan folder run workspace dihapus, status Lark tidak diubah.
- R4. **Kolom Done di board** menampilkan tombol "Close N finished runs" bila ada setidaknya satu run yang bisa ditutup. Tombol membuka dialog konfirmasi berisi daftar run (kunci kartu, judul, ukuran worktree) dan daftar run yang dilewati beserta alasannya, lalu menutup semua run yang bisa ditutup dan menampilkan ringkasan ("Closed N runs, freed X").
- R5. **Settings → Storage** menampilkan total ukuran worktree run yang terbuka, jumlah run selesai yang bisa ditutup beserta ukurannya, tombol yang sama dengan R4, dan setelan "Close finished runs automatically" (default mati).
- R9. **Settings → Storage** juga menampilkan daftar semua run terbuka di daemon itu, diurutkan dari worktree terbesar: kunci kartu, judul, status run, status kartu di Lark, ukuran worktree, dan waktu terakhir run berubah. Setiap baris punya aksi Open (membuka workspace run) dan Close run (perilaku Close run yang ada, dengan konfirmasi; worktree dengan perubahan belum di-push tetap dibiarkan seperti sekarang). Run yang belum selesai tidak pernah ditutup otomatis.
- R6. Bila setelan otomatis menyala, daemon memeriksa run selesai secara berkala setelah sinkron Lark dan menutup yang bisa ditutup (R2). Setiap penutupan otomatis dicatat di log, dan satu notifikasi ringkas per pemeriksaan yang menutup sesuatu ("Closed N finished runs, freed X").
- R7. Daemon VM memakai aturan yang sama untuk run VM. Tab Overnight menampilkan tombol "Close N finished runs" untuk run di VM, dan Settings → VM menampilkan setelan otomatis untuk daemon VM.
- R8. Kartu Waiting Deploy yang run-nya sudah ditutup lalu kembali ke Waiting Fix ditangani loop Waiting Fix yang ada: run baru dibuat dari branch kartu.

### Acceptance Examples

- AE1. **Covers R1, R2, R4.** Given tiga run laptop: PGS-1 Delivered dengan worktree bersih, PGS-2 Waiting Deploy dengan satu commit belum di-push, PGS-3 In Progress, when dev membuka kolom Done, then tombol menampilkan "Close 1 finished run", dialog mencantumkan PGS-1 untuk ditutup dan PGS-2 dilewati "Unpushed changes", dan setelah konfirmasi hanya run PGS-1 yang tertutup dan worktree-nya hilang.
- AE2. **Covers R1.** Given run grup PGS-10..12 dengan PGS-11 dikeluarkan, PGS-10 Delivered dan PGS-12 Waiting Review, then run grup belum selesai; setelah PGS-12 Delivered, run grup bisa ditutup.
- AE3. **Covers R6.** Given setelan otomatis menyala dan run PGS-1 bersih, when kartu PGS-1 pindah ke Delivered dan sinkron Lark berikutnya selesai, then run ditutup tanpa tindakan dev, log mencatatnya, dan satu notifikasi "Closed 1 finished run, freed X" muncul.
- AE4. **Covers R2.** Given run PGS-1 Delivered sedang menjalankan review AC, then run tidak ditutup sampai review selesai.
- AE5. **Covers R7.** Given run VM PGS-5 Delivered dan bersih, when dev menekan "Close 1 finished run" di tab Overnight, then run VM tertutup dan worktree di VM hilang.
- AE7. **Covers R9.** Given run kelola-v2 PGS-1401 awaiting dev dengan worktree 1 GB dan dua run kecil, when dev membuka Settings → Storage, then PGS-1401 tampil paling atas dengan ukuran sekitar 1 GB, dan Close run pada baris itu menutup run dan menghapus worktree-nya setelah konfirmasi.
- AE6. **Covers R8.** Given run PGS-1 Waiting Deploy ditutup otomatis, when QA memindahkan PGS-1 ke Waiting Fix dengan komentar tepercaya, then loop Waiting Fix membuat run baru dari branch PGS-1 seperti sekarang.

### Scope Boundaries

- Tidak menghapus branch di remote dan tidak menutup PR.
- Tidak mengubah status Lark.
- Tidak membersihkan worktree yatim (folder tanpa run di database); itu pekerjaan terpisah.
- Tidak menutup run yang kartunya di status lain (mis. Waiting Review lama).

---

## Planning Contract

### Key Technical Decisions

- **KTD-C1. Aturan selesai di domain.** `mandor_domain::cleanup::finished(statuses)` untuk satu kartu atau kartu grup yang masih disertakan, dan `close_decision(run_state, finished, clean) -> Close | Skip(reason)`. Status yang dihitung selesai: `Delivered`, `WaitingDeploy`.
- **KTD-C2. Usecase memakai `close_run` yang ada.** `RunService::finished_runs()` mengembalikan kandidat (run, kunci kartu, ukuran worktree) dan run yang dilewati (alasan); `close_finished(run_ids)` memeriksa ulang setiap run sebelum menutup (status kartu dari cache yang baru disinkron, kebersihan worktree lewat `GitPort::changes`). Pemeriksaan ulang mencegah kondisi berubah antara dialog dan konfirmasi.
- **KTD-C3. Ukuran worktree lewat port.** `GitPort` mendapat `dir_size(path)` (adapter: jalan direktori di `spawn_blocking` dengan batas waktu, tanpa mengikuti symlink, `.git` worktree ikut dihitung). Ukuran hanya informasi; gagal mengukur menampilkan "size unknown" dan tidak menghalangi penutupan.
- **KTD-C4. Setelan otomatis per daemon.** Disimpan di tabel `settings` (kunci `cleanup`, JSON `{auto_close: bool}`) lewat store; RPC `settings.cleanup.get` / `settings.cleanup.set`. Pemeriksaan otomatis berjalan di loop sesi setelah poll Lark, maksimal sekali per 10 menit, dan berhenti bersih saat shutdown (`CancellationToken`).
- **KTD-C5. Protokol.** `runs.finished` → `FinishedRunsView { closable: [FinishedRunView], skipped: [SkippedRunView], total_bytes, closable_bytes }`; `runs.close_finished { run_ids }` → `CloseFinishedResult { closed: [run_id], skipped: [SkippedRunView], freed_bytes }`. `runs.storage` → `[RunStorageView { run_id, key, title, run_state, card_status, bytes, updated_at_ms }]` untuk R9 (penutupan per baris memakai `runs.close` yang ada). Klien VM memakai RPC yang sama ke daemon VM.

### Implementation Units

### U1. Aturan selesai di domain

- **Goal:** keputusan selesai dan boleh ditutup tanpa I/O.
- **Requirements:** R1, R2.
- **Files:** `crates/mandor-domain/src/cleanup.rs` (baru), `crates/mandor-domain/src/lib.rs`.
- **Test scenarios:** Delivered dan Waiting Deploy selesai, status lain tidak; grup selesai hanya bila semua kartu yang disertakan selesai (kartu dikeluarkan diabaikan); run bekerja (Running, Reviewing, Publishing, WfRunning) → Skip "Working"; tidak bersih → Skip "Unpushed changes".

### U2. Usecase, port, dan setelan

- **Goal:** daftar kandidat, penutupan sekaligus, dan penutupan otomatis.
- **Requirements:** R2, R3, R6, R8; KTD-C2–C4.
- **Files:** `crates/mandor-usecase/src/run/cleanup.rs` (baru), `crates/mandor-usecase/src/run_ports.rs` (`GitPort::dir_size`), `crates/mandor-git/src/` (adapter), `crates/mandor-store/src/` (setelan `cleanup`), `crates/mandor-usecase/tests/cleanup.rs` (baru), fake di `tests/common`.
- **Test scenarios:** AE1, AE2, AE4 lewat usecase; daftar storage semua run terbuka urut ukuran (AE7); pemeriksaan ulang saat konfirmasi (kartu kembali ke Waiting Fix setelah dialog dibuka → dilewati); AE3 otomatis dengan setelan menyala, tidak berjalan bila mati, maksimal sekali per 10 menit; AE6 (Waiting Fix setelah ditutup membuat run baru); ukuran gagal diukur tidak menghalangi penutupan.

### U3. Protokol dan dispatch

- **Goal:** RPC untuk app laptop dan VM.
- **Requirements:** R4–R7; KTD-C5.
- **Files:** `crates/mandor-protocol/src/{dto.rs,lib.rs}`, `crates/mandord/src/dispatch/cleanup.rs` (baru), `crates/mandord/src/session.rs` (pemeriksaan otomatis), TS diregenerasi.
- **Test scenarios:** DTO bolak-balik; `close_finished` dengan id run yang tidak dikenal diabaikan dan dilaporkan; setelan get/set.

### U4. UI

- **Goal:** tombol di kolom Done, Settings → Storage, tombol di tab Overnight, setelan VM.
- **Requirements:** R4, R5, R7.
- **Files:** `apps/desktop/src/lib/cleanup.ts` (baru) dan test-nya, `apps/desktop/src/lib/components/kanban/Board.svelte`, `apps/desktop/src/lib/components/kanban/CloseFinishedDialog.svelte` (baru), `apps/desktop/src/lib/components/settings/StorageSettings.svelte` (baru), `apps/desktop/src/lib/components/settings/Settings.svelte`, `apps/desktop/src/lib/components/settings/VmSettings.svelte`, `apps/desktop/src/lib/components/overnight/OvernightPage.svelte`, `apps/desktop/src/lib/dev/mock-transport.ts`, prototype `docs/design/mandor-design/mandor-design-handoff/prototype/js/components-sheet.js` (diperbarui lebih dulu).
- **Test scenarios:** label tombol ("Close 1 finished run" / "Close N finished runs", tersembunyi bila 0); format ukuran (KB/MB/GB); ringkasan hasil; dialog menampilkan run yang dilewati beserta alasan; daftar storage urut ukuran dengan aksi Open dan Close run (AE7).

### U5. Runbook dan uji langsung

- **Goal:** panduan dan bukti.
- **Files:** `README.md` (bagian storage), `docs/progress/close-finished-runs.md` (baru).
- **Test scenarios:** AE1 di laptop dengan run uji lama; AE3 dengan setelan otomatis; AE5 di VM; AE7 dengan run kelola-v2 yang besar.

### Sequencing

U1 → U2 → U3 → U4 → U5.

---

## Verification Contract

- `make check` hijau.
- `bun run --cwd apps/desktop check` dan `test` hijau.
- Test `cleanup.rs` (domain dan usecase) lulus untuk AE1–AE4, AE6, dan AE7.
- Uji langsung U5 tercatat di `docs/progress/close-finished-runs.md`.

## Definition of Done

- U1–U5 selesai dengan skenario test terpenuhi, gerbang hijau, dan uji langsung tercatat; PR dibuka tanpa merge sendiri.

## Appendix

### Risks

- Menghapus kerja yang belum di-push: dicegah R2 (pemeriksaan bersih dan tanpa commit belum di-push, diulang saat konfirmasi).
- Mengukur folder besar memakan waktu: dibatasi timeout dan dijalankan di thread blocking (KTD-C3).
