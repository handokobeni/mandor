# Progres: plan siang, lembur malam di VM

Plan: `docs/plans/2026-10-30-001-feat-vm-planned-queue-plan.md`. Branch: `feat/vm-planned-queue`.

| Unit | Status | Commit |
|---|---|---|
| U1 Tipe domain kartu Planned | Selesai | `6b39ba7` |
| U2 Antrean menyimpan paket plan | Selesai | `e9e35fa` |
| U3 Laptop: siapkan dan kirim ke VM | Selesai | `df51680` |
| U9 Take over workspace dan paket plan | Selesai | `d09628a` |
| U4 VM: terima, jaga, mulai, kembalikan | Selesai | `f199b4d` |
| U5 Sumber desain di antrean VM | Selesai | `353b520` |
| U6 Protokol dan RPC | Selesai | `429c79f` |
| U7 UI (prototype dan app) | Selesai | `39d81a2` |
| U8 Runbook | Selesai | (commit ini) |
| U8 Uji langsung AE1–AE10 di VM | Menunggu pemilik produk | |

## Catatan keputusan saat implementasi

- Heartbeat item Planned memperbarui klaim bila klaim masih dipegang daemon VM ini, termasuk klaim yang sudah basi tetapi tidak diambil siapa pun (baca dulu, tulis hanya bila pemegangnya VM). Klaim yang sudah dipegang daemon lain tidak pernah ditulis.
- `approve_plan` mengambil daftar AC Follow design dari baris berstatus Follow design setelah pilihan diterapkan, sehingga run hasil Take over membawa keputusan dari paket plan.
- Skill plan Mandor tidak menulis berkas (prompt melarangnya), jadi commit di `prepare_for_vm` biasanya kosong; push tetap dilakukan agar branch ada di remote.
- Pesan desain untuk giliran implementasi memakai `design_reference_message` (acuan tampilan), bukan `design_message` tahap plan yang meminta mengisi kolom Design lewat tool ledger. Dipakai untuk plan yang disetujui (VM dan setelah Take over) dan Start tanpa plan di VM. Start Skip di laptop tidak berubah (desain hanya untuk reviewer).
- `takeover` menerima `PlannedWork` langsung dan me-resolve sumber desain laptop lewat port `DesignSources`; sumber yang hilang dicatat di chat run.
- Folder desain hanya di-pull bila `.git` dimiliki user yang sama dengan `~/.mandor` daemon.

## Uji langsung (U8)

Menunggu biner baru dipasang di VM oleh pemilik produk. Kartu uji di Base salinan `Sv1NbPJh5aD7SesHvtGlmga8g2f`, repo `mandor-smoke`.

| AE | Skenario | Hasil |
|---|---|---|
| AE1 | Approve & queue for VM, VM mulai dari plan malam hari | |
| AE2 | VM menolak antrean, run laptop tetap | |
| AE3 | Take over kartu Planned siang hari, approve di laptop | |
| AE4 | Klaim Planned tetap segar 6 jam | |
| AE5 | Desain repo git di-pull dan dikirim ke agent | |
| AE6 | Pull gagal, kartu tetap jalan, dicatat | |
| AE7 | Folder desain hilang, Planned ditahan, Take over | |
| AE8 | Follow design tanpa desain VM: Queue nonaktif | |
| AE9 | Take over kartu workspace | |
| AE10 | Berkas liar menahan pengiriman | |
