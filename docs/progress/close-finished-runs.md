# Progres: tutup run kartu yang sudah selesai

Plan: `docs/plans/2026-11-03-001-feat-close-finished-runs-plan.md`. Branch `feat/close-finished-runs` dari `main`.

| Unit | Status | Catatan |
|---|---|---|
| U1 | selesai | `mandor_domain::cleanup`: `card_finished` (Delivered, Waiting Deploy), `finished` (semua kartu disertakan), `close_decision` (`Working` untuk Preparing/Running/Reviewing/Publishing/WfRunning, `UnpushedChanges` bila tidak bersih). 3 test. |
| U2 | selesai | `run/cleanup.rs`: `finished_runs`, `close_finished` (periksa ulang lalu `close_run`), `auto_close_finished` (setelan menyala, paling sering 10 menit, notifikasi `RunsClosed` "Closed N finished runs, freed X"), `run_storage` (semua run terbuka urut ukuran), setelan `cleanup` di tabel `settings`. Port `GitPort::dir_size` (adapter `mandor-git/src/size.rs`: jalan direktori di `spawn_blocking`, batas 60 detik, tanpa mengikuti symlink). 7 test usecase (`tests/cleanup.rs`: AE1, AE2, AE3, AE4, AE7, periksa ulang saat konfirmasi, ukuran gagal), 2 test adapter, 1 test teks. AE6 sudah dicakup `waiting_fix.rs::app_mati_saat_waiting_fix_membuat_ulang_worktree_dan_sesi_baru_dengan_konteks` (Waiting Fix tanpa run membuat ulang worktree dari branch kartu). |
| U3 | selesai | Method `runs.finished`, `runs.close_finished` (ringkasan "Closed N finished runs, freed X"), `runs.storage`, `settings.cleanup.get/set` di `dispatch/cleanup.rs`; DTO `FinishedRunView`, `SkippedRunView`, `FinishedRunsView`, `CloseFinishedParams`, `CloseFinishedResult`, `RunStorageView`, `CleanupSettingsView`; TS diregenerasi. Loop sesi memanggil `auto_close_finished` tiap 60 detik (usecase membatasi 10 menit) dan mengirim `board.updated` bila ada yang ditutup; blok Waiting Fix dipindah ke `fix_once` agar `run_due` tetap di bawah batas kompleksitas. Test DTO `mandor-protocol/tests/cleanup_dto.rs`, id run tak dikenal diabaikan. |
| U4 | selesai | Prototype "Close finished runs" (header Done, dialog, Settings › Storage, toast) dibuat lebih dulu. `lib/cleanup.ts` (label tombol, ukuran, alasan, label konfirmasi) + `CleanupStore` (`cleanup` laptop, `vmCleanup` VM; muat ulang saat `board.updated`/`run.updated`). `CloseFinishedDialog` dipakai bersama; kolom Done menampilkan "Close N finished runs" (ikon saja saat kolom tertutup); Settings › Storage untuk target Laptop dan VM (ringkasan ukuran, setelan otomatis, tombol, daftar semua run urut ukuran dengan Open dan Close run); tab Overnight menampilkan "Storage on the VM" bila ada run VM selesai. Jenis notifikasi `runs_closed` (default tidak muncul di desktop). Mock `/?mock` dicek di browser: tombol, dialog (3 ditutup, 1 dilewati "Unpushed changes"), toast, halaman Storage. 10 test Vitest baru. |
| U5 | selesai | README "Mengosongkan storage worktree". Uji langsung AE1/AE2/AE7 (laptop, manual), AE3 (laptop, otomatis), dan AE5 (VM, otomatis) lolos; lihat tabel di bawah. |

### Temuan uji langsung dan perbaikan (2026-11-03)

| Temuan | Tindakan |
|---|---|
| Dialog melebar dan terpotong oleh kunci grup panjang | `keyLabel` ("PGS-1513 +3", daftar lengkap di tooltip); baris dibatasi lebar dialog; status kartu grup tidak berulang di Storage. |
| "Worktrees: 923 MB" terbaca seolah-olah yang dihapus | Teks menjadi "All worktrees: …". |
| Gagal memuat ditampilkan sebagai "0 B" dan "No open runs" | Halaman menampilkan "Could not read storage" dan Try again. |
| Menutup 17 run butuh lebih dari satu menit (melepas klaim di Lark kena rate limit, ~3–5 detik per run) sehingga app melaporkan failed to fetch | `runs.close_finished` menjawab langsung (`CloseFinishedStarted { count }`) dan daemon menutup di latar belakang (`start_close_finished`, satu penutupan sekaligus, lewat `TaskTracker`); hasil datang sebagai notifikasi `runs_closed`; app menampilkan "Closing…" lalu toast ringkasan. Penutupan otomatis juga di latar belakang agar loop sinkron tidak tertahan. |

Uji B (sebelum perbaikan latar belakang): 17 run uji Delivered ditutup dan 17 folder worktree terhapus; PGS-1512, PGS-1537, PGS-1544, PGS-1546+2 dilewati "Unpushed changes" dan worktree-nya tetap; run kelola-v2 (kartu In Progress/To Do) tidak disentuh. Uji A: Settings › Storage menampilkan PGS-1401 (845 MB) paling atas.

### Uji langsung setelah perbaikan latar belakang (2026-11-03)

| Skenario | Hasil |
|---|---|
| AE3 (otomatis) | Lolos. Kartu uji PGS-1572 dikerjakan di laptop sampai PR https://github.com/handokobeni/mandor-smoke/pull/33 (worktree bersih), setelan otomatis dinyalakan, kartu dipindah ke Delivered 16:44:28; 16:44:40 daemon memulai penutupan otomatis (1 run), 16:44:42 run tertutup, worktree terhapus, notifikasi `runs_closed` "Closed 1 finished run, freed 66.4 KB". |
| AE5 (VM, otomatis) | Lolos. Biner VM `0ccc47b8…` dari branch ini, setelan otomatis dinyalakan dari target VM di Settings → Storage. Sebelum: 23 run terbuka. 09:49:04 UTC daemon VM memulai penutupan otomatis (22 run); 09:50:28 selesai, 21 run tertutup dan worktree-nya terhapus, notifikasi `runs_closed` "Closed 21 finished runs, freed 1.4 MB" (kecil karena sebagian besar folder worktree run uji VM sudah tidak ada). Dilewati: PGS-1554+1555 (2 commit belum di-push, R2). PGS-1561+1562 baru masuk cache VM sebagai Delivered setelah pemeriksaan pertama, lalu tertutup pada pemeriksaan otomatis berikutnya 09:59:34 (jeda 10 menit, KTD-C4). Sisa 1 run terbuka (PGS-1554+1555). |
| Catatan | Folder worktree yang sudah hilang memunculkan peringatan `ukuran worktree tidak terbaca` berulang setiap daftar Storage dimuat. Diperbaiki: `dir_size` menghitung folder yang tidak ada sebagai 0 B tanpa peringatan; error lain (izin, batas waktu) tetap dilaporkan. Test adapter `folder_yang_sudah_hilang_berukuran_nol`. |
