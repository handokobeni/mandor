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
- Temuan uji langsung AE9 (2026-10-30): id workspace laptop (`work-smoke`) dan VM (`mandor-smoke-test`) berbeda, popover diam-diam memilih repo `mandor-smoke` dan VM menjalankan plan workspace sebagai satu repo. Perbaikan: paket plan membawa `repos` run laptop; VM menolak target dengan repo berbeda ("this plan covers …; queue it to a VM target with the same repos"); popover memilih workspace VM yang memuat semua repo run (id boleh beda) dan memblok target yang tidak cocok. Paket dari laptop lama (tanpa `repos`) tetap diterima.

## Uji langsung (U8)

Menunggu biner baru dipasang di VM oleh pemilik produk. Kartu uji di Base salinan `Sv1NbPJh5aD7SesHvtGlmga8g2f`, repo `mandor-smoke`.

| AE | Skenario | Hasil |
|---|---|---|
| AE1 | Approve & queue for VM, VM mulai dari plan malam hari | Lulus 2026-10-30 (PGS-1535, dalam jendela 22:00–00:00): branch ter-push, klaim `~handoko_ben-vm`, run laptop ditutup; VM langsung "Started: implement the approved plan." tanpa giliran plan, agent membuka `/srv/mandor/handoko_ben/design/mandor-smoke-design/kata`, selesai dengan draft PR mandor-smoke#17. PGS-1536 juga dimulai dari plan. |
| AE2 | VM menolak antrean, run laptop tetap | Lulus 2026-10-30 (PGS-1533): folder desain VM disembunyikan setelah popover dibuka → "rejected: Design source "mandor-smoke-design" is not available: Path missing"; run tetap `awaiting_dev`, klaim tetap `~handokobeni-laptop`. Branch sudah ter-push oleh langkah 1 (tanpa commit baru, sama dengan main), sesuai KTD. Catatan: mematikan service VM tidak mensimulasikan VM mati karena `mandord connect` menyalakan daemon lagi; percobaan itu (PGS-1535) justru antre sukses. |
| AE3 | Take over kartu Planned siang hari, approve di laptop | Lulus 2026-10-30 (PGS-1537, di luar jendela): Take over to laptop → antrean VM kosong, klaim kembali `~handokobeni-laptop`; run laptop baru membawa plan identik (hash sama dengan run lama); "Plan approved; implement it" langsung implement dan membaca desain `pace/`. |
| AE4 | Klaim Planned tetap segar 6 jam | |
| AE5 | Desain repo git di-pull dan dikirim ke agent | Lulus 2026-10-30 (PGS-1539, No plan): commit "Desain bensin v2" di-push; VM fast-forward pukul 16:03:22 UTC, run dimulai 16:03:29 UTC (7 detik kemudian); agent langsung membuka `/srv/mandor/handoko_ben/design/mandor-smoke-design/bensin`. |
| AE6 | Pull gagal, kartu tetap jalan, dicatat | Lulus 2026-10-30 (PGS-1540, No plan): remote desain VM diarahkan ke repo yang tidak ada; kartu tetap dimulai dengan salinan lama; run mencatat "Design mandor-smoke-design not updated: fetch failed: remote: Repository not found. fatal: repository <remote> not found" (URL disamarkan). Teks di laporan pagi dicek setelah laporan terbit. |
| AE7 | Folder desain hilang, Planned ditahan, Take over | Lulus 2026-10-30 (PGS-1541): folder desain VM diganti nama, Start now → item `held` dengan alasan "Design source "mandor-smoke-design" is not available: Path missing", tidak ada run VM, klaim tetap `~handoko_ben-vm`; Take over → antrean kosong, klaim `~handokobeni-laptop`, run laptop baru `awaiting_dev` dengan plan identik (hash sama). |
| AE8 | Follow design tanpa desain VM: Queue nonaktif | Lulus 2026-10-30 (PGS-1533): AC-4 Follow design, sumber desain VM tidak dipilih, Queue nonaktif dengan "Some AC follow the design. Pick a design source on the VM." |
| AE9 | Take over kartu workspace | Lulus 2026-10-31 (PGS-1544, setelah perbaikan `5f42d15` dan `3c8bd91`): Approve & queue for VM memilih workspace VM `mandor-smoke-test` (laptop `work-smoke`), VM mengerjakan dua worktree; setelah Waiting Fix, Take over membuat run laptop baru pada workspace `work-smoke` dengan worktree `mandor-smoke` dan `mandor-smoke-api` berisi commit VM (`e936533`, `aa44314`) dan sinkron dengan branch remote. Run masuk Waiting Fix laptop (`wf_quiet`), sesuai status kartu. |
| AE10 | Berkas liar menahan pengiriman | Lulus 2026-10-30 (PGS-1534): `notes.txt` tak di-ignore → "Commit or remove these new files before queueing for the VM: notes.txt"; branch tidak ada di remote, tidak ada commit baru, run tetap `awaiting_dev` |

### Temuan lain saat uji langsung (belum diperbaiki)

- PGS-1543 (Take over sebelum perbaikan `3c8bd91`): klaim sudah pindah ke laptop tetapi run laptop tidak dibuat; mesin Waiting Fix laptop lalu membuat run satu repo (`mandor-smoke`) untuk kartu workspace itu. Jalur ini tidak terjadi lagi setelah perbaikan, tetapi Take over sebaiknya bisa diulang bila langkah laptop gagal.
- Kartu workspace menyimpan dua URL PR dipisah baris di `mandor_pr`; pembaruan badan PR laptop gagal dengan "gh pr view failed: invalid qualified head ref format" karena kedua URL dibaca sebagai satu.
- Diagram status di plan VM (2026-10-23-002) menggambarkan draft PR bisa di-Take over, sedangkan R22 dan UI hanya untuk Waiting Fix, tertahan, atau gagal. Perlu keputusan pemilik produk.

