# Progres: tutup run kartu yang sudah selesai

Plan: `docs/plans/2026-11-03-001-feat-close-finished-runs-plan.md`. Branch `feat/close-finished-runs` dari `main`.

| Unit | Status | Catatan |
|---|---|---|
| U1 | selesai | `mandor_domain::cleanup`: `card_finished` (Delivered, Waiting Deploy), `finished` (semua kartu disertakan), `close_decision` (`Working` untuk Preparing/Running/Reviewing/Publishing/WfRunning, `UnpushedChanges` bila tidak bersih). 3 test. |
| U2 | selesai | `run/cleanup.rs`: `finished_runs`, `close_finished` (periksa ulang lalu `close_run`), `auto_close_finished` (setelan menyala, paling sering 10 menit, notifikasi `RunsClosed` "Closed N finished runs, freed X"), `run_storage` (semua run terbuka urut ukuran), setelan `cleanup` di tabel `settings`. Port `GitPort::dir_size` (adapter `mandor-git/src/size.rs`: jalan direktori di `spawn_blocking`, batas 60 detik, tanpa mengikuti symlink). 7 test usecase (`tests/cleanup.rs`: AE1, AE2, AE3, AE4, AE7, periksa ulang saat konfirmasi, ukuran gagal), 2 test adapter, 1 test teks. AE6 sudah dicakup `waiting_fix.rs::app_mati_saat_waiting_fix_membuat_ulang_worktree_dan_sesi_baru_dengan_konteks` (Waiting Fix tanpa run membuat ulang worktree dari branch kartu). |
