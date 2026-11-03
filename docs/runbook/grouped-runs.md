# Runbook: run grup kartu satu flow

Panduan mengerjakan beberapa kartu Lark yang saling terkait (satu epic, flow milik Anda) sebagai satu run, satu branch, dan satu PR. Plan: `docs/plans/2026-10-23-004-feat-grouped-card-runs-plan.md`. Bagian VM lembur (U10) belum tersedia; panduan ini untuk laptop.

## 1. Syarat kartu

Kartu bisa dipilih untuk grup bila:

- epic-nya sama dengan kartu awal (field **Epic**),
- Anda PIC-nya,
- statusnya To Do dan lolos gerbang Start (Development Readiness siap),
- belum punya run lain.

Kartu yang tidak memenuhi syarat tetap tampil di daftar dengan alasannya. Grup lintas orang tidak didukung; kartu yang menunggu kartu orang lain sebaiknya tidak dimasukkan.

## 2. Memulai grup

1. Klik **Start** pada salah satu kartu flow.
2. Di bagian **Start together**, centang kartu lain. Urutan centang menjadi urutan pengerjaan; ubah dengan panah naik/turun.
3. Pilih repo atau workspace, Planning (Plan atau Skip, berlaku untuk setiap kartu), sumber desain, dan skill reviewer seperti Start biasa.
4. Tombol menjadi **Start N cards**.

Mandor mengklaim semua kartu, membuat satu branch `mandor/<kunci-awal>-<nomor-akhir>-<epic>`, memindahkan semua kartu ke In Progress, lalu agent mulai dari kartu pertama dengan konteks seluruh flow dan AC kartu itu saja.

## 3. Pindah kartu

Setelah giliran kartu selesai (state **Waiting for you**), panel **Group** di workspace menampilkan setiap kartu: Waiting, Working, Done, atau Removed.

- **Next card**: commit lokal semua perubahan dengan pesan `<KEY>: …` (tanpa push), kartu menjadi Done (kartu tanpa perubahan tetap Done tanpa commit, jadi jawab dulu pertanyaan agent sebelum menekannya), lalu agent mulai kartu berikutnya di sesi yang sama (plan dulu bila grup memakai Plan).
- **Finish card**: sama untuk kartu terakhir; setelah itu Push & PR tersedia.
- Sebelum memulai kartu berikutnya, Mandor memeriksa kartu itu di Lark. Bila statusnya bukan lagi In Progress atau PIC-nya bukan Anda, kartu dikeluarkan (klaim dilepas, status tidak diubah) dan grup berhenti dengan catatan. Pilih **Continue with …** atau **Stop group**.
- **Remove** pada kartu yang belum dimulai mengeluarkannya; kembalikan kartu itu ke To Do di Lark.
- **Stop group** (menu ⋯) push commit kartu yang sudah selesai tanpa PR, menutup run, dan melepas klaim semua kartu.

## 4. Push & PR

1. **Push & create PR** memeriksa ulang semua kartu Done di Lark. Kartu yang berubah setelah di-commit menahan gerbang dan dialog **A card changed in Lark** muncul:
   - **Revert its commit**: commit kartu itu di-revert, kartu keluar dari grup, klaimnya dilepas, dan verdict AC menjadi usang (review ulang). Bila revert konflik, revert dibatalkan, branch tidak berubah, dan gerbang tetap tertahan dengan catatan "revert conflict".
   - **Keep in the PR**: wajib alasan; alasan dicatat di deskripsi PR dan status kartu itu tidak diubah. Bila kartu itu sudah dikembalikan ke To Do, klaimnya sudah dilepas, sehingga Mandor tidak menulis link PR ke kartu itu; link dan alasannya tetap ada di deskripsi PR.
2. Review AC berjalan sekali untuk AC semua kartu. Baris AC diberi label kartunya (mis. `PGS-1403 AC-3`) dan tab AC review dikelompokkan per kartu.
3. PR berjudul `<KEY-awal>–<KEY-akhir>: <nama epic>` dengan ringkasan AC per kartu, kartu yang dikeluarkan, dan alasan kartu yang tetap disertakan. Workspace multi-repo membuat satu PR per repo yang berubah.
4. Semua kartu yang tidak berubah pindah ke Waiting Review bersama dan membawa link PR yang sama.

## 5. Waiting Fix

- Bila QA memindah satu atau beberapa kartu ke Waiting Fix, perbaikan dikerjakan di branch dan PR grup. Jeda tenang dan aturan penulis tepercaya (termasuk Feedback QA) berlaku per kartu.
- Kartu yang siap diperbaiki digabung dalam satu giliran; komentar diberi label kartunya. Kartu yang masuk Waiting Fix saat giliran berjalan ikut giliran berikutnya.
- Review setelah perbaikan mencakup AC semua kartu. Keputusan lama (Push anyway, Checked manually) tetap berlaku bila verdict-nya tidak berubah; baris yang berubah menjadi Fail menahan push.
- Hanya kartu yang diperbaiki yang berpindah Waiting Fix → In Progress → Waiting Review.
- Selama ada kartu Waiting Fix, deskripsi PR memuat **Do not merge: <kartu> is in Waiting Fix**. Mandor tidak memblokir merge di GitHub.
- Bila PR grup sudah merged, perbaikan kartu itu pindah ke run lanjutan dari branch default (PR baru) dan tidak di-push ke branch grup.

## 5a. Satu plan untuk seluruh grup

Di dialog Start, bila memilih dua kartu atau lebih dengan Planning **Plan**, centang **One plan for all cards**:

- Agent membuat satu plan yang mencakup AC semua kartu, satu bagian per kartu. Anda menyetujuinya sekali.
- Kartu tetap dikerjakan dan di-commit satu per satu mengikuti bagiannya, tanpa giliran plan per kartu. Aturan lain (berhenti saat tersangkut, kartu dikeluarkan, Keep/Revert, Waiting Fix, laporan) tetap per kartu.
- Sebelum setiap kartu dimulai, Mandor membaca ulang AC kartu yang belum selesai. Bila ada AC yang ditambah, diubah, atau dihapus, agent merevisi plan untuk kartu yang tersisa dan Anda menyetujuinya ulang. Perubahan AC pada kartu yang sudah di-commit tidak membatalkan plan; gerbang AC yang menanganinya.
- **Approve & queue for VM** juga bisa dipakai untuk plan grup sebelum kartu pertama dikerjakan. VM mengerjakan semua kartu dari plan itu. Bila AC berubah di VM, grup diparkir **Group plan outdated**; ambil alih ke laptop, lalu **Continue** untuk revisi plan.

## 6. Grup di VM lembur

Grup bisa dikerjakan VM lembur (lihat `docs/runbook/setup-vm.md`) sebagai satu item antrean.

**Mengantre grup.**
- **Tanpa plan:** tab **Overnight → Queue**, pilih kartu pertama, repo VM, dan sumber desain VM (dipilih sekali untuk seluruh grup). Di **Queue together**, centang kartu lain epic itu lalu **Queue N cards together**. Kartu yang sudah di antrean VM tampil nonaktif "Queued for VM".
- **Dengan plan (kartu pertama diplan siang):** Start together di laptop, lalu di panel plan kartu pertama pilih **Approve & queue for VM**. Tombol ini hanya ada selama kartu pertama masih di fase plan. Laptop mem-push branch, VM mengantre seluruh grup dengan plan kartu pertama, lalu klaim semua kartu pindah ke VM (kartu pertama paling akhir).

Baris antrean grup menampilkan **Group of N** dan urutan kartunya. Semua kartu grup di papan bertanda "Queued for VM".

**Cara VM mengerjakan grup.**
- Kartu dikerjakan berurutan di satu branch tanpa persetujuan plan. Tiap kartu di-commit lokal saat gilirannya selesai.
- Giliran yang berakhir **tanpa perubahan kode** dianggap buntu: grup diparkir dengan pertanyaan agent di laporan pagi.
- Sebelum kartu berikutnya, VM berjeda bila jam lembur sudah berakhir (dan grup tidak Start now) atau ada Waiting Fix lain yang siap. Grup kembali di posisi pertama antrean ("Continues") dan lanjut siklus berikutnya tanpa notifikasi.
- Kartu berikutnya yang berubah di Lark (dipindah, PIC diganti) dikeluarkan dari grup dan klaimnya dilepas; grup diparkir "Card changed in Lark". Di laporan, **Answer and queue** mengantre grup lagi tanpa kartu itu.
- Setelah kartu terakhir: kartu ter-commit diperiksa ulang di Lark. Yang berubah menahan push sampai diputuskan (lihat Laporan pagi). Selain itu review AC sekali, lalu satu PR draft grup.

**Laporan pagi.** Run grup tampil satu baris per kartu dengan keadaannya (Committed, In progress, Not started, Removed). Aksi run (Send to review, Take over to laptop, jawaban untuk agent, keputusan AC) ada di baris pertama grup. Kartu bertanda **Changed in Lark** diputuskan dari laporan: **Keep** dengan alasan (dicatat di PR) atau **Revert** commit kartu itu; setelahnya grup kembali ke antrean. Revert yang konflik membiarkan grup tertahan; ambil alih ke laptop untuk menyelesaikannya.

**Take over grup.** **Take over to laptop** memindahkan seluruh grup: klaim semua kartu, branch, commit per kartu, PR draft (Send to review dari laptop), dan hasil review AC VM. Kartu yang tersisa dilanjutkan di laptop dengan plan per kartu.

## 7. Uji langsung (U11)

Di Base salinan (`Sv1NbPJh5aD7SesHvtGlmga8g2f`) dengan repo `handokobeni/mandor-smoke`:

1. Buat 3–4 kartu uji To Do dalam satu epic uji, PIC Anda, readiness siap, masing-masing dengan 1–2 AC.
2. **F1**: Start together semua kartu, Next card sampai selesai, Push & PR. Cek satu PR, judul rentang kunci, dan semua kartu Waiting Review dengan link sama.
3. **F3**: jawab pertanyaan agent di kartu ketiga lalu lanjutkan.
4. **F4**: pindah satu kartu ke Waiting Fix dengan komentar; cek perbaikan di PR grup, peringatan "Do not merge", dan hanya kartu itu yang kembali ke Waiting Review.
5. **AE8**: ulangi grup baru, pindah kartu pertama ke status lain setelah Next card, lalu Push & PR; cek dialog kartu berubah, keep dengan alasan, dan revert.

Catat hasil di `docs/progress/grouped-runs.md`.

## 8. Uji langsung grup di VM (U17)

Di Base salinan dengan repo `handokobeni/mandor-smoke` di VM dan sumber desain `mandor-smoke-design`. Biner VM dipasang pemilik produk.

1. Buat 3–4 kartu uji To Do baru dalam satu epic uji, PIC Anda, readiness siap.
2. **F2/AE6**: Queue together tanpa plan, Start now. Cek commit per kartu, satu PR draft grup, laporan per kartu, lalu Send to review grup.
3. **F5/AE9**: Start together di laptop, Approve & queue for VM di kartu pertama. Cek klaim semua kartu pindah ke VM dan VM mulai dari plan kartu pertama.
4. **AE4 (VM)**: pindah satu kartu yang belum gilirannya ke Blocked; cek kartu dikeluarkan dan grup diparkir "Card changed in Lark" bila kartu sudah ter-commit.
5. **AE7**: pasang batas token kecil sehingga grup berhenti di kartu kedua; cek laporan.
6. **R21**: jam lembur berakhir di tengah grup; cek baris "Continues" dan lanjut siklus berikutnya.
7. **Take over grup**: ambil alih grup dengan PR draft ke laptop, lalu Send to review dari laptop.

Catat hasil di `docs/progress/grouped-runs.md`.

## 9. Uji langsung satu plan grup (U21)

1. **AE10**: Start together 3 kartu dengan One plan for all cards. Cek satu plan dengan bagian per kartu, satu persetujuan, dan commit per kartu tanpa giliran plan.
2. **AE11 laptop**: setelah kartu pertama di-commit, ubah AC kartu ketiga di Lark; tekan Next card. Cek plan direvisi dan menunggu persetujuan ulang.
3. **F6 VM**: plan grup lalu Approve & queue for VM sampai PR draft.

Catat hasil di `docs/progress/grouped-runs.md`.
