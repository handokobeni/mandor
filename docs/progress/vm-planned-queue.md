# Progres: plan siang, lembur malam di VM

Plan: `docs/plans/2026-10-30-001-feat-vm-planned-queue-plan.md`. Branch: `feat/vm-planned-queue`.

| Unit | Status | Commit |
|---|---|---|
| U1 Tipe domain kartu Planned | Selesai | `6b39ba7` |
| U2 Antrean menyimpan paket plan | Selesai | `e9e35fa` |
| U3 Laptop: siapkan dan kirim ke VM | Selesai | `df51680` |
| U9 Take over workspace dan paket plan | Selesai | `d09628a` |
| U4 VM: terima, jaga, mulai, kembalikan | Belum | |
| U5 Sumber desain di antrean VM | Belum | |
| U6 Protokol dan RPC | Belum | |
| U7 UI | Belum | |
| U8 Runbook dan uji langsung | Belum | |

## Catatan keputusan saat implementasi

- Heartbeat item Planned memperbarui klaim bila klaim masih dipegang daemon VM ini, termasuk klaim yang sudah basi tetapi tidak diambil siapa pun (baca dulu, tulis hanya bila pemegangnya VM). Klaim yang sudah dipegang daemon lain tidak pernah ditulis.
- `approve_plan` mengambil daftar AC Follow design dari baris berstatus Follow design setelah pilihan diterapkan, sehingga run hasil Take over membawa keputusan dari paket plan.
- Skill plan Mandor tidak menulis berkas (prompt melarangnya), jadi commit di `prepare_for_vm` biasanya kosong; push tetap dilakukan agar branch ada di remote.
