---
title: Run grup kartu satu flow - Plan
type: feat
date: 2026-10-23
topic: grouped-card-runs
artifact_contract: ce-unified-plan/v1
artifact_readiness: implementation-ready
product_contract_source: ce-brainstorm
execution: code
---

# Run grup kartu satu flow - Plan

## Goal Capsule

- **Objective:** Kartu-kartu satu flow milik satu developer di dalam satu epic dikerjakan sebagai satu run grup: satu branch, berurutan per kartu, satu PR (per repo). Grup bisa dimulai dari app laptop atau diantrekan ke VM lembur sehingga satu flow selesai semalaman.
- **Product authority:** brainstorm dengan pemilik produk dan keputusannya atas dua ronde review dokumen (coherence, feasibility, product-lens, adversarial), 2026-10-23. Melengkapi plan mode VM (`docs/plans/2026-10-23-002-feat-vm-overnight-mode-plan.md`) dan mengubah aturan Rilis 1 "satu kartu = satu run = satu branch = satu PR per repo" untuk run grup saja.
- **Open blockers:** tidak ada untuk bagian laptop (U1–U9, U11). U10 (VM) menunggu implementasi plan mode VM.
- **Product Contract preservation:** Product Contract unchanged sejak review ronde kedua.
- **Execution profile:** test-first lewat antarmuka publik (usecase dan store dengan fake yang ada); frontend dengan Vitest; uji langsung dengan kartu uji di Base salinan dan repo `mandor-smoke`.
- **Authority hierarchy:** `CLAUDE.md` (standar `rust-engineering` mengikat) > Product Contract > Planning Contract > unit.
- **Stop conditions:** berhenti dan tanya bila perilaku produk harus berubah, bila unit tidak bisa selesai tanpa melanggar `rust-engineering`, atau bila U10 dicapai sebelum plan VM terimplementasi.
- **Tail ownership:** branch fitur, PR ke `main`, tanpa merge sendiri. Bagian laptop boleh dikirim sebagai PR pertama; U10 menyusul.

---

## Product Contract

### Summary

"Start together" memilih kartu To Do satu flow (default: epic sama, PIC saya, lolos gerbang Start, tanpa run dan tidak di antrean VM; dev bisa menambah atau mengurangi). Kartu dikerjakan berurutan di satu branch (plan → kerjakan → commit per kartu), lalu satu gerbang Push & PR di akhir membuat satu PR per repo dan memindahkan semua kartu ke Waiting Review bersama. Grup bisa diantrekan ke VM lembur sebagai satu item.

### Problem Frame

Hari ini setiap kartu adalah satu run dengan branch dan PR sendiri dari branch default. Untuk flow yang kartunya saling bergantung (mis. E-PGS43 "Job & Position": PGS-1401 daftar job, 1402 kartu job, 1403 buat job, 1404 ubah dan hapus job), ini membuat:
- kartu berikutnya tidak melihat kode kartu sebelumnya sampai PR-nya di-merge;
- reviewer menerima beberapa PR kecil yang saling terkait untuk satu fitur;
- agent kehilangan konteks flow di setiap kartu;
- status kartu di Lark bergerak tidak serempak.

Antrean VM lembur sudah mengerjakan kartu berurutan, tetapi tetap satu PR per kartu dari branch default, sehingga flow yang saling bergantung tidak bisa selesai dalam satu malam.

### Requirements

**Membentuk grup**

- R1. Dari kartu To Do, dev memilih "Start together". Mandor menawarkan kartu dengan epic sama, PIC dev itu, berstatus To Do, dan lolos gerbang Start (Development Readiness siap). Kartu yang tidak lolos tampil tidak bisa dipilih beserta alasannya. Dev bisa menambah atau mengurangi kartu sebelum mulai.
- R2. Kartu yang punya run aktif, ada di antrean VM, atau dipegang daemon VM tidak bisa masuk grup. Dev menutup run itu atau mengeluarkannya dari antrean dulu bila ingin memasukkannya.
- R3. Urutan pengerjaan default mengikuti nomor kartu; dev bisa mengubah urutan sebelum mulai.
- R4. Satu grup memakai satu target repo atau workspace, dipilih saat mulai, dan selalu bercabang dari branch default.

**Menjalankan grup**

- R5. Kartu dikerjakan berurutan di satu branch grup: plan, implementasi, dan commit per kartu, lalu kartu berikutnya.
- R6. Di laptop, plan setiap kartu menunggu persetujuan dev seperti run biasa. Di VM, plan tidak menunggu persetujuan (aturan VM).
- R7. Saat grup dimulai, semua kartu grup diklaim dan pindah ke In Progress di Lark.
- R8. Bila satu kartu tersangkut (agent bertanya, batas biaya, atau batas pemakaian VM), grup berhenti di kartu itu. Kartu yang sudah selesai tetap di branch. Setelah masalahnya beres, grup melanjutkan dari kartu itu di branch yang sama.
- R9. Sebelum memulai setiap kartu, Mandor memeriksa ulang kartu itu di Lark. Kartu yang dipindah ke status lain atau berganti PIC dikeluarkan dari grup: klaimnya dilepas (hanya bila klaim itu masih milik dev ini), statusnya tidak diubah dan dev tidak diminta mengubahnya, dan grup berhenti seperti R8 karena kartu berikutnya mungkin bergantung padanya. Di laptop dev memilih melanjutkan tanpa kartu itu atau menghentikan grup; di VM grup diparkir dengan alasan "card removed". Perubahan status, PIC, atau klaim pada kartu grup yang sedang tidak dikerjakan tidak menghentikan grup seketika: kartu itu ditandai, lalu ditangani R9 sebelum gilirannya (bila belum dimulai) atau R13 di gerbang (bila sudah di-commit). Hanya kehilangan klaim pada kartu yang sedang dikerjakan yang menghentikan grup seperti R8.
- R10. Dev bisa mengeluarkan kartu yang belum dimulai dari grup yang berjalan, atau menghentikan grup. Kartu yang dikeluarkan dev atau belum dimulai saat grup dihentikan dilepas klaimnya dan app meminta dev mengembalikannya ke To Do di Lark. Kartu yang sudah di-commit tetap di branch.
- R11. UI menampilkan grup sebagai satu run dengan progres per kartu (selesai, sedang dikerjakan, menunggu, dikeluarkan).

**PR dan review**

- R12. Push & PR terjadi sekali setelah semua kartu aktif selesai, lewat satu gerbang Push & PR. Buku AC (daftar AC beserta verdict-nya) menggabungkan AC semua kartu aktif grup, dan review AC berjalan sekali untuk seluruh flow, setelah pemeriksaan R13. Bila gerbang menahan karena AC, yang tertahan seluruh grup dengan baris AC diberi label kartunya; setelah dev memutuskan baris itu, push berlanjut tanpa mengulang kartu.
- R13. Setelah kartu terakhir selesai dan sebelum review AC grup, Mandor memeriksa ulang semua kartu. Kartu yang sudah di-commit tetapi berubah di Lark (status atau PIC) tampil "changed in Lark, code already in the branch", dan dev memilih me-revert commit kartu itu atau tetap menyertakannya dengan alasan yang tercatat di PR. Di VM, grup diparkir untuk keputusan ini.
  - Revert menjalankan review AC seluruh grup pada kode setelah revert (di VM sebagai item antrean grup).
  - Revert yang konflik tidak diterapkan: grup ditahan dengan alasan "revert conflict", dan dev memilih tetap menyertakan kartu itu dengan alasan atau mengambil alih grup ke laptop.
  - Kartu yang tetap disertakan tetap ikut buku AC, deskripsi PR (R16), dan Waiting Fix (R17), tetapi Mandor tidak memindahkan statusnya (R14, R17); deskripsi PR mencatat status dan PIC-nya saat itu.
- R14. Di laptop, PR dibuat lalu semua kartu aktif pindah ke Waiting Review bersama. Dari VM, PR dibuat draft dan semua kartu tetap In Progress; satu "Send to review" pada grup menandai PR siap di-review dan memindahkan semua kartu aktif ke Waiting Review bersama.
- R15. Grup dengan target workspace membuat satu PR per repo anggota yang berubah; semua kartu grup membawa tautan PR yang sama.
- R16. Deskripsi PR mencantumkan setiap kartu grup beserta ringkasan AC dan verdict-nya per kartu, serta kartu yang dikeluarkan.

**Waiting Fix**

- R17. Bila QA memindahkan kartu grup ke Waiting Fix, perbaikan dikerjakan di branch dan PR grup yang sama. Bila beberapa kartu grup masuk Waiting Fix, semuanya ditangani dalam satu run perbaikan dengan komentar tepercaya diberi label kartunya. Tiap kartu tetap memakai jeda tenang dan aturan pemberi komentar tepercaya miliknya sendiri. Kartu yang masuk Waiting Fix saat perbaikan grup berjalan ikut sebagai giliran berikutnya di run yang sama sebelum review AC; kartu yang masuk setelah push memulai run perbaikan berikutnya. Hanya kartu yang diperbaiki yang berubah status dan kembali ke Waiting Review setelah perbaikan.
- R18. Review AC setelah perbaikan mencakup AC semua kartu aktif grup, karena kode dipakai bersama. Baris yang verdict-nya berubah menjadi Fail atau Can't verify menahan push perbaikan (diberi label kartunya, dev memutuskan), termasuk baris kartu yang tidak diperbaiki. Baris yang sudah diputuskan dev di gerbang sebelumnya dan verdict-nya tidak berubah tidak menahan lagi. Perubahan verdict di kartu yang tidak diperbaiki dicatat di deskripsi PR tanpa mengubah statusnya.
- R19. Selama kartu grup masih Waiting Fix, deskripsi PR dan kartu grup lain menampilkan peringatan "do not merge: <kartu> is in Waiting Fix". Mandor tidak memblokir merge di GitHub. Waiting Fix yang datang setelah PR grup di-merge dikerjakan sebagai run lanjutan dari branch default. Sebelum push perbaikan, Mandor memeriksa apakah PR grup sudah di-merge; bila sudah, perbaikan menjadi run lanjutan dari branch default dan tidak di-push ke branch grup.

**VM lembur**

- R20. Dari tab Overnight, dev bisa mengantrekan grup sebagai satu item antrean. Grup memakai aturan VM yang sama: satu item sekaligus per developer, pagar lembur, dan batas pemakaian per siklus. Kartu grup yang diantrekan tampil "Queued for VM" di board laptop dan tidak bisa dimulai atau diantrekan sendiri. Bila ada Waiting Fix kartu VM lain yang siap, grup mengalah di antara kartu: setelah kartu yang sedang dikerjakan selesai, perbaikan itu dikerjakan dulu, lalu grup lanjut di branch-nya.
- R21. Bila jam lembur berakhir di tengah grup, kartu yang sedang dikerjakan diselesaikan, lalu grup berhenti dengan alasan "window ended", tetap di depan antrean, dan otomatis dilanjutkan dari kartu berikutnya di awal siklus berikutnya tanpa perlu diantrekan ulang. Grup yang dimulai dengan "Start now" di luar jam lembur berjalan sampai selesai atau tersangkut, tanpa berhenti karena jam.
- R22. Grup yang diparkir (R8, R9, R12, R13) tercatat di laporan pagi dengan progres per kartu; item antrean berikutnya tetap berjalan. Grup diparkir dijawab dan diantrekan ulang, atau diambil alih ke laptop ("Take over to laptop"), sebagai satu kesatuan: klaim semua kartu grup dan branch grup berpindah bersama, dan kartu tersisa mengikuti R6 untuk laptop.
- R23. Laporan pagi dan ringkasan siklus menghitung hasil grup per kartu: kartu selesai dihitung mencapai PR draft hanya setelah PR grup dibuat; kartu lain dihitung diparkir dengan alasan grup.

### Key Flows

- F1. **Siang, laptop:** dev membuka PGS-1401, menekan "Start together", menerima 1401–1404, lalu mulai. Agent membuat plan 1401; dev menyetujui; agent mengerjakan dan commit; lanjut ke 1402 sampai 1404. Setelah 1404, gerbang Push & PR menampilkan AC keempat kartu; dev mengirim PR; keempat kartu pindah ke Waiting Review.
- F2. **Lembur, VM:** dev mengantrekan grup 1401–1404 di tab Overnight. Malam hari VM mengerjakan keempat kartu berurutan tanpa persetujuan plan, lalu membuat satu PR draft; kartu tetap In Progress. Pagi hari dev membaca laporan, melihat PR, lalu menekan "Send to review" pada grup; keempat kartu pindah ke Waiting Review.
- F3. **Tersangkut:** agent bertanya di kartu 1403. Grup berhenti; 1401–1402 sudah di branch. Dev menjawab; grup melanjutkan dari 1403.
- F4. **Waiting Fix:** QA memindahkan 1402 ke Waiting Fix dengan komentar. Perbaikan masuk ke PR grup dan review AC mencakup keempat kartu; 1402 kembali ke Waiting Review; 1401, 1403, dan 1404 tidak berubah status.

### Acceptance Examples

- AE1. **Covers R1, R2.** Given epic E-PGS43 dengan kartu To Do 1401–1404 milik Beni dan 1405 milik Arsya, 1402 sudah punya run, dan 1404 ada di antrean VM, when Beni menekan "Start together" di 1401, then Mandor menawarkan 1401 dan 1403, dan 1402 serta 1404 tidak bisa dipilih beserta alasannya.
- AE2. **Covers R5, R12, R14.** Given grup 1401–1404 di laptop selesai, when dev membuka gerbang Push & PR, then gerbang menampilkan AC keempat kartu, dan setelah PR dibuat keempat kartu berstatus Waiting Review dengan tautan PR yang sama.
- AE3. **Covers R8.** Given grup berjalan dan agent bertanya di kartu ketiga, when pertanyaan belum dijawab, then kartu keempat tidak dimulai dan kartu pertama dan kedua tetap ter-commit di branch grup.
- AE4. **Covers R9.** Given grup berjalan di VM dan kartu ketiga dipindah ke Blocked oleh orang lain, when kartu kedua selesai, then kartu ketiga dikeluarkan dengan klaim dilepas, grup diparkir dengan alasan "card removed", dan kartu keempat belum dikerjakan.
- AE5. **Covers R17, R18.** Given PR grup terbuka dan keempat kartu Waiting Review, when QA memindahkan 1402 ke Waiting Fix dengan komentar tepercaya, then perbaikan di-commit ke branch grup, review AC mencakup keempat kartu, hanya 1402 yang berubah status, dan 1402 kembali ke Waiting Review setelah selesai.
- AE6. **Covers R14.** Given grup selesai di VM dengan PR draft, when dev menekan "Send to review" pada grup, then PR siap di-review dan keempat kartu pindah dari In Progress ke Waiting Review bersama.
- AE7. **Covers R20, R22.** Given grup dan satu kartu tunggal di antrean VM, when grup berhenti di kartu kedua karena batas biaya, then grup diparkir, laporan pagi menampilkan progres per kartu (kartu pertama sudah di-commit, kartu kedua tertahan) dan menghitung keduanya diparkir dengan alasan grup karena PR grup belum dibuat, dan kartu tunggal tetap dikerjakan.
- AE8. **Covers R13.** Given kartu 1401 sudah di-commit lalu dipindah ke Blocked sebelum gerbang Push & PR, when kartu terakhir selesai, then 1401 tampil "changed in Lark, code already in the branch", review AC dan PR tidak berjalan sebelum dev memilih revert atau tetap menyertakannya, dan bila revert konflik grup ditahan dengan alasan "revert conflict".

### Success Criteria

- Satu flow 3–5 kartu yang saling bergantung selesai sebagai satu PR draft dalam satu malam di VM tanpa campur tangan, bila batas pemakaian siklus cukup dan semua AC bisa diverifikasi.
- Reviewer menerima satu PR per flow dengan ringkasan AC per kartu.

### Scope Boundaries

- Tidak menggabungkan run per-kartu yang sudah ada ke dalam grup.
- Tidak mendukung grup lintas developer atau branch dasar dari PR orang lain (stacked PR); grup selalu dari branch default.
- Tidak mendeteksi dependensi antar kartu secara otomatis; urutan dari nomor kartu dan pilihan dev.
- Tidak mendukung grup yang kartunya butuh repo atau workspace berbeda.
- Tidak memblokir merge di GitHub; hanya memperingatkan (R19).

#### Deferred for later

- Branch dasar dari PR orang lain (stacked PR) untuk flow yang bergantung pada pekerjaan rekan.
- Mengeluarkan kartu dari grup setelah PR dibuat (memecah PR).
- Pemeriksaan AC di tengah grup (saat ini satu review di akhir; satu AC yang tidak bisa diverifikasi memarkir seluruh grup setelah semua kartu dikerjakan).

### Dependencies / Assumptions

- Plan mode VM (`docs/plans/2026-10-23-002-feat-vm-overnight-mode-plan.md`) menyediakan antrean, siklus, pagar lembur, "Send to review", "Take over to laptop", dan laporan pagi. R20–R23 dan klausa VM di R2, R6, R8, R9, R13, R14 mengikuti rilis plan VM; bagian lain bisa dikirim lebih dulu.
- R19 membutuhkan deteksi status merged PR grup, yang belum ada di Mandor (hari ini status PR hanya diperiksa untuk kartu tanpa tautan PR).
- Field `Epic` di Sprint Tasks adalah link dua arah ke tabel "Epic Pegasus"; tidak ada field dependensi antar kartu (Base salinan, 2026-10-23).
- Klaim semua kartu sejak awal (R7) membuat kartu terakhir tampil In Progress berjam-jam sebelum dikerjakan; diterima pemilik produk agar rekan tim tidak mengambil kartu di tengah flow. Klaim tanpa heartbeat menjadi basi setelah 30 menit (Rilis 1), jadi perlindungan ini melemah bila daemon laptop mati lama; grup di VM tidak terpengaruh.

---

## Planning Contract

### Key Technical Decisions

- **KTD-G1. Run grup adalah run biasa dengan daftar kartu, bukan entitas baru.** `runs` tetap menyimpan `record_id` kartu pertama (kartu utama) sehingga jalur yang ada (pesan, review, app run, notifikasi) tetap bekerja. Tabel baru `run_cards` (run_id, record_id, posisi, state kartu, `commits_json` berisi commit per repo anggota, plan kartu, alasan dikeluarkan, catatan "changed in Lark", `fix_requested_by`, `quiet_until_ms`, status loop perbaikan kartu) menyimpan semua kartu. Saat kartu utama keluar (R9, R10, atau revert R13), `runs.record_id` dipindah ke kartu aktif pertama berikutnya dalam transaksi yang sama. Run tunggal adalah grup satu kartu secara konseptual, tetapi tidak dimigrasikan: kode membaca `run_cards` bila ada dan jatuh ke `record_id` bila tidak. Alasan: perubahan terkecil yang menjaga invarian Rilis 1–2 di run tunggal.
- **KTD-G2. Satu kartu hanya boleh di satu run terbuka.** `run_cards` punya kolom `open` yang ikut ditutup bersama run, dengan indeks unik parsial (record_id) WHERE open. Untuk run grup, `active_for(record_id)` membaca `run_cards.open`; untuk run tunggal, `runs.record_id`. Pembuatan run tunggal memeriksa `run_cards` terbuka dan penyisipan `run_cards` memeriksa run tunggal terbuka, dalam transaksi yang sama, sehingga keunikan berlaku dua arah. Board memetakan semua kartu grup ke run-nya.
- **KTD-G3. Daemon membuat commit lokal per kartu; agent tetap dilarang commit.** Saat kartu selesai ("Next card" di laptop, otomatis di VM), daemon commit semua perubahan di worktree (dan setiap anggota workspace) dengan pesan dari `commit_message` yang diawali kunci kartu, tanpa push. Commit tidak di-squash. Publish hanya push (dan commit sisa perubahan bila ada). `GitPort` mengembalikan SHA dari commit dan mendapat operasi revert yang melaporkan konflik. Plan turn kartu berikutnya melanjutkan sesi agent yang sama (konteks flow terjaga); plan disimpan per kartu di `run_cards`, dan `approve_plan` serta `DesignGaps` memakai kartu aktif. Pilihan Planning (Plan/Skip) dipilih sekali saat mulai dan berlaku untuk setiap kartu. Alasan: revert per kartu (R13) dan review per kartu butuh batas commit yang jelas.
- **KTD-G4. State kartu di domain:** `pending → active → done`, `removed{reason}`, dan tanda `changed_in_lark` pada kartu `done`. Keputusan murni di `mandor-domain`: kandidat grup, kartu berikutnya, apakah grup harus berhenti (R8, R9), klasifikasi kartu saat pemeriksaan gerbang (R13), dan kartu yang dipindah status saat publish (R14).
- **KTD-G5. Heartbeat per kartu.** `heartbeat_all` memeriksa setiap kartu terbuka. Kehilangan klaim pada kartu aktif menghentikan run seperti sekarang; pada kartu lain hanya menandai kartu (R9/R13 yang menangani).
- **KTD-G6. Buku AC berlabel kartu.** `ac_rows` mendapat kolom `record_id`; `refresh_ledger` mengambil AC dari semua kartu aktif grup; `AcRowView` membawa kunci kartu. Untuk R18, `ac_rows` menyimpan verdict reviewer sebelum keputusan (`decided_verdict`) di samping alasan dan `decided_by`; fungsi domain setelah review memulihkan keputusan bila verdict baru sama dengan `decided_verdict` (Skipped↔Fail, CheckedManually↔CantVerify). Perilaku ini hanya untuk run grup; run tunggal tetap seperti Rilis 2.
- **KTD-G7. PR grup.** Judul `<KEY-awal>–<KEY-akhir>: <judul epic>` (judul dibaca dari tabel epic yang ditaut field Epic; jatuh ke judul kartu pertama bila gagal), deskripsi per kartu (ringkasan AC, verdict, kartu dikeluarkan, kartu yang tetap disertakan setelah berubah), dan peringatan Waiting Fix (R19) di bagian atas deskripsi selama ada kartu Waiting Fix. `mark_waiting_review` memindahkan setiap kartu aktif yang bukan `changed_in_lark` dan menulis `mandor_pr` ke semuanya.
- **KTD-G8. Waiting Fix grup.** `fix_candidates` dan `fix_loop_records` memetakan semua kartu grup terbuka ke run lewat KTD-G2. Satu run perbaikan per grup; `fix_requested_by`, jeda tenang, dan status loop disimpan per kartu di `run_cards`. State loop run grup diturunkan dari kartunya: run tetap di loop perbaikan selama ada kartu grup di Waiting Fix atau sedang diperbaiki; reset "status diubah manusia" hanya berlaku untuk kartu itu sendiri. `finish_fix_turn` dan `mark_waiting_review` bekerja pada daftar kartu yang diperbaiki, bukan `run.record_id`. Pesan perbaikan menggabungkan komentar per kartu dengan label. Status merged diperiksa saat kartu masuk loop perbaikan, sebelum giliran perbaikan dimulai, dan sebelum push. PR merged sebelum komentar dijurnal: kartu keluar dari run grup dan run tunggal dibuat dari branch default. PR merged yang baru terdeteksi sebelum push: komentar batch dikembalikan ke `pending` agar run tunggal baru menerimanya, dan tidak ada push ke branch grup.
- **KTD-G9. Field Epic dibaca dari Lark** sebagai daftar id record epic pada `Task` (default kosong agar cache lama tetap terbaca); judul epic dibaca dari tabel yang ditaut field itu, mengikuti pola pembacaan AC. Kandidat grup = kartu di cache tugas dengan epic sama, PIC dev, To Do, lolos `check_start`, tanpa run terbuka (KTD-G2), dan tidak di antrean VM. Fungsi kandidat menerima tanda "di antrean VM" sebagai input sehingga AE1 diuji di U1 dengan fake; sumber antrean nyata dipasang di U10.
- **KTD-G10. VM (U10) menunggu plan mode VM.** Unit VM memakai antrean, siklus, "Send to review", dan "Take over" dari plan VM; tidak dikerjakan sebelum itu ada.

### High-Level Technical Design

```mermaid
stateDiagram-v2
  [*] --> pending
  pending --> active: giliran kartu (recheck Lark lolos)
  pending --> removed: dev mengeluarkan / berubah di Lark (grup berhenti)
  active --> done: Next card (laptop) / turn selesai tanpa tanya (VM) → commit lokal
  active --> active: agent bertanya / batas biaya (grup berhenti, lanjut dari sini)
  done --> done: changed_in_lark (ditandai, diputuskan di gerbang)
  done --> removed: revert di gerbang
```

```mermaid
sequenceDiagram
  participant Dev
  participant Daemon
  participant Lark
  participant Agent
  Dev->>Daemon: runs.start_group(kartu, target)
  Daemon->>Lark: klaim semua kartu, To Do → In Progress
  loop tiap kartu
    Daemon->>Lark: recheck kartu (R9)
    Daemon->>Agent: plan turn (laptop: tunggu approve)
    Daemon->>Agent: implement turn
    Dev->>Daemon: Next card
    Daemon->>Daemon: commit lokal kartu
  end
  Daemon->>Lark: recheck semua kartu (R13)
  Daemon->>Agent: review AC grup (buku AC berlabel)
  Dev->>Daemon: Push & PR
  Daemon->>Lark: In Progress → Waiting Review (kartu aktif), mandor_pr
```

### Sequencing

U2 → U1 → U3 → U4 → U5 → U6 → U7 (U7 juga bergantung pada U2). U8 mengikuti U4–U7 (RPC per kemampuan). U9 setelah U8. U11 terakhir untuk bagian laptop. U10 setelah plan VM terimplementasi.

---

## Implementation Units

### U1. Aturan grup di domain

- **Goal:** keputusan murni grup.
- **Requirements:** R1, R2, R3, R8, R9, R10, R13, R14.
- **Dependencies:** U2 (`Task.epic`).
- **Files:** `crates/mandor-domain/src/group.rs` (baru), `crates/mandor-domain/src/ac_ledger.rs` (pemulihan keputusan), `crates/mandor-domain/src/lib.rs`, test di modul yang sama.
- **Approach:** tipe kartu grup dan state (KTD-G4); fungsi kandidat dari daftar tugas (epic, PIC, To Do, `check_start`, tanpa run, tidak di antrean); urutan default dari nomor kartu; kartu berikutnya; keputusan berhenti (kartu dikeluarkan sebelum gilirannya, kehilangan klaim kartu aktif); klasifikasi gerbang (kartu `done` berubah status/PIC → `changed_in_lark`); kartu yang dipindah ke Waiting Review saat publish.
- **Execution note:** test-first.
- **Test scenarios:**
  - Covers AE1. Lima kartu epic sama: dua milik dev To Do siap, satu sudah punya run, satu di antrean, satu milik orang lain → kandidat hanya dua, alasan untuk yang tidak bisa dipilih.
  - Kartu tanpa readiness siap → tidak bisa dipilih dengan alasan dari `check_start`.
  - Urutan default naik menurut nomor kartu; urutan dev dipertahankan.
  - Kartu ketiga berubah status sebelum gilirannya → grup berhenti dengan alasan "card removed".
  - Kehilangan klaim pada kartu `pending` → hanya ditandai; pada kartu `active` → grup berhenti.
  - Kartu `done` berubah PIC, atau berubah status → `changed_in_lark`; tidak masuk daftar kartu yang dipindah status saat publish.
  - Pemulihan keputusan gerbang: verdict baru sama dengan `decided_verdict` → keputusan dipulihkan; berbeda → baris kembali menahan.
  - Kartu `removed` tidak dihitung "semua kartu aktif selesai".

### U2. Field Epic dan status merged PR

- **Goal:** data yang dibutuhkan kandidat grup dan R19.
- **Requirements:** R1, R19.
- **Dependencies:** —
- **Files:** `crates/mandor-lark/src/gateway.rs`, `crates/mandor-lark/src/fields.rs`, `crates/mandor-domain/src/task.rs` (`Task.epic`), `crates/mandor-usecase/src/ports.rs` (pembacaan judul epic di samping `acceptance_criteria`), `crates/mandor-git/src/port.rs`, `crates/mandor-usecase/src/run_ports.rs` (`PrRef` dengan status), test di crate masing-masing.
- **Approach:** baca field `Epic` (link) menjadi daftar id record; tugas yang tersimpan ikut membawa epic (default kosong). Baca judul record epic dari tabel yang ditaut (tabel ditemukan dari properti field link), dengan fallback judul kartu pertama bila gagal. `Gh::pr_for_branch` sudah mengurai `PrState::Merged`; ubahan cukup di `port.rs` agar `PrRef` membawa `Open | Merged | Closed`, dan pemakai lama memakai `is_open()`.
- **Test scenarios:**
  - Record dengan Epic satu link, tanpa Epic, dan Epic dua link → `Task.epic` sesuai.
  - `PrState` Merged/Closed/Open → `PrRef` sesuai; `check_prs` tetap berperilaku sama untuk `Open`.
  - Judul epic terbaca; tabel epic gagal dibaca → fallback tanpa error.
  - Baris tugas cache lama tanpa field epic tetap terbaca.

### U3. Penyimpanan kartu grup dan buku AC berlabel

- **Goal:** `run_cards`, keunikan kartu di run terbuka, dan label kartu di `ac_rows`.
- **Requirements:** R2, R11, R12, R16 (data).
- **Dependencies:** U1.
- **Files:** `crates/mandor-store/migrations/0006_run_cards.sql` (nomor berikutnya saat implementasi), `crates/mandor-store/src/run_port.rs`, `crates/mandor-store/src/ac_ledger.rs`, `crates/mandor-usecase/src/run_ports.rs` (port), `crates/mandor-store/tests/run_cards.rs`, `.sqlx` offline.
- **Approach:** tabel `run_cards` (KTD-G1) dengan indeks unik parsial pada kartu terbuka (KTD-G2); `close` menutup kartu; `active_for` mencari lewat kartu; `ac_rows.record_id` (nullable untuk run lama). Port: simpan/muat kartu, ubah state kartu.
- **Execution note:** test-first lewat store dengan SQLite sementara.
- **Test scenarios:**
  - Kartu yang sama di dua run terbuka → konflik "record already has an active run".
  - Run ditutup → kartu bisa masuk run baru.
  - `active_for(kartu ketiga)` → run grup.
  - Run tunggal untuk kartu `pending` grup → konflik; run grup untuk kartu yang punya run tunggal terbuka → konflik.
  - Kartu utama keluar → `runs.record_id` pindah ke kartu aktif berikutnya, dan kartu yang keluar bisa dipakai run baru.
  - `ac_rows` dengan label kartu bolak-balik; run lama tanpa label tetap terbaca.
  - Migrasi dua kali tidak mengubah apa pun (pola test migrasi yang ada).

### U4. Memulai grup

- **Goal:** `start_group`: klaim semua kartu, In Progress, worktree, buku AC, dan plan turn kartu pertama.
- **Requirements:** R1–R7; F1.
- **Dependencies:** U1, U3.
- **Files:** `crates/mandor-usecase/src/run/group.rs` (baru), `crates/mandor-usecase/src/run/start.rs`, `crates/mandor-usecase/src/review/ledger.rs` (`init_ledger` semua kartu), `crates/mandor-domain/src/run.rs` (nama branch grup), `crates/mandor-usecase/tests/group_flow.rs` (baru), `crates/mandor-usecase/tests/common/run_fakes.rs`.
- **Approach:** klaim berurutan dengan rollback semua klaim bila salah satu gagal; nama branch `mandor/<KEY-awal>-<KEY-akhir>-<slug epic>`; satu worktree (atau satu per anggota workspace) dari branch default; `run_cards` dibuat; outbox To Do → In Progress untuk setiap kartu; plan turn kartu pertama memakai pesan plan dengan konteks flow (judul semua kartu grup) dan AC kartu itu saja.
- **Execution note:** test-first lewat `group_flow.rs`.
- **Test scenarios:**
  - Covers F1. Empat kartu → empat klaim, empat transisi In Progress, satu run dengan empat kartu, plan turn kartu pertama.
  - Klaim kartu ketiga gagal → klaim pertama dan kedua dilepas, tidak ada run, tidak ada transisi.
  - Kartu yang sudah punya run → ditolak sebelum klaim.
  - Target workspace → satu worktree per anggota, satu run.

### U5. Berpindah kartu, berhenti, dan mengeluarkan kartu

- **Goal:** "Next card", commit lokal per kartu, berhenti, mengeluarkan kartu, menghentikan grup, dan heartbeat per kartu.
- **Requirements:** R5, R6, R8, R9, R10, R11; F3; AE3, AE4.
- **Dependencies:** U4.
- **Files:** `crates/mandor-usecase/src/run/group.rs`, `crates/mandor-usecase/src/run/members.rs` (commit tanpa push), `crates/mandor-usecase/src/run/ownership.rs` (heartbeat per kartu; `close_run` melepas klaim semua kartu terbuka di `run_cards`), `crates/mandor-usecase/src/run_ports.rs` (`GitPort` commit dengan SHA, revert dengan konflik), `crates/mandor-git/src/port.rs`, `crates/mandor-git/src/publish.rs`, `crates/mandor-usecase/tests/common/run_fakes.rs`, `crates/mandor-usecase/tests/group_flow.rs`.
- **Approach:** `next_card` hanya dari AwaitingDev + Implement: commit lokal kartu aktif (pesan diawali kunci kartu), tandai `done`, recheck kartu berikutnya di Lark (R9), lalu plan turn kartu berikutnya, atau selesai bila tidak ada lagi. Kartu tanpa perubahan kode tetap `done` tanpa commit. Setelah R9 mengeluarkan kartu, run berada di AwaitingDev dengan catatan "card removed" tanpa kartu aktif; `continue_group` memeriksa ulang dan memulai plan turn kartu `pending` berikutnya. `remove_card` untuk kartu `pending`. `stop_group` push branch grup tanpa PR (agar commit kartu `done` tidak hilang), menutup run, dan melepas klaim semua kartu terbuka; kartu `done` dan `pending` diberi catatan agar dev mengembalikannya ke To Do. Heartbeat per kartu (KTD-G5).
- **Execution note:** test-first.
- **Test scenarios:**
  - Next card setelah implementasi kartu 1 → commit dengan kunci kartu 1, kartu 2 `active`, plan turn kartu 2.
  - Covers AE3. Agent bertanya di kartu 3 (AwaitingDev setelah turn dengan pertanyaan) → kartu 4 tetap `pending`; Next card hanya tersedia setelah dev melanjutkan.
  - Covers AE4 (laptop). Kartu 3 berubah status sebelum gilirannya → dikeluarkan, klaim dilepas bila masih milik dev, grup berhenti dengan pilihan lanjut/hentikan.
  - Remove card `pending` → klaim dilepas, kartu `removed`, catatan "kembalikan ke To Do".
  - Stop group → branch di-push tanpa PR, run ditutup, klaim kartu `pending`, `active`, dan `done` dilepas.
  - Kartu 3 dikeluarkan R9, dev memilih lanjut → plan turn kartu 4.
  - Heartbeat: PIC kartu 4 berubah saat kartu 2 aktif → run tetap berjalan, kartu 4 ditandai.
  - Restart daemon saat kartu 2 aktif → run Failed (rekonsiliasi), Resume melanjutkan kartu 2.

### U6. Gerbang, revert, dan PR grup

- **Goal:** pemeriksaan kartu berubah, revert, review AC grup, PR, dan Waiting Review bersama.
- **Requirements:** R12–R16; AE2, AE8.
- **Dependencies:** U5, U2.
- **Files:** `crates/mandor-usecase/src/run/publish.rs`, `crates/mandor-usecase/src/run_ports.rs`, `crates/mandor-git/src/port.rs` (revert), `crates/mandor-usecase/src/run/members.rs` (judul dan deskripsi PR grup), `crates/mandor-usecase/src/review/service.rs` (brief dengan semua kartu), `crates/mandor-domain/src/push_gate.rs` (ringkasan per kartu), `crates/mandor-usecase/tests/group_flow.rs`, `crates/mandor-usecase/tests/publish_gate.rs`.
- **Approach:** setelah kartu terakhir `done`, recheck semua kartu; kartu `changed_in_lark` menahan gerbang sampai dev memilih revert atau tetap (alasan). Revert memakai revert commit kartu itu; konflik → batalkan revert dan tahan dengan "revert conflict". Revert membuat baris AC stale sehingga review AC grup berjalan ulang. Brief review memuat semua kartu aktif. Publish: push, PR (satu per repo), `mandor_pr` ke semua kartu, transisi Waiting Review untuk kartu aktif selain `changed_in_lark`.
- **Execution note:** test-first.
- **Test scenarios:**
  - Covers AE2. Empat kartu `done`, review lulus → satu PR, empat transisi Waiting Review, `mandor_pr` sama.
  - Covers AE8. Kartu 1 berubah ke Blocked → gerbang menahan; keep dengan alasan → PR mencatat alasan, kartu 1 tidak dipindah status.
  - Revert kartu 1 tanpa konflik → commit revert, baris AC stale, review diperlukan lagi.
  - Revert konflik → tidak ada perubahan di branch, gerbang tertahan "revert conflict".
  - Baris AC gagal di kartu 3 → gerbang menahan seluruh grup dengan label kartu 3; Push anyway dengan alasan → push.
  - Workspace → satu PR per anggota yang berubah, semua kartu membawa tautan yang sama.
  - Revert kartu utama → `runs.record_id` pindah, judul PR memakai kartu aktif, Start kartu yang di-revert tidak ditolak "record already has an active run".

### U7. Waiting Fix grup dan PR merged

- **Goal:** perbaikan kartu grup di branch grup, review AC seluruh grup, peringatan merge, dan run lanjutan setelah merged.
- **Requirements:** R17–R19; F4; AE5.
- **Dependencies:** U2, U6.
- **Files:** `crates/mandor-usecase/src/waiting_fix/engine.rs`, `crates/mandor-usecase/src/waiting_fix/feedback_qa.rs`, `crates/mandor-usecase/src/run/mod.rs` (`fix_loop_records`), `crates/mandor-usecase/src/run/publish.rs` (`mark_waiting_review` per kartu), `crates/mandor-usecase/src/review/service.rs` (pemulihan keputusan setelah review), `crates/mandor-domain/src/push_gate.rs`, `crates/mandor-usecase/src/run/members.rs` (peringatan di deskripsi), `crates/mandor-usecase/tests/waiting_fix.rs`.
- **Approach:** kandidat perbaikan dari kartu grup (KTD-G8); jeda tenang dan `fix_requested_by` per kartu; satu run perbaikan menggabungkan komentar berlabel; kartu baru yang masuk saat run perbaikan berjalan ikut turn berikutnya; review setelah perbaikan memakai semua kartu dengan keputusan lama dipertahankan bila verdict tidak berubah; hanya kartu yang diperbaiki berpindah WF → In Progress → Waiting Review. Peringatan "do not merge" di deskripsi PR selama ada kartu WF. Sebelum push, PR merged → kartu itu dipindah ke run tunggal baru dari branch default.
- **Execution note:** test-first.
- **Test scenarios:**
  - Covers AE5. 1402 ke Waiting Fix dengan komentar tepercaya → perbaikan di branch grup, review mencakup empat kartu, hanya 1402 yang berubah status.
  - 1402 dan 1404 ke Waiting Fix bersamaan → satu run perbaikan, komentar berlabel, keduanya kembali ke Waiting Review.
  - Feedback QA kartu 1404 dari orang yang bukan pemindahnya → tidak dipercaya (aturan per kartu).
  - Baris kartu 1401 yang dulu diputuskan "Push anyway" dan verdict-nya tetap Fail → tidak menahan; baris yang berubah Pass → Fail → menahan.
  - PR merged sebelum push perbaikan → tidak ada push ke branch grup; run tunggal baru untuk kartu itu dari branch default menerima komentar QA yang sama.
  - PR sudah merged saat kartu masuk Waiting Fix → run tunggal langsung dibuat, run grup tidak masuk loop.
  - Kartu utama di Waiting Review dan kartu kedua di Waiting Fix selama dua tick → satu notifikasi, run tetap di loop perbaikan, `fix_requested_by` tidak ditimpa.
  - Deskripsi PR memuat peringatan saat ada kartu WF dan menghilangkannya setelah kembali ke Waiting Review.

### U8. RPC dan DTO grup

- **Goal:** kontrak protokol untuk app.
- **Requirements:** R1, R5, R10, R11, R13.
- **Dependencies:** U4–U7.
- **Files:** `crates/mandor-protocol/src/dto.rs`, `crates/mandor-protocol/src/lib.rs`, `crates/mandord/src/dispatch.rs`, `crates/mandord/src/views.rs`, `apps/desktop/src/lib/protocol/*.ts` (generate), `crates/mandord/tests/daemon.rs`.
- **Approach:** `groups.candidates(record_id)`, `runs.start_group`, `runs.next_card`, `runs.continue_group`, `runs.remove_card`, `runs.stop_group`, `runs.gate_card_decision` (revert/keep). `RunView` membawa `cards` (kunci, judul, state, catatan); `AcRowView` membawa kunci kartu; `CardView.run` diisi untuk semua kartu grup dengan `start_blocked` berisi alasan grup.
- **Test scenarios:**
  - Daemon sungguhan: `groups.candidates` mengembalikan kandidat dan alasan; method tak dikenal tetap error konsisten.
  - Board menampilkan run grup pada keempat kartu; Start nonaktif dengan alasan "in group KEY-…".

### U9. UI grup

- **Goal:** "Start together", progres per kartu, Next card, keputusan kartu berubah di gerbang, dan label kartu di buku AC.
- **Requirements:** R1, R3, R10, R11, R13, R16, R19.
- **Dependencies:** U8.
- **Files:** `apps/desktop/src/lib/components/kanban/StartGroupDialog.svelte` (baru), `apps/desktop/src/lib/components/mandor/TaskCard.svelte`, `apps/desktop/src/lib/components/workspace/Workspace.svelte`, `apps/desktop/src/lib/components/workspace/GroupProgress.svelte` (baru), `apps/desktop/src/lib/components/workspace/GateDialogs.svelte`, `apps/desktop/src/lib/components/workspace/AcReviewTab.svelte`, `apps/desktop/src/lib/group.ts` (baru), `apps/desktop/src/lib/group.test.ts`, `apps/desktop/src/lib/dev/mock-transport.ts`.
- **Approach:** logika murni di `group.ts` (urutan, ringkasan progres, teks alasan); dialog memakai kandidat dari RPC dengan centang dan urutan; workspace menampilkan daftar kartu dengan state dan tombol Next card / Remove / Stop group; gerbang menampilkan kartu "changed in Lark" dengan pilihan revert/keep; buku AC dikelompokkan per kartu.
- **Test scenarios:**
  - Ringkasan progres dari state kartu (2 selesai, 1 aktif, 1 menunggu).
  - Urutan dev dipertahankan saat kandidat dicentang ulang.
  - Tombol Next card hanya saat AwaitingDev + Implement dan ada kartu berikutnya; "Finish" pada kartu terakhir.
  - Kartu tidak bisa dipilih menampilkan alasan.
- **Verification:** dicek di `/?mock` dengan grup tiruan.

### U10. Grup di VM lembur (menunggu plan VM)

- **Goal:** grup sebagai item antrean VM.
- **Requirements:** R2 (VM), R6 (VM), R8 (VM), R9 (VM), R13 (VM), R14 (VM), R20–R23; F2; AE4 (VM), AE6, AE7.
- **Dependencies:** U1–U9 dan implementasi plan `docs/plans/2026-10-23-002-feat-vm-overnight-mode-plan.md` (antrean, siklus, Send to review, Take over).
- **Files:** ditentukan setelah plan VM terimplementasi (modul antrean VM, laporan pagi, tab Overnight).
- **Approach:** item antrean bertipe grup; kartu berlanjut otomatis bila turn selesai tanpa pertanyaan; grup mengalah ke Waiting Fix di antara kartu; "window ended" tetap di depan antrean; parkir, antre ulang, dan ambil alih sebagai satu kesatuan; laporan per kartu; "Send to review" grup.
- **Test scenarios:** Covers AE4 (VM), AE6, AE7, aturan R20–R23, dan pengecualian kartu di antrean (R2) dengan sumber antrean nyata di test antrean VM.

### U11. Runbook dan uji langsung laptop

- **Goal:** panduan dan bukti di Base salinan.
- **Requirements:** semua bagian laptop.
- **Dependencies:** U9.
- **Files:** `docs/runbook/grouped-runs.md` (baru), `docs/progress/grouped-runs.md` (baru), `README.md`.
- **Approach:** tiga sampai empat kartu uji baru di Base salinan dalam satu epic uji, repo `mandor-smoke`.
- **Test scenarios:**
  - F1 penuh sampai PR dan Waiting Review keempat kartu.
  - F3: pertanyaan di kartu ketiga lalu lanjut.
  - F4: satu kartu ke Waiting Fix lalu kembali.
  - AE8: satu kartu dipindah di Lark setelah commit.
- **Verification:** hasil tercatat di `docs/progress/grouped-runs.md`.

---

## Verification Contract

- `make check` hijau.
- `bun run --cwd apps/desktop check` dan `bun run --cwd apps/desktop test`.
- Test usecase `group_flow.rs`, `waiting_fix.rs`, `publish_gate.rs`, test store `run_cards.rs` lulus.
- TS protokol diregenerasi dan sinkron.
- Uji langsung U11 tercatat.
- Setelah U10: test antrean VM untuk AE4 (VM), AE6, AE7, dan R20–R23 lulus.

## Definition of Done

- U1–U9 dan U11 selesai dengan skenario test terpenuhi dan gerbang hijau; PR bagian laptop dibuka tanpa merge sendiri.
- U10 dikerjakan setelah plan VM terimplementasi, dengan AE6–AE7 dan R20–R23 terbukti.

---

## Appendix

### Risks

- **Invarian satu record per run tersebar luas** (board, notifikasi, Waiting Fix, heartbeat, check_prs). Mitigasi: kartu utama tetap di `runs.record_id`, `active_for` lewat kartu, test regresi run tunggal tetap dijalankan.
- **Commit per kartu mengubah perilaku publish** (sebelumnya satu commit per repo). Mitigasi: hanya untuk run grup; run tunggal tetap satu commit.
- **Revert di tengah flow** bisa konflik; ditangani sebagai penahanan, bukan kegagalan diam.
- **Biaya review AC grup** pada 3–5 kartu lebih besar; dicatat di laporan untuk dipantau.
- **Refresh buku AC sebagian.** Bila satu kartu gagal dibaca, `reconcile_rows` bisa menandai AC kartu itu Removed. Mitigasi: refresh grup batal seluruhnya bila ada kartu gagal dibaca (baris lama dipertahankan).
- **Beban heartbeat Lark** bertambah satu baca per kartu per siklus; dipantau lewat rate limiter yang ada.

### Assumptions

- `stop_group` push branch tanpa PR agar kerja kartu `done` tidak hilang; dev memutuskan kelanjutannya secara manual.
- Plan turn kartu berikutnya melanjutkan sesi agent yang sama; Planning (Plan/Skip) dipilih sekali untuk seluruh grup.

### Sources

- Product Contract di atas dan dua ronde review dokumen (2026-10-23).
- Peta kode: `crates/mandor-usecase/src/run/{start,session,publish,members,ownership,reconcile}.rs`, `crates/mandor-usecase/src/waiting_fix/engine.rs`, `crates/mandor-usecase/src/review/{ledger,service,capability}.rs`, `crates/mandor-domain/src/{run,claim,gate,push_gate,ac_ledger,task}.rs`, `crates/mandor-store/migrations/0001_init.sql`, `crates/mandor-store/migrations/0005_ac_review.sql`, `crates/mandord/src/{dispatch,views}.rs`.
- Plan mode VM: `docs/plans/2026-10-23-002-feat-vm-overnight-mode-plan.md`.
