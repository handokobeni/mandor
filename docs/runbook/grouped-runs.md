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

## 6. Uji langsung (U11)

Di Base salinan (`Sv1NbPJh5aD7SesHvtGlmga8g2f`) dengan repo `handokobeni/mandor-smoke`:

1. Buat 3–4 kartu uji To Do dalam satu epic uji, PIC Anda, readiness siap, masing-masing dengan 1–2 AC.
2. **F1**: Start together semua kartu, Next card sampai selesai, Push & PR. Cek satu PR, judul rentang kunci, dan semua kartu Waiting Review dengan link sama.
3. **F3**: jawab pertanyaan agent di kartu ketiga lalu lanjutkan.
4. **F4**: pindah satu kartu ke Waiting Fix dengan komentar; cek perbaikan di PR grup, peringatan "Do not merge", dan hanya kartu itu yang kembali ke Waiting Review.
5. **AE8**: ulangi grup baru, pindah kartu pertama ke status lain setelah Next card, lalu Push & PR; cek dialog kartu berubah, keep dengan alasan, dan revert.

Catat hasil di `docs/progress/grouped-runs.md`.
