# Progres: mode lembur di VM

Plan: `docs/plans/2026-10-23-002-feat-vm-overnight-mode-plan.md`. Branch `feat/vm-overnight-mode` dari `main` (setelah PR #12). Urutan mengikuti Sequencing plan: U1 → U2 → U4 → U13 → U3 → U14 → U5 → U6 → U7 → U8 → U11 → U9 → U10 → U12.

Syarat rollout ke tim (bukan blocker implementasi): konfirmasi tertulis Anthropic bahwa login langganan Claude Code developer boleh berjalan tanpa didampingi di VM bersama.

| Unit | Status | Catatan |
|---|---|---|
| U1 | selesai | `ClaimIdentity` dan `Holder` di `mandor-domain/src/claim.rs`: nonce klaim `<acak>~<daemon_id>` (id disaring dari `|` dan `~`), pembaca lama tetap bisa mengurai. `may_claim` menolak klaim aktif daemon lain milik dev yang sama (`OwnOtherDaemon`) sampai basi. `DaemonRole` (`role.rs`). `Task::held_here`/`holder`; `revalidate`, poller (lepas klaim saat kembali ke To Do, cermin kanban), heartbeat, `release_claim`, dan `fix_candidates` memakai pemegang daemon. `MANDOR_ROLE` dan `MANDOR_DAEMON_ID` (default `<user>@<HOSTNAME>` atau `<user>-<peran>`), `SyncService::with_daemon`. `CardView.holder`; kartu yang dipegang daemon lain hanya-baca (Start dan ambil alih nonaktif). |

### Penyimpangan dari plan

- Klaim lama tanpa id daemon: plan (KTD-55) menyebutnya milik daemon tak dikenal. Daemon **laptop** menganggap klaim lama atas `open_id`-nya sebagai miliknya dan mengadopsinya (nonce baru dengan id daemon) pada heartbeat berikutnya, agar run yang berjalan sebelum upgrade tidak kehilangan klaim. Daemon **VM** mengikuti plan: klaim lama ditolak sampai basi.
- Penolakan link VM untuk daemon laptop yang belum mendukung klaim per daemon (dari versi protokol `hello`) dikerjakan bersama koneksi SSH (U9).
