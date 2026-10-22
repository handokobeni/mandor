---
title: Mandor AC Review (Rilis 2) - Plan
type: feat
date: 2026-10-21
topic: mandor-ac-review
artifact_contract: ce-unified-plan/v1
artifact_readiness: implementation-ready
product_contract_source: ce-brainstorm
execution: code
---

# Mandor AC Review (Rilis 2) - Plan

## Goal Capsule

- **Objective:** Kurangi task yang kembali dari reviewer karena perilaku AC terlewat, dengan memeriksa setiap Acceptance Criteria terhadap sumber desain sebelum ngoding dan terhadap kode, test, serta app yang berjalan sebelum PR.
- **Product authority:** Rilis 2 di `docs/plans/2026-10-18-001-feat-korlap-lark-lembur-plan.md` (R17–R18). Dokumen ini memperinci keduanya; R-ID di bawah berlaku lokal untuk dokumen ini.
- **Builds on:** Rilis 1: fase brainstorm/plan hanya baca dengan skill plugin, workspace multi-repo, satu PR per repo, perbaikan otomatis dari komentar reviewer, MCP server per run, dan pagar agent (hook, allowlist, tanpa kredensial).
- **Audience:** Produk ditujukan untuk dijual ke tim lain. Perilaku yang khusus satu tim menjadi pengaturan, dan dokumen memakai istilah netral; implementasi Rilis 2 tetap hanya untuk tracker Lark Base dan GitHub.
- **Open blockers:** Smoke test Rilis 1 (U14 di rencana Rilis 1) belum dijalankan. Desain layar belum ada; brief desainnya ada di `docs/design/mandor-screens-prompt-rilis-2.md` (lanjutan `docs/design/mandor-screens-prompt.md`), dan slot tab AC review di `docs/design/mandor-design/mandor-design-handoff/docs/design-notes.md` §5.
- **Authority hierarchy:** `CLAUDE.md` (standar `rust-engineering` mengikat) > Product Contract dokumen ini > Planning Contract (KTD-25 dst.) > unit. KTD Rilis 1 (KTD-0 sampai KTD-24) tetap berlaku kecuali disebut diubah.
- **Execution profile:** Implementation Units hanya untuk Rilis 2.0, dikerjakan test-first di `mandor/`. Rilis 2.1 dicatat di Deferred to Follow-Up Work; seam-nya (konsep sumber desain berjenis, tool milik daemon) dibangun di 2.0.
- **Stop conditions:** berhenti dan tanya pemilik produk bila sebuah unit tidak bisa selesai tanpa melanggar `rust-engineering`, bila asumsi di Planning Contract (A1–A5) terbukti salah, atau bila perilaku produk harus berubah.
- **Tail ownership:** commit hanya bila diminta; tidak ada push ke repo tim atau perubahan status Lark di luar record uji saat smoke.

---

## Product Contract

### Summary

Setiap run punya buku AC: satu tabel per AC di tab AC review. Sebelum ngoding, agent menandai perilaku AC yang tidak ada di sumber desain pilihan dev. Sebelum PR, reviewer terpisah menilai setiap AC terhadap kode, test, dan, untuk repo web, app yang berjalan di browser. Hasilnya menjadi gerbang Push & PR, ringkasan di deskripsi PR, dan catatan selisih desain di tracker task.

### Problem Frame

Dev membangun tampilan mengikuti desain dari desainer atau UI/UX engineer, sementara AC di tracker sering memuat perilaku yang tidak ada di desain. Perilaku itu tidak dikerjakan, dan task kembali dari reviewer atau QA setelah PR dibuat. Bentuk desain berbeda antar tim: kode prototype (mis. `kelola-mock` di tim pertama), file Figma, prototype yang di-deploy, atau ekspor gambar. Tidak ada tautan rapi dari task ke bagian desain yang relevan, dan selisih AC dengan desain tidak pernah sampai ke desainer, sehingga desain tertinggal dari AC.

### Key Decisions

- **AC di tracker adalah source of truth; desain adalah acuan tampilan.** Perilaku (apa yang terjadi, validasi, aturan) mengikuti AC, tampilan (layout, letak, gaya) mengikuti desain sejauh tidak bertentangan, dan gerbang menilai kode terhadap AC, bukan kecocokan dengan desain. Setiap selisih diarahkan agar kedua sumber kembali sinkron: desain diperbarui mengikuti AC, atau AC diperbarui bila desain yang benar.
- **AC menang atas desain, kecuali dev menandai sebaliknya.** Secara bawaan agent mengerjakan perilaku AC walau tidak ada di desain, dan tampilan mengikuti desain sejauh tidak bertentangan; selisih adalah informasi, bukan keputusan yang wajib diambil dev. Bila desain lebih baru dan AC sudah usang, dev dapat menandai baris AC itu "ikuti desain", karena kasus ini juga membuat task kembali dengan alasan sebaliknya.
- **Satu buku AC per run untuk dua titik pemeriksaan.** Kolom Design diisi sebelum ngoding dan kolom Verdict sebelum PR pada tabel yang sama, sehingga gerbang, deskripsi PR, dan tracker membaca sumber yang sama. Selisih sebagai teks bebas di plan, atau fase pemeriksaan terpisah sebelum brainstorm, ditolak karena hasilnya tidak terstruktur per AC atau menambah fase tanpa keputusan baru.
- **Sumber desain sebagai satu konsep dengan beberapa jenis.** Repo/folder kode, Figma, URL prototype, dan gambar/PDF diperlakukan sama oleh buku AC, sehingga tim dengan alur desain berbeda memakai fitur yang sama dan jenis baru bisa ditambah tanpa mengubah alur.
- **Reviewer terpisah dari agent pelaksana.** Review berjalan di sesi baru yang hanya baca, agar agent tidak menilai pekerjaannya sendiri dengan konteks yang sama.
- **Hal yang berbeda antar tim menjadi pengaturan.** Cara uji per repo, uji browser, dan ketatnya gerbang diatur per repo atau workspace, dengan bawaan yang cocok untuk tim pertama.
- **Gerbang memblokir dengan jalan keluar.** Dalam mode blokir, AC yang gagal mengunci Push & PR dan dev dapat melewatinya dengan alasan tertulis. AC yang tidak bisa dipastikan butuh konfirmasi dev bahwa ia sudah mengeceknya manual, karena AC tampilan adalah jenis yang paling sering terlewat; perbaikan otomatis dengan AC seperti itu menunggu dev.
- **Rilis 2 dikirim dalam dua tahap.** 2.0 memuat semua yang dipakai tim pertama: buku AC, sumber desain berupa repo atau folder kode, cek desain sebelum ngoding, review kode dan test, Run app dengan uji browser, gerbang, dan hasil keluar. 2.1 menambah jenis sumber Figma, URL prototype, dan gambar/PDF (R4, R6, R6a, dan bagian R3 serta R30 untuk jenis itu). Konsep sumber desain dibangun sejak 2.0 agar 2.1 hanya menambah jenis. Uji browser tetap di 2.0 karena spike membuktikan mekanismenya.
- **Uji di browser memakai sesi dev, bukan kredensial agent.** Dev menyalakan database dan login sendiri; reviewer hanya memakai tab miliknya sendiri di alamat yang diizinkan. Agent tidak pernah membaca atau menerima password, `.env`, atau token desain secara langsung, sejalan dengan pagar Rilis 1; app yang dinyalakan Mandor hanya menerima kunci `.env` pilihan dev setelah konfirmasi (R25a).

```mermaid
flowchart TB
  S[Sumber desain: repo/folder, Figma, URL, gambar/PDF] --> B[Brainstorm/plan: kolom Design]
  B --> L[Buku AC per run]
  R[Reviewer sebelum PR: kolom Verdict] --> L
  L --> T[Tab AC review di Mandor]
  L --> G[Gerbang Push & PR]
  L --> P[Ringkasan per AC di deskripsi PR]
  L --> K[Selisih desain di tracker task]
```

### Actors

- A1. Dev: mendaftarkan dan memilih sumber desain, menyalakan database dan login di browser, menjalankan review, memutuskan Push anyway.
- A2. Agent pelaksana: menyusun plan, mengisi kolom Design, dan memperbaiki temuan yang dikirim kepadanya.
- A3. Agent reviewer: sesi hanya baca yang menilai setiap AC dengan bukti.
- A4. Mandor daemon: menyimpan buku AC dan token desain, menyalakan app dari worktree, menegakkan gerbang, menulis PR dan tracker.
- A5. PM dan desainer: membaca selisih desain di tracker dan memperbarui desain.

### Requirements

**Buku AC**

- R1. Setiap run punya buku AC berisi satu baris per AC yang tidak di-drop, tampil di tab kelima "AC review".
- R2. Buku AC bertahan lintas giliran dan restart daemon, dan menambah atau menandai baris saat AC di tracker berubah selama run.

**Sumber desain**

- R3. Settings menyediakan daftar sumber desain dengan empat jenis: repo atau folder kode, file Figma, URL prototype, dan folder gambar atau PDF.
- R4. Token Figma (hanya baca, cakupan minimum) disimpan daemon seperti kredensial tracker. Agent membaca Figma lewat alat milik daemon yang hanya mengembalikan isi desain, hanya untuk run yang memakai sumber Figma, termasuk run satu repo; token tidak pernah berada di env, argumen, berkas, atau proses yang bisa dibaca agent.
- R5. Dialog Start memilih satu atau lebih sumber desain dari daftar, dan pilihan terakhir diingat untuk Start berikutnya.
- R6. Dialog Start menerima link desain khusus task yang hanya berlaku untuk run itu, dan menawarkan link desain yang terdeteksi di deskripsi task; link yang terdeteksi baru dipakai setelah dev mengonfirmasinya.
- R6a. Sumber berupa URL hanya menerima http(s), menolak alamat privat, link-local, dan metadata selain port app lokal yang terdaftar, dan menampilkan host lengkapnya kepada dev sebelum dipakai.

**Pemeriksaan desain sebelum ngoding**

- R7. Saat brainstorm atau plan, agent membaca sumber desain hanya baca, mencari bagian yang relevan sendiri, dan mengisi per AC: ada, sebagian, tidak ada, atau bertentangan dengan desain, beserta bagian desain yang ditemukan.
- R7a. Konten sumber desain diperlakukan sebagai data tidak tepercaya, seperti teks tracker di Rilis 1: dibungkus sebagai data, instruksi di dalamnya tidak pernah diikuti, dan path folder atau repo dibatasi ke akar yang didaftarkan tanpa jalan keluar lewat symlink.
- R8. Plan yang disusun mengerjakan semua perilaku AC, termasuk yang tidak ada di desain, kecuali baris yang ditandai "ikuti desain" (R8a).
- R8a. Saat menyetujui plan, dev dapat menandai baris AC yang bertentangan dengan desain sebagai "ikuti desain": agent mengikuti desain untuk AC itu, baris itu dikecualikan dari gerbang, dan permintaan memperbarui AC ditulis ke tracker untuk PM. Baris itu tidak dinilai reviewer terhadap AC dan tidak bisa dikirim ke agent lewat R17.
- R9. Saat plan disetujui, selisih desain ditulis ke field milik app di task pada tracker, sehingga tim bisa membuat view atau filter "task dengan selisih desain" yang dipantau desainer; runbook menjelaskan cara membuatnya. Mandor tidak mengirim pesan ke desainer.

**Review sebelum PR**

- R10. Reviewer berjalan di sesi agent terpisah yang hanya baca dan menilai setiap AC sebagai lulus, gagal, atau tidak bisa dipastikan, masing-masing dengan bukti (berkas dan baris, hasil test, atau langkah di browser). Status keempat, dilewati dengan alasan, hanya diberikan dev lewat Push anyway (R21).
- R11. Reviewer menjalankan test proyek yang ada sebagai bagian dari penilaian.
- R12. Setiap repo punya cara uji: web, yang memakai browser, atau tanpa app, yang hanya memakai kode dan test.
- R13. Untuk repo web, saat app lokal run itu menyala dan dev sudah login, reviewer mencoba AC tampilan di browser; tanpa itu, AC tampilan dinilai tidak bisa dipastikan.
- R14. Reviewer memakai prompt bawaan Mandor atau skill plugin pilihan dev, dan pilihan terakhir diingat.
- R15. Review dapat dijalankan dev kapan saja, dan berjalan otomatis saat dev menekan Push & PR tanpa hasil review yang masih berlaku.
- R16. Hasil review hanya berlaku untuk kode yang direview; perubahan kode setelahnya menandai hasil itu usang.
- R17. Dev dapat memilih baris yang gagal lalu mengirimnya ke agent pelaksana untuk diperbaiki, kemudian menjalankan review lagi.
- R18. Run workspace multi-repo direview sekali untuk semua repo, karena AC berlaku untuk task, bukan per repo.

**Gerbang Push & PR**

- R19. Setiap workspace atau repo memilih ketatnya gerbang: blokir (bawaan) atau peringatan.
- R20. Dalam mode blokir, Push & PR terkunci selama ada AC yang gagal.
- R21. Dev dapat menekan Push anyway dengan alasan tertulis; AC yang gagal ditandai dilewati dengan alasan itu di buku AC, di run, dan di deskripsi PR.
- R22. Dalam mode blokir, setiap AC yang tidak bisa dipastikan butuh konfirmasi dev "sudah dicek manual" sebelum Push & PR; konfirmasi itu tercatat di buku AC dan di deskripsi PR. Dalam mode peringatan, AC itu hanya ditandai.
- R23. Perbaikan otomatis setelah reviewer meminta perubahan menjalankan review sebelum push otomatis; dalam mode blokir, AC yang gagal atau tidak bisa dipastikan menahan push, menandai task, dan memberi notifikasi ke dev.

**Menjalankan app dan browser**

- R24. Pengaturan repo web memuat perintah start dan port app lokal.
- R25. Tombol Run app menyalakan app dari worktree run, dengan konfigurasi `.env` dari folder repo asli yang diberikan hanya ke proses itu dan tidak ditulis ke worktree.
- R25a. Karena app menjalankan kode tulisan agent, Run app meminta konfirmasi dev setelah menampilkan perubahan sejak app terakhir dijalankan, hanya meneruskan kunci `.env` yang dipilih dev, dan `.env` itu diharapkan hanya berisi kredensial lokal atau dev, bukan produksi.
- R26. Database dan layanan pendukung tetap dinyalakan dev; Mandor tidak menyiapkan database maupun akun.
- R27. Run app menyalakan semua repo run yang punya perintah start sekaligus (mis. api dan web dari worktree yang sama); hanya satu run yang app-nya menyala dalam satu waktu, dan menyalakan app run lain mematikan semua app run sebelumnya.
- R28. Uji browser dapat dimatikan per workspace, untuk tim yang tidak boleh memperlihatkan data aplikasi kepada model AI.
- R29. Reviewer memakai browser keluarga Chrome milik dev (Chrome, Edge, atau Chromium) lewat ekstensi Claude in Chrome, hanya di tab yang ia buat sendiri, tanpa menjalankan skrip di halaman.
- R30. Reviewer hanya membuka alamat app lokal, URL sumber desain yang terdaftar, dan link desain khusus task untuk run itu; pagar di luar prompt memeriksa alamat akhir setelah setiap redirect dan menolak alamat lain, tab lain, serta akses cookie atau penyimpanan browser.

**Hasil keluar**

- R31. Deskripsi setiap PR memuat ringkasan per AC: lulus, gagal, tidak bisa dipastikan (beserta konfirmasi dev bila ada), dilewati dengan alasan, atau mengikuti desain (R8a) beserta permintaan pembaruan AC.
- R32. Hasil lengkap per AC, beserta bukti dan riwayat review, tersedia di Mandor dengan masa simpan yang ditetapkan.
- R33. Bukti yang ditulis ke deskripsi PR dan tracker hanya menyebut langkah dan nama elemen, tidak pernah isi data halaman atau tangkapan layar.

### Key Flows

- F1. Start dengan sumber desain
  - **Trigger:** Dev menekan Start, memilih sumber desain (dan link khusus task bila ada), serta mode brainstorm atau plan.
  - **Actors:** A1, A2, A4
  - **Steps:** Agent membaca kode dan sumber desain; kolom Design di buku AC terisi; plan mencakup semua perilaku AC; dev menyetujui plan; selisih ditulis ke tracker.
  - **Outcome:** Dev dan agent tahu sejak awal perilaku AC mana yang tidak ada di desain.
  - **Covered by:** R3–R9
- F2. Review dan gerbang sebelum PR
  - **Trigger:** Agent selesai mengerjakan; dev menekan Review atau Push & PR.
  - **Actors:** A1, A3, A4
  - **Steps:** Untuk repo web, dev menyalakan database, Mandor menyalakan app dari worktree, dan dev login di browser; reviewer menilai setiap AC; baris gagal dikirim ke agent atau dev memilih Push anyway.
  - **Outcome:** PR dibuat dengan ringkasan per AC, atau tertahan sampai AC lulus.
  - **Covered by:** R10–R22, R24–R33
- F3. Perbaikan otomatis setelah reviewer meminta perubahan
  - **Trigger:** Komentar reviewer tepercaya memicu perbaikan otomatis (Rilis 1).
  - **Actors:** A2, A3, A4
  - **Steps:** Agent memperbaiki; reviewer berjalan dengan aturan R13, sehingga tanpa app lokal yang menyala dan login dev, AC tampilan dinilai tidak bisa dipastikan; dalam mode blokir, AC yang gagal menahan push.
  - **Outcome:** Push otomatis hanya terjadi bila gerbang mengizinkannya.
  - **Covered by:** R13, R22, R23

### Acceptance Examples

- AE1. **Covers R20, R22.** Given mode blokir dan review menilai AC-1 lulus, AC-2 tidak bisa dipastikan, dan AC-3 gagal, when dev menekan Push & PR, then tombol terkunci karena AC-3; setelah AC-3 lulus, Push & PR meminta konfirmasi dev untuk AC-2, lalu PR mencatat AC-2 sebagai dicek manual.
- AE2. **Covers R19.** Given mode peringatan dan AC-3 gagal, when dev menekan Push & PR, then dialog konfirmasi menampilkan AC-3 dan PR tetap bisa dibuat tanpa alasan tertulis.
- AE3. **Covers R21, R31.** Given mode blokir dan AC-3 gagal karena di luar kendali dev, when dev menekan Push anyway dan menulis alasannya, then PR dibuat dan deskripsinya menampilkan AC-3 sebagai dilewati beserta alasan itu.
- AE4. **Covers R16, R15.** Given review lulus, when agent mengubah kode setelahnya, then hasil itu ditandai usang dan Push & PR menjalankan review baru lebih dulu.
- AE5. **Covers R12, R13, R28.** Given repo bercara uji tanpa app, uji browser workspace dimatikan, atau app lokal tidak menyala, when review berjalan, then AC tampilan dinilai tidak bisa dipastikan dengan alasannya, bukan gagal.
- AE6. **Covers R23.** Given mode blokir, perbaikan otomatis selesai, dan review menilai satu AC gagal atau tidak bisa dipastikan, when daemon akan push otomatis, then push ditahan, task ditandai, dan dev menerima notifikasi.
- AE7. **Covers R2.** Given PM menambah AC baru di tracker setelah review lulus, when buku AC diperbarui, then baris baru muncul tanpa verdict dan Push & PR menunggu review berikutnya.
- AE8. **Covers R30.** Given reviewer mencoba membuka alamat di luar app lokal dan sumber desain terdaftar, atau berpindah ke tab lain, when tool browser dipanggil, then pagar menolaknya di luar prompt.
- AE9. **Covers R6 (Rilis 2.1).** Given deskripsi task memuat link Figma, when dev membuka dialog Start, then link itu ditawarkan sebagai sumber desain khusus task.
- AE10. **Covers R8a.** Given AC-4 meminta tombol Hapus di daftar sedangkan desain terbaru memindahkannya ke halaman detail, when dev menandai AC-4 "ikuti desain" saat menyetujui plan, then agent mengikuti desain, AC-4 tidak menahan Push & PR, dan task di tracker mendapat permintaan memperbarui AC-4.

### Success Criteria

- Jumlah task yang kembali dari reviewer karena perilaku AC terlewat turun setelah Rilis 2 dipakai, dibandingkan baseline dari task yang dikerjakan lewat Mandor Rilis 1 dalam periode yang sama panjangnya. Target penurunan dan jendela waktunya ditetapkan sebelum Rilis 2 dipakai.
- Kepercayaan pada gerbang terjaga: tingkat Push anyway per workspace dan porsi AC berstatus lulus yang kemudian dikembalikan reviewer manusia dipantau; mode blokir sebagai bawaan ditinjau ulang bila salah satunya tinggi.
- Desainer mulai memperbarui desain berdasarkan selisih yang ditulis ke tracker.
- Tim dengan sumber desain berbeda (kode, Figma, URL, atau gambar) dapat memakai alur yang sama tanpa perubahan di luar Settings (setelah Rilis 2.1).

### Scope Boundaries

**Deferred for later**

- Jenis sumber desain lain, seperti Penpot, Sketch, atau Zeplin.
- Uji app mobile di emulator atau perangkat; repo mobile memakai cara uji tanpa app.
- Browser di luar keluarga Chrome.
- Integrasi dua arah dengan alat desain, seperti mengubah atau berkomentar di Figma.
- Reviewer menyiapkan database, menjalankan migrasi, atau login sendiri, serta sanitasi dump data.
- Menyalakan app untuk beberapa run sekaligus.

**Separate track: Mandor untuk umum**

- Rilis 1 masih terikat pada skema tracker tim pertama: nama field Lark, nilai Task Status dan Development Readiness, AC di tabel tertaut, dan PR hanya lewat GitHub. Pemetaan field dan status yang bisa dikonfigurasi, sumber AC yang fleksibel (deskripsi atau checklist), serta adapter tracker dan Git hosting lain direncanakan sebagai jalur terpisah, bukan di Rilis 2.

### Dependencies / Assumptions

- Dengan uji browser aktif, data aplikasi yang tampil di halaman ikut terbaca oleh model Claude; tim pertama menerima ini, dan tim lain dapat mematikannya (R28).
- Spike 2026-10-21 (Claude Code 2.1.306, Chrome 155 di Windows, daemon di WSL dengan jaringan NAT) membuktikan: `claude -p --chrome` di WSL mengendalikan Chrome harian di Windows lewat ekstensi Claude in Chrome, dan agent bekerja di tab yang ia buat sendiri. Hook `PreToolUse` menerima `url` dan `tabId` setiap navigasi, sehingga alamat di luar daftar ditolak di luar prompt. Tool skrip halaman juga bisa ditolak, dan agent tetap membaca halaman lewat teks halaman. Alternatif profil khusus lewat port debugging (CDP) berjalan di Windows, tetapi tidak terjangkau dari WSL tanpa mengubah jaringan sistem (mirrored networking atau port proxy), sehingga tidak dipilih.
- Uji browser bergantung pada ekstensi Claude in Chrome yang terpasang dan tersambung di browser dev; Mandor perlu memeriksanya sebagai prasyarat, seperti pemeriksaan prasyarat Rilis 1.
- Sumber desain berbentuk repo, folder, atau gambar tersedia di mesin daemon; Figma dan URL dapat dijangkau dari mesin itu.
- Baseline jumlah task yang kembali dari reviewer belum diukur. Alasan pengembalian (AC terlewat atau lainnya) perlu dicatat, misalnya lewat label atau field dari reviewer manusia, karena riwayat status saja tidak menunjukkan penyebabnya.

### Outstanding Questions

**Resolve Before Planning**

- Tidak ada. Pertanyaan browser dijawab oleh spike 2026-10-21 (lihat Dependencies / Assumptions): Chrome harian dev bisa dikendalikan lewat ekstensi Claude in Chrome dari `claude -p` di WSL, dan pagar alamat di hook terbukti bekerja.

**For the design brief** (dijawab di `docs/design/mandor-screens-prompt-rilis-2.md` §3–4; desainer boleh mengusulkan perubahan)

- Status tab AC review: tanpa AC, kolom Design saja, review berjalan, review gagal atau melewati batas waktu, hasil terkini, hasil usang.
- Tampilan hasil review usang (R16), baris AC baru atau berubah (R2), dan status tombol Push & PR saat review dibutuhkan.
- Status Run app: mati, menyala, menunggu port, gagal (dengan log), diambil alih run lain (R27), serta cara dev menyatakan sudah login.
- Cakupan setiap pengaturan (daftar sumber global; cara uji per repo; uji browser dan gerbang per workspace dengan kemungkinan override repo) beserta urutan prioritasnya, serta validasi atau uji koneksi tiap jenis sumber.

**Deferred to Planning** (semua diselesaikan di Planning Contract)

- Pemeriksaan alamat tab sesudah redirect (R30): KTD-31.
- Sesi Claude lain yang memakai ekstensi bersamaan, dan ekstensi tidak tersambung: KTD-32.
- Mengikat hasil review ke snapshot kode: KTD-28.
- Bentuk field tracker untuk selisih desain: KTD-34.
- Test di sesi reviewer yang hanya baca: KTD-29.
- Model dan pemakaian token reviewer: KTD-30.
- Penandaan usang (R16) dan perubahan AC di tracker (R2): KTD-27, KTD-28.

### Sources / Research

- `docs/plans/2026-10-18-001-feat-korlap-lark-lembur-plan.md`: R17–R18, KTD-10 (pagar agent dan MCP per run), KTD-11 (daemon satu-satunya yang push), KTD-14 (path pendamping hanya baca), KTD-22 (fase plan).
- `docs/design/mandor-design/mandor-design-handoff/docs/design-notes.md` §5 dan `docs/design/mandor-screens-prompt.md` §8: slot tab AC review dengan baris lulus/gagal per AC.
- `docs/progress/rilis-1.md`: fase brainstorm/plan, perintah `/skill`, workspace multi-repo dengan MCP terpilih, penahanan push untuk perubahan sensitif.
- `crates/mandor-lark/src/gateway.rs` (modul `field`) dan `crates/mandor-domain/src/gate.rs` (`READY_VALUES`): nama field dan nilai readiness tim pertama yang ditulis langsung di kode, dasar jalur "Mandor untuk umum".
- Agent lokal `audit-ac-kelola` mengaudit AC task PGS terhadap kelola v1, prototype (`kelola-mock`), dan staging v2; pola pencarian bagian desain-nya relevan untuk R7.

---


## Planning Contract

**Product Contract preservation:** changed: R7. Nilai "bertentangan dengan desain" ditambahkan agar baris yang ditandai "ikuti desain" (R8a) bisa terdeteksi. Pemilik produk menyetujuinya pada 2026-10-21. Ada juga tiga klarifikasi saat perencanaan:
- R15: Push & PR tanpa review yang berlaku memulai review dan menampilkan progresnya. Push tidak berlanjut otomatis setelah review selesai; dev menekan Push & PR lagi saat gerbang siap (KTD-35).
- R28: run satu repo memakai setelan uji browser milik repo itu, dengan bawaan yang sama seperti workspace (KTD-37).
- Success Criteria keempat berlaku setelah Rilis 2.1, karena 2.0 hanya membawa sumber desain folder/repo.

**Target repo:** semua kode ada di `mandor/` (repo `github.com/handokobeni/mandor`). Path di bawah relatif terhadap root `korlap-clone`.

### Key Technical Decisions

Penomoran melanjutkan rencana Rilis 1 agar KTD bisa dirujuk lintas dokumen.

- KTD-25. **Buku AC adalah tabel SQLite milik run, dengan aturan di `mandor-domain`.**
  - Baris dikunci oleh `record_id` AC di Lark (stabil), bukan urutan atau label.
  - Nilai Design, Verdict, status baris (New, Changed, Removed), dan evaluasi gerbang adalah fungsi murni di domain; store hanya menyimpan.
  - "Follow design" hanya dicatat di kolom Design. Baris dengan Design Follow design dikecualikan dari gerbang dan review; verdict-nya ditampilkan sebagai Follows design, bukan disimpan.
  - Riwayat review disimpan sebagai tabel terpisah agar verdict lama tetap terbaca (R32).
  - Alasan: gerbang, PR, tracker, dan UI membaca sumber yang sama, dan aturan gerbang bisa diuji tanpa I/O.
- KTD-26. **Agent mengisi buku AC lewat tool MCP milik daemon, bukan dengan blok teks di jawaban akhir.**
  - Subcommand baru `mandord mcp` adalah server MCP stdio kecil yang meneruskan panggilan ke daemon lewat WebSocket lokal.
  - Ia memakai token kapabilitas per giliran yang hanya berlaku untuk satu run dan satu peran: giliran plan hanya boleh mengisi Design, sesi reviewer hanya boleh mengisi Verdict.
  - `hello` yang membawa token kapabilitas membuka koneksi terbatas. Validasinya lewat trait verifier yang diimplementasikan di `mandord`, sehingga `mandor-api` tetap hanya bergantung pada `mandor-protocol`.
  - Koneksi terbatas itu hanya bisa memanggil metode ledger untuk run dan peran yang terikat. Metode lain mengembalikan Unauthorized, tidak ada broadcast event, dan koneksi ditutup saat token dicabut.
  - Setiap run kini selalu mendapat berkas `--mcp-config` (0600, di luar folder kerja agent), termasuk run satu repo. Isinya server ledger ditambah server workspace yang dicentang; port daemon dan token ada di env server ledger di berkas itu, tidak di argv.
  - MCP diimplementasikan sendiri di atas `serde_json` (initialize, tools/list, tools/call) tanpa crate baru.
  - Alasan: baris terisi satu per satu selama review (brief §3.2), hasil terstruktur per AC, dan 2.1 menambah tool Figma di server yang sama (R4).
- KTD-27. **Perubahan AC di tracker dideteksi dengan membaca ulang task induk dan AC untuk run aktif.**
  - Refresh membaca ulang record task lebih dulu untuk mendapat id AC yang tertaut saat ini, lalu mengambil AC-nya. Dengan begitu AC yang ditautkan dari sisi tabel AC tetap terbaca.
  - Pemicu: rekonsiliasi penuh (5 menit), saat tab AC review dibuka, sebelum review, dan sebelum publish.
  - Hash teks per AC menentukan statusnya:
    - AC baru menjadi New, dan teks yang berubah menjadi Changed. Keduanya dengan verdict Not reviewed.
    - AC yang di-drop atau dilepas menjadi Removed dan keluar dari gerbang.
    - Verdict lama tetap ada di riwayat.
  - Bila refetch sebelum publish gagal, publish tetap boleh. Dialog menampilkan peringatan, dan PR mencatat waktu sinkron AC terakhir.
- KTD-28. **Hasil review diikat ke snapshot kode berupa tree id git per repo anggota.**
  - Tree id dihitung dengan salinan index worktree sebagai index sementara (`GIT_INDEX_FILE`), lalu `git add -A` dan `git write-tree`. Index asli dan worktree run tidak berubah, dan stat cache salinan membuat berkas yang tidak berubah tidak di-hash ulang.
  - Snapshot diambil di awal dan akhir review, setelah setiap giliran agent selesai, saat app dijalankan, dan sebelum publish.
  - Review menjadi usang bila snapshot saat ini berbeda dari snapshot di awal review.
  - Bila snapshot akhir review berbeda dari awalnya, review langsung ditandai usang dengan alasan "review changed files".
- KTD-29. **Reviewer berjalan di worktree run, dan agent pelaksana dikunci selama review.**
  - `RunState::Reviewing` baru dihitung sibuk (`is_busy`), sehingga pesan dev diantrekan dan Push & PR terkunci. Antrean dijalankan setelah review selesai.
  - Sesi reviewer adalah sesi Claude baru (bukan `--resume`), memakai hook mode `--review` yang menolak tool penulis berkas, `git` yang mengubah state, dan tool di luar daftar.
  - Test berjalan lewat allowlist repo yang sama dengan agent pelaksana. Efek samping test ke berkas tertangkap oleh KTD-28.
  - Alasan: worktree salinan tidak punya dependensi terpasang, sehingga test tidak bisa jalan di sana.
- KTD-30. **Model dan biaya reviewer bisa diatur dan dibatasi.**
  - Bawaannya model yang sama dengan agent pelaksana. Settings › General dapat menggantinya.
  - Setiap review memakai `--max-budget-usd` (bawaan 2 USD, bisa diatur) dan batas waktu 20 menit.
  - Usage dan biaya dari event `result` disimpan di catatan review, menjadi angka nyata untuk menyetel bawaan dan untuk anggaran Lembur Rilis 3.
  - Review yang terhenti karena batas biaya, waktu, crash, atau restart berstatus Failed dengan alasannya.
- KTD-31. **Pagar browser memakai hook `PreToolUse` dan `PostToolUse` dengan state per review milik daemon.**
  - `PostToolUse` (`mandord hook --post --review <id>`) berjalan setelah setiap tool browser, bukan hanya navigasi. Ia mencatat tab yang dibuat reviewer dan alamat terakhir tab dari respons tool. State disimpan di berkas 0600 di bawah `~/.mandor/agent/review/` yang ditolak bagi agent.
  - Respons tool interaksi (`computer`, `form_input`, `find`) yang tidak memuat alamat yang bisa dibaca menandai tab itu "tidak diketahui". Dengan begitu klik atau redirect JavaScript ke luar origin tidak lolos.
  - `PreToolUse` menolak tiga hal: navigasi ke origin di luar daftar, tool apa pun pada tab yang bukan milik reviewer, dan tool pembaca halaman bila alamat terakhir tab di luar daftar atau tidak diketahui. Screenshot lewat `computer` dihitung pembaca halaman.
  - Daftar origin berisi URL app kanonik run itu (KTD-36), ditambah URL desain terdaftar di 2.1.
  - Tool yang selalu ditolak: `javascript_tool`, `read_network_requests`, `read_console_messages`, unggah berkas atau gambar, `gif_creator`, `shortcuts_execute`, serta pindah atau pilih browser.
  - Bila respons tidak bisa dibaca, pagar gagal-tertutup dan reviewer harus menilai AC tampilan Can't verify.
  - Alasan: `PreToolUse` hanya melihat alamat yang diminta, sedangkan redirect dan perpindahan halaman baru terlihat sesudahnya (R30).
- KTD-32. **Ekstensi dipakai bersama sesi lain lewat kepemilikan tab, dan ekstensi yang terputus diturunkan menjadi Can't verify.**
  - Reviewer hanya bekerja di tab yang ia buat (KTD-31), jadi sesi Claude lain milik dev tidak terganggu dan tidak mengganggu. Asumsi ini (A5) dibuktikan di U9.
  - Status tersambung diperiksa sesuai permintaan dari checklist kesiapan, dengan cache 60 detik (U13).
  - Bila ekstensi terputus di tengah review, panggilan browser gagal, dan prompt reviewer mewajibkan Can't verify dengan alasan "Claude in Chrome extension not connected".
- KTD-33. **Sumber desain 2.0 berjenis folder atau repo, dan diberikan ke agent sebagai `--add-dir` yang hanya baca.**
  - Mekanismenya sama dengan path pendamping KTD-14, ditambah dua aturan hook. Pertama, path dikanonisasi (`realpath`) dan harus berada di bawah akar yang terdaftar, sehingga symlink tidak bisa dipakai untuk keluar dari akar. Kedua, Edit dan Write di akar itu ditolak (R7a).
  - Daemon tidak bisa membungkus isi yang dibaca agent lewat tool. Karena itu prompt operator menyatakan bahwa semua isi dari Read di akar desain dan dari tool browser adalah data tidak tepercaya, bukan instruksi; ini cara R7a dipenuhi untuk isi lewat tool.
  - Daftar sumber desain disimpan di tabel global `design_sources` dengan kolom `kind` dan `config_json`; jenis 2.1 menambah nilai `kind` tanpa mengubah alur.
  - Pilihan sumber terakhir di Start disimpan di localStorage seperti pilihan planning.
- KTD-34. **Selisih desain ditulis ke field teks milik app `mandor_design_gaps` lewat outbox.**
  - Field dibuat oleh `ensure_app_fields`.
  - Isinya satu baris per AC yang Missing from design, Partly in design, Conflicts with design, atau Follow design. Contoh: "AC-4: Missing from design: konfirmasi sebelum hapus", atau "AC-7: Follow design: PM please update the AC".
  - Panjangnya dibatasi 4.000 karakter, dengan penutup "+N more in Mandor".
  - Field ditulis saat plan disetujui, dan dikosongkan bila tidak ada selisih.
  - Tulisan memakai `WriteOp::Fields` tanpa transisi status, jadi validasi ulang outbox (KTD-7) tetap berlaku.
  - Runbook menjelaskan view "Design gaps" dengan filter field tidak kosong (R9).
- KTD-35. **Gerbang dievaluasi di daemon pada awal `publish`, sebelum state Publishing.**
  - `publish` menerima keputusan dev dalam satu panggilan: daftar AC yang dikonfirmasi "checked manually", dan alasan Push anyway bila ada.
  - Domain menghitung hasilnya: Ready, NeedsReview, Blocked (dengan AC yang gagal), atau NeedsConfirmation (dengan AC yang Can't verify).
  - NeedsReview memulai review dan mengembalikan `review_started`. Push tidak berlanjut otomatis setelah review selesai.
  - Blocked dan NeedsConfirmation ditolak dengan rinciannya, agar client menampilkan dialog yang sesuai.
  - Mode peringatan: Fail dan Can't verify hanya ditampilkan di satu dialog konfirmasi, tanpa alasan tertulis dan tanpa konfirmasi per baris (R22).
  - Ringkasan per AC ditambahkan ke badan PR di `create_pr` (R31). Isinya hanya label, status, alasan, dan langkah, tanpa isi halaman (R33).
- KTD-36. **Run app adalah proses supervisi milik daemon, satu run pemilik dalam satu waktu.**
  - `AppService` menyalakan setiap repo anggota yang punya perintah start, berurutan sesuai daftar anggota.
  - Bila repo punya `setup_command` (mis. `bun install`) dan lockfile berubah sejak setup terakhir, setup dijalankan lebih dulu dengan timeout 10 menit dan lognya disimpan.
  - Perintah start dijalankan sebagai `sh -c '<start_command>; :' mandor-app-<run_id>` di worktree anggota. Akhiran `; :` mencegah `exec`, sehingga penanda tetap ada di command line pemimpin group. Proses berjalan sebagai `Supervised` (process group, KTD-17), dan pgid-nya disimpan di state app run.
  - Kesiapan diukur dengan probe TCP ke port repo: interval 1 detik, batas 180 detik per repo.
  - Setiap repo punya satu URL app kanonik (bawaan `http://localhost:<port>`, bisa diganti). URL ini dipakai sebagai alamat login dev dan satu-satunya origin reviewer, karena cookie login terikat ke host.
  - Log stdout/stderr disimpan di ring buffer 256 KB per repo, sudah melewati `redact`.
  - Environment memakai basis yang sudah dibersihkan seperti `agent_env`: tanpa token, tanpa SSH agent, dan tanpa kunci API. Di atasnya hanya kunci `.env` yang dipilih. `.env` dibaca dari folder repo asli, tidak ditulis ke worktree, dan nilainya disimpan sebagai `SecretString` yang tidak pernah masuk log.
  - Menyalakan app run lain menghentikan app sebelumnya lebih dulu.
  - Saat daemon start, group app yatim dibunuh lewat pgid tersimpan dan penanda (`reap_group`), dan app tidak dinyalakan ulang otomatis.
  - Konfirmasi R25a dibuktikan di daemon. Client menampilkan daftar berkas yang berubah per repo (dibandingkan dengan snapshot app terakhir, atau base branch saat app pertama kali dijalankan), dengan penanda pada manifest paket, skrip, dan berkas perintah start. Client mengirim snapshot yang ditampilkan itu, dan daemon menolak bila snapshot saat ini berbeda.
  - Snapshot saat app dijalankan disimpan. Konfirmasi dev bahwa database menyala dan bahwa ia sudah login disimpan di state app milik run, dan direset saat app berhenti.
- KTD-37. **Setelan baru menumpang di tempat yang sudah ada.**
  - Per repo, disimpan di `settings_json` tanpa migrasi: cara uji (Web/No app), perintah setup, perintah start, port, URL app, kunci `.env`, override gerbang, dan uji browser untuk run satu repo.
  - Per workspace, lewat kolom baru `settings_json` di tabel `workspaces`: uji browser dan mode gerbang.
  - Global (General), lewat tabel kunci-nilai `settings` di migrasi 0005: model dan batas biaya reviewer. Nilainya dibaca saat review dimulai, jadi perubahan berlaku tanpa restart.
  - Bawaan: gerbang Block, uji browser aktif, dan cara uji No app sampai dev mengisi perintah start.
  - Prioritas gerbang: override repo > workspace > bawaan. Untuk run multi-repo, mode paling ketat di antara anggotanya yang berlaku.
- KTD-38. **Prompt reviewer bawaan atau skill plugin, dengan data tidak tepercaya dibungkus seperti KTD-15.**
  - Prompt operator reviewer tetap untuk satu sesi. AC dan task masuk sebagai blok data bernonce.
  - Isi lewat tool diperlakukan sebagai data (KTD-33). Reviewer mengabaikan teks halaman yang ditujukan kepadanya, dan setiap Pass untuk AC tampilan wajib menyebut langkah konkret.
  - Prompt memuat fakta kesiapan: app menyala dari snapshot yang sama dengan awal review, login dikonfirmasi, uji browser diizinkan, dan ekstensi tersambung. Aturan R13 berlaku: tanpa semua fakta itu, AC tampilan dinilai Can't verify dengan alasan yang tepat, mis. "app running older code".
  - Bila dev memilih skill, pesan pertama berisi instruksi memakai tool `Skill` (pola KTD-24). Tool MCP ledger tetap satu-satunya cara melapor.
  - Setelah sesi selesai normal, setiap AC aktif yang tidak dilaporkan menjadi Can't verify dengan alasan "reviewer did not report". Review yang Failed tidak mengisi apa pun: AC yang belum dilaporkan tetap Not reviewed, sehingga gerbang meminta review ulang dan tidak bisa dilewati hanya dengan konfirmasi manual.
- KTD-39. **Checklist kesiapan adalah dialog pra-review.** Dialog ini muncul setiap kali review dimulai dari Run review atau dari Push & PR, untuk run yang punya repo Web dan uji browser diizinkan. Item yang kurang menjelaskan cara memperbaikinya, dan dev bisa memilih "Run review anyway". Review dari perbaikan otomatis melewati dialog ini dan memakai fakta kesiapan apa adanya.
- KTD-40. **Review perbaikan otomatis adalah lanjutan milik engine Waiting Fix.**
  - `start_review(trigger=AutomaticFix)` boleh dimulai dari loop perbaikan setelah giliran perbaikan yang menghasilkan perubahan; state run menjadi Reviewing.
  - Setelah review selesai, engine yang meneruskan, bukan kembali ke AwaitingDev:
    - Gerbang Ready, atau mode Warn: jalur Publishing → `publish_members` → `mark_waiting_review` yang sudah ada.
    - Mode Block dengan Fail atau Can't verify: `flag_fix(run, AwaitingDev, "Push held")` dan notifikasi.
    - Review Failed: push juga ditahan.

### High-Level Technical Design

Komponen dan arah dependensi (aturan crate KTD-0 tetap):

```mermaid
flowchart TB
  UI[Desktop client: AC review tab, gate dialogs, Run app, Settings] -->|JSON-RPC| API[mandor-api + mandord dispatch]
  API --> RS[usecase: ReviewService]
  API --> AS[usecase: AppService]
  API --> RUN[usecase: RunService publish/approve_plan]
  RUN --> WF[usecase: waiting_fix engine]
  WF --> RS
  RS --> DOM[mandor-domain: ac_ledger, push_gate, hook review mode]
  RUN --> DOM
  AS --> DOM
  RS --> ST[mandor-store: ac_rows, reviews, design_sources, settings]
  RS --> AG[mandor-agent: reviewer session spec]
  AS --> PR[mandor-proc: Supervised, log ring, port probe]
  RUN --> GIT[mandor-git: snapshot tree, PR body]
  RUN --> LK[mandor-lark: mandor_design_gaps]
  AG -.spawn.-> CL[claude -p --chrome]
  CL -.MCP stdio.-> MCP[mandord mcp: ledger tools]
  MCP -.capability hello, ledger methods only.-> API
  CL -.Pre/PostToolUse.-> HK[mandord hook --review]
```

Alur review dan gerbang:

```mermaid
sequenceDiagram
  participant Dev
  participant D as mandord
  participant R as Reviewer (claude -p)
  participant M as mandord mcp
  Dev->>D: Run review (checklist kesiapan) atau publish tanpa review berlaku
  D->>D: baca ulang task dan AC, snapshot awal, state Reviewing, token review
  D->>R: sesi baru, hook --review, --chrome bila siap
  loop per AC
    R->>M: ac_set_verdict(ac, status, evidence)
    M->>D: tulis baris, event review.progress
  end
  R-->>D: Done (usage, biaya) atau Failed
  D->>D: selesai normal: AC tanpa laporan jadi Can't verify; snapshot akhir; simpan review
  D->>D: jalankan pesan antrean, atau serahkan ke engine Waiting Fix (KTD-40)
  D-->>Dev: notifikasi, gerbang dihitung ulang
  Dev->>D: publish(confirm_manual, push_anyway_reason)
  D->>D: push_gate: Ready / Blocked / NeedsConfirmation / NeedsReview
  D->>D: commit, push, PR dengan ringkasan per AC
```

Verdict per baris (flag stale terpisah dari verdict, KTD-28):

```mermaid
stateDiagram-v2
  [*] --> NotReviewed
  NotReviewed --> Pass: reviewer
  NotReviewed --> Fail: reviewer
  NotReviewed --> CantVerify: reviewer, atau tidak dilaporkan di review yang selesai normal
  Fail --> FixInProgress: Send to agent
  FixInProgress --> NotReviewed: review berikutnya dimulai
  CantVerify --> CheckedManually: konfirmasi dev saat publish
  Fail --> Skipped: Push anyway dengan alasan
  note right of NotReviewed
    Verdict apa pun kembali ke NotReviewed saat review baru dimulai
    atau saat teks AC berubah di tracker (KTD-27).
    Snapshot yang berubah memberi flag stale tanpa menghapus verdict.
    Baris dengan Design Follow design dikecualikan dan tidak punya verdict.
  end note
```

Keputusan gerbang (domain `push_gate`, mode blokir):

| Kondisi baris aktif (selain Follow design dan Removed) | Hasil |
|---|---|
| Ada baris tanpa review berlaku (Not reviewed, stale, Fix in progress) | NeedsReview |
| Ada Fail tanpa alasan Push anyway | Blocked |
| Ada Can't verify yang belum dikonfirmasi | NeedsConfirmation |
| Semua Pass, Checked manually, atau Skipped | Ready |

Mode peringatan: NeedsReview tetap memulai review. Fail dan Can't verify cukup dikonfirmasi lewat satu dialog di client, tanpa alasan tertulis (KTD-35).

### Assumptions

Asumsi yang harus dibuktikan sebelum atau di unit yang menyentuhnya. Bila salah, berhenti dan laporkan (Goal Capsule).

- A1. `claude -p --chrome` tetap memuat Claude in Chrome bersama `--strict-mcp-config` dan `--mcp-config` yang berisi server stdio. Spike hanya membuktikan `--chrome` tanpa flag MCP. A1 dibuktikan di spike S0 sebelum U5, dengan server MCP stdio dummy.
- A2. Mengedit record di tabel AC tidak mengubah `last_modified_time` task induknya. KTD-27 tetap benar walau asumsi ini salah; asumsi ini hanya menentukan apakah refetch berkala bisa dikurangi. Dibuktikan di U3 dengan record uji, termasuk menautkan dan melepas AC dari sisi tabel AC.
- A3. Respons tool Claude in Chrome yang terlihat oleh `PostToolUse` (`navigate`, `tabs_create_mcp`, `computer`, `find`, `form_input`) memuat alamat tab dan id tab yang bisa dibaca. Dibuktikan di U8 dengan fixture yang direkam dari CLI. Bila tidak, gagal-tertutup (KTD-31).
- A4. `--max-budget-usd` menghentikan sesi `-p` dengan event `result` yang bisa dibedakan dari error biasa. Dibuktikan di U9 dengan fixture stream.
- A5. Claude in Chrome melayani sesi reviewer bersamaan dengan sesi Claude lain yang aktif, tanpa memutus koneksi salah satunya. Dibuktikan manual di U9. Bila salah, readiness checklist menambahkan item "Close other Claude in Chrome sessions".

### Sequencing

```mermaid
flowchart TB
  S0[S0 smoke Rilis 1 + spike A1] --> U5
  S0 --> U8
  S0 --> U11
  U1[U1 domain ledger + gate] --> U2[U2 store]
  U2 --> U3[U3 ledger sync]
  U1 --> U4[U4 snapshot]
  U2 --> U5[U5 MCP ledger server]
  U3 --> U6[U6 design sources]
  U5 --> U6
  U6 --> U7[U7 follow design + tracker]
  U1 --> U8[U8 hook review mode]
  U3 --> U9[U9 reviewer session]
  U4 --> U9
  U5 --> U9
  U8 --> U9
  U10[U10 Run app] --> U9
  U9 --> U11[U11 gate, PR, auto-fix]
  U7 --> U11
  U11 --> U12[U12 UI AC review + gate]
  U10 --> U13[U13 UI Run app + readiness]
  U6 --> U14[U14 UI settings, Start, plan approval, board]
  U7 --> U14
  U12 --> U15[U15 runbook + smoke]
  U13 --> U15
  U14 --> U15
```

- **S0** bukan unit kode. Isinya smoke test Rilis 1 (U14 di rencana Rilis 1) dan spike A1. Unit yang memperluas jalur MCP, hook, dan perbaikan otomatis Rilis 1 (U5, U8, U11) baru dimulai setelah S0 tercatat di `docs/progress/rilis-2.md`.
- U1–U4 dan U10 tidak bergantung pada S0 dan bisa dikerjakan sejak awal.

---

## Implementation Units

| U-ID | Judul | File utama | Bergantung pada |
|---|---|---|---|
| U1 | Model buku AC dan gerbang di domain | `mandor-domain/src/ac_ledger.rs`, `push_gate.rs` | - |
| U2 | Store buku AC, review, sumber desain, setelan | `mandor-store/migrations/0005_ac_review.sql`, `ac_ledger.rs` | U1 |
| U3 | Sinkron buku AC dengan tracker | `mandor-usecase/src/review/ledger.rs` | U2 |
| U4 | Snapshot kode dan penanda usang | `mandor-git/src/snapshot.rs` | U1 |
| U5 | Server MCP ledger milik daemon | `mandor-api/src/mcp.rs`, `mandor-api/src/server.rs`, `mandord/src/mcp.rs` | U2, S0 |
| U6 | Sumber desain dan kolom Design | `mandor-usecase/src/review/design.rs`, `mandor-agent/src/prompt.rs` | U3, U5 |
| U7 | Follow design dan selisih ke tracker | `run/session.rs`, `fields_bootstrap.rs` | U6 |
| U8 | Hook mode review dan pagar browser | `mandor-domain/src/hook.rs`, `mandord/src/hook.rs` | U1, S0 |
| U9 | Sesi reviewer | `mandor-usecase/src/review/service.rs` | U3, U4, U5, U8, U10 |
| U10 | Run app | `mandor-usecase/src/app_run.rs`, `mandor-proc/src/app.rs` | - |
| U11 | Gerbang publish, ringkasan PR, perbaikan otomatis | `run/publish.rs`, `run/members.rs`, `waiting_fix/engine.rs` | U9, U7, S0 |
| U12 | UI tab AC review dan dialog gerbang | `AcReviewTab.svelte`, `lib/ac-review.ts` | U11 |
| U13 | UI Run app dan checklist kesiapan | `RunAppControl.svelte`, `lib/app-run.ts` | U10 |
| U14 | UI Settings, Start, persetujuan plan, board | `DesignSourceSettings.svelte`, `PlanningPicker.svelte` | U6, U7 |
| U15 | Runbook dan smoke Rilis 2.0 | `docs/runbook/`, `mandor-e2e/tests/rilis2_smoke.rs` | U12, U13, U14 |

Semua path crate di tabel relatif terhadap `crates/`, dan path frontend terhadap `apps/desktop/src/lib/`.

### U1. Model buku AC dan gerbang di domain

- **Goal:** Aturan murni buku AC: identitas baris, nilai Design dan Verdict, status New, Changed, dan Removed, flag usang, evaluasi gerbang, serta teks ringkasan untuk PR dan tracker.
- **Requirements:** R1, R2, R7, R8a, R10, R16, R19–R22, R31, R33; AE1, AE2, AE3, AE7, AE10.
- **Dependencies:** -
- **Files:**
  - `crates/mandor-domain/src/ac_ledger.rs` (baru)
  - `crates/mandor-domain/src/push_gate.rs` (baru)
  - `crates/mandor-domain/src/lib.rs`
  - Test inline di setiap modul, mengikuti pola `run.rs` dan `hook.rs`.
- **Approach:**
  - Tipe utama:
    - `AcRow`: ac record id, label, hash teks, design, verdict, evidence, status baris, flag stale.
    - `Design`: NotChecked, InDesign, Partly, Missing, Conflicts, FollowDesign.
    - `Verdict`: NotReviewed, Pass, Fail, CantVerify, CheckedManually, Skipped, FixInProgress.
    - `GateMode`: Block, Warn.
  - Baris dengan Design FollowDesign dikecualikan dari gerbang; tampilannya sebagai Follows design diturunkan, tidak disimpan sebagai verdict (KTD-25).
  - `reconcile_rows(existing, fetched)` menghasilkan baris baru dan perubahan sesuai KTD-27.
  - `evaluate(rows, mode, decision)` mengembalikan hasil gerbang KTD-35.
  - `pr_summary(rows, ac_synced_at)` dan `design_gaps_text(rows, limit)` menghasilkan teks tanpa isi halaman.
  - Tidak ada I/O, tidak ada crate baru.
- **Execution note:** Implement new domain behavior test-first.
- **Patterns to follow:** `crates/mandor-domain/src/run.rs` (enum state dengan helper), `crates/mandor-domain/src/gate.rs` (fungsi keputusan murni).
- **Test scenarios:**
  - Covers AE1. Mode Block dengan AC-1 Pass, AC-2 CantVerify, dan AC-3 Fail menghasilkan Blocked [AC-3]. Setelah AC-3 Pass hasilnya NeedsConfirmation [AC-2]. Dengan konfirmasi AC-2 hasilnya Ready, dan `pr_summary` memuat "AC-2: Checked manually".
  - Satu keputusan berisi alasan Push anyway untuk AC-3 dan konfirmasi AC-2 menghasilkan Ready dalam satu evaluasi.
  - Covers AE2. Mode Warn dengan AC-3 Fail dan AC-2 CantVerify menghasilkan hasil yang bisa dikonfirmasi tanpa alasan dan tanpa konfirmasi per baris.
  - Covers AE3. Mode Block dengan AC-3 Fail plus alasan Push anyway menghasilkan Ready, AC-3 menjadi Skipped, dan ringkasan memuat alasan itu.
  - Covers AE7. `reconcile_rows` dengan AC baru setelah semua Pass menambah baris New dengan NotReviewed, dan gerbang menjadi NeedsReview.
  - Teks AC berubah: baris Changed dengan NotReviewed, apa pun verdict sebelumnya. AC menjadi dropped: baris Removed dan diabaikan gerbang.
  - Covers AE10. Baris dengan Design FollowDesign diabaikan gerbang, tidak bisa dipilih untuk Send to agent, dan muncul di `design_gaps_text` sebagai permintaan update AC.
  - Baris Conflicts dan Missing muncul di `design_gaps_text` dengan labelnya masing-masing.
  - Baris dengan flag stale membuat gerbang NeedsReview tanpa menghapus verdict-nya.
  - `design_gaps_text` di atas batas 4.000 karakter dipotong di batas baris, dengan penutup "+N more in Mandor".
  - Tidak ada baris aktif (task tanpa AC): gerbang Ready, dan ringkasan PR menyebut "No acceptance criteria".
  - `pr_summary` dengan waktu sinkron AC yang diberikan mencatat "AC last synced at …".
- **Verification:** Semua skenario lulus; `mandor-domain` tetap tanpa dependensi I/O.

### U2. Store buku AC, riwayat review, sumber desain, dan setelan

- **Goal:** Persistensi baris AC per run, catatan review, verdict per review, sumber desain, setelan global, dan setelan workspace.
- **Requirements:** R1, R2, R3 (jenis folder/repo), R32.
- **Dependencies:** U1
- **Files:**
  - `crates/mandor-store/migrations/0005_ac_review.sql`: tabel `ac_rows`, `reviews`, `review_verdicts`, `design_sources`, `settings` (kunci-nilai), dan kolom `workspaces.settings_json`.
  - `crates/mandor-store/src/ac_ledger.rs`, `src/reviews.rs`, `src/design_sources.rs`, `src/settings.rs` (baru)
  - `crates/mandor-store/src/workspaces.rs`, `src/lib.rs`
  - `crates/mandor-store/.sqlx/` (dihasilkan ulang)
  - `crates/mandor-store/tests/review_store.rs` (baru)
- **Approach:**
  - Ikuti pola `workspaces.rs`: struct baru, `impl Store`, `query!`, dan `StoreError::from_sqlx`.
  - Port `LedgerStore`, `ReviewStore`, `DesignSourceStore`, dan `SettingsStore` didefinisikan di `mandor-usecase` dan masuk bundel `Deps`; adapternya di `run_port.rs` atau modul baru.
  - Retensi (R32): simpan 20 review terakhir per run, dan hapus review dari run yang ditutup lebih dari 90 hari. Pemangkasan dijalankan sekali sehari dari loop `session.rs`.
  - Data per run yang kecil (state app dengan pgid dan snapshot, snapshot review terakhir, waktu sinkron AC) masuk `RunExtra` tanpa migrasi (KTD-23).
- **Patterns to follow:** `crates/mandor-store/src/workspaces.rs`, `crates/mandor-store/src/run_port.rs`.
- **Test scenarios:**
  - Simpan baris lalu baca ulang setelah `Store::open` ulang: data sama (R2 restart).
  - Upsert baris dengan id AC sama memperbarui, bukan menduplikasi.
  - Review baru dengan verdict per AC tersimpan; review lama tetap bisa dibaca read-only.
  - Pemangkasan: run tertutup 91 hari lalu kehilangan riwayatnya, run aktif tidak; review ke-21 menghapus yang tertua.
  - Sumber desain: tambah, ubah, hapus. Nama ganda ditolak dengan error domain, bukan error sqlx mentah.
  - Setelan global: kunci yang belum ada mengembalikan bawaan; tulis lalu baca mengembalikan nilai baru.
  - Migrasi pada database Rilis 1 yang sudah berisi data berjalan tanpa kehilangan run.
- **Verification:** `make sqlx-prepare` bersih; `make check` hijau.

### U3. Sinkron buku AC dengan tracker

- **Goal:** Buku AC dibuat saat Start dan diperbarui saat AC di Lark berubah selama run.
- **Requirements:** R1, R2; AE7.
- **Dependencies:** U2
- **Files:**
  - `crates/mandor-usecase/src/review/mod.rs`, `src/review/ledger.rs` (baru)
  - `crates/mandor-usecase/src/run/start.rs` (inisialisasi buku AC dari AC yang sudah diambil; id AC dipertahankan)
  - `crates/mandor-usecase/src/sync/mod.rs` (baca ulang satu record task untuk link AC terbaru)
  - `crates/mandord/src/session.rs` (refetch saat rekonsiliasi penuh)
  - `crates/mandor-protocol/src/dto.rs`, `src/lib.rs` (`AcLedgerView`, `AcRowView`, metode `ledger.get`, event `ledger.updated`)
  - `crates/mandord/src/dispatch/review.rs` (baru)
  - `crates/mandor-usecase/tests/ac_ledger.rs` (baru)
- **Approach:**
  - `start.rs` saat ini memformat AC menjadi `Vec<String>` dan membuang id-nya. Ubah agar `AcItem` utuh dipakai untuk buku AC; prompt tetap menerima label dan teks.
  - `refresh_ledger(run_id)` berjalan dalam empat langkah:
    1. Baca ulang record task untuk id AC yang tertaut saat ini.
    2. Ambil AC lewat `sync.lark.acceptance_criteria`.
    3. Panggil `reconcile_rows`, lalu simpan hasil dan waktu sinkron.
    4. Pancarkan `ledger.updated`.
  - Pemicu sesuai KTD-27. Panggilan Lark dibatasi ke run aktif milik pengguna, dan memakai timeout klien Lark yang sudah ada.
  - Hasil refresh yang gagal dikembalikan sebagai status (`synced_at` lama plus error), agar publish bisa memberi peringatan (KTD-27).
- **Patterns to follow:** `crates/mandor-usecase/src/run/start.rs` (pengambilan AC), `crates/mandord/src/dispatch/workspaces.rs` (sub-dispatch).
- **Test scenarios:**
  - Start dengan tiga AC, salah satunya dropped, membuat dua baris Not checked / Not reviewed.
  - Covers AE7. Refresh setelah PM menambah AC menghasilkan baris New dan event `ledger.updated`.
  - AC yang ditautkan dari sisi tabel AC (link task berubah, cache task lama) tetap muncul, karena task dibaca ulang lebih dulu.
  - AC yang dilepas dari task menjadi Removed.
  - Refresh saat Lark gagal atau timeout membiarkan buku AC apa adanya dan mengembalikan error beserta `synced_at` lama. Run tidak gagal.
  - Run ditutup: refresh berkala tidak lagi menyentuh run itu.
  - A2: test integrasi manual dengan record uji mencatat apakah waktu ubah task berubah saat AC diedit, ditautkan, atau dilepas. Hasilnya ditulis di `docs/progress/rilis-2.md`.
- **Verification:** Test usecase dengan fake gateway lulus; TS binding dihasilkan ulang.

### U4. Snapshot kode dan penanda usang

- **Goal:** Hasil review terikat ke tree id per repo anggota, dan perubahan kode menandai hasil itu usang.
- **Requirements:** R16; AE4.
- **Dependencies:** U1
- **Files:**
  - `crates/mandor-git/src/snapshot.rs` (baru)
  - `crates/mandor-git/src/port.rs`
  - `crates/mandor-usecase/src/run_ports.rs` (`GitPort::snapshot`, `GitPort::changed_files_between`)
  - `crates/mandor-usecase/src/run/session.rs` (`finish_turn` menghitung snapshot dan memberi flag stale)
  - `crates/mandor-git/tests/snapshot.rs` (baru)
- **Approach:**
  - Salin index asli worktree (path dari `git rev-parse --git-path index`) ke berkas temp milik daemon, lalu jalankan `git add -A` dan `git write-tree` dengan `GIT_INDEX_FILE` menunjuk ke salinan itu.
  - Git dijalankan dengan pagar KTD-11 (`core.hooksPath=/dev/null`, `fsmonitor=false`) dan timeout `Cmd`.
  - `Snapshot` berisi peta repo ke tree id. Bila berbeda dari snapshot review terakhir, semua baris diberi flag stale dan `ledger.updated` dipancarkan.
  - `changed_files_between(tree_a, tree_b)` dipakai oleh konfirmasi Run app (U10).
- **Patterns to follow:** `crates/mandor-git/src/diff.rs` (git di worktree dengan berkas untracked).
- **Test scenarios:**
  - Worktree tanpa perubahan menghasilkan tree id yang sama dua kali.
  - Berkas untracked baru mengubah tree id; index asli dan status worktree tetap sama sebelum dan sesudahnya.
  - Berkas yang diabaikan `.gitignore` (mis. `node_modules`) tidak mengubah tree id.
  - Covers AE4. Giliran agent yang mengubah berkas setelah review lulus memberi flag stale pada semua baris, dan gerbang menjadi NeedsReview.
  - `changed_files_between` mengembalikan daftar path yang berubah antara dua snapshot.
  - Git timeout menghasilkan error domain, dan baris dianggap stale (gagal-aman).
- **Verification:** Test git dengan repo sementara lulus.

### U5. Server MCP ledger milik daemon

- **Goal:** Agent dapat membaca buku AC dan mengisi Design atau Verdict lewat tool, dengan izin sesuai peran giliran dan tanpa akses ke RPC lain.
- **Requirements:** R7, R10 (pelaporan terstruktur); seam untuk R4.
- **Dependencies:** U2, S0 (A1 terbukti)
- **Files:**
  - `crates/mandor-api/src/mcp.rs` (baru: framing MCP stdio dan dispatch tool)
  - `crates/mandor-api/src/server.rs` (hello kapabilitas, trait verifier, koneksi terbatas tanpa broadcast)
  - `crates/mandor-protocol/src/hello.rs` (varian hello kapabilitas)
  - `crates/mandord/src/mcp.rs` (baru: subcommand `mandord mcp`, port dan token dari env proses MCP)
  - `crates/mandord/src/main.rs`, `crates/mandord/src/serve.rs` (implementasi verifier)
  - `crates/mandor-usecase/src/review/capability.rs` (baru: token per giliran di memori, dengan peran Design atau Verdict)
  - `crates/mandor-agent/src/port.rs` (berkas `--mcp-config` untuk setiap run, digabung dengan server workspace)
  - `crates/mandor-api/tests/mcp.rs`, `crates/mandor-api/tests/handshake.rs`, `crates/mandor-agent/tests/run.rs`
- **Approach:**
  - Tool:
    - `ac_ledger_read`: label, teks sebagai data, dan nilai saat ini.
    - `ac_set_design`: ac, nilai, lokasi desain.
    - `ac_set_verdict`: ac, status, bukti berisi berkas:baris, ringkasan test, dan langkah browser.
  - Token dibuat dengan `SecretString` dan dibandingkan waktu-konstan (`subtle`). Token hidup selama satu giliran, dicabut saat giliran selesai, dan dibangun ulang per giliran setelah restart, karena KTD-13 menghentikan semua sesi.
  - Koneksi terbatas mengikuti KTD-26.
  - Hook `--mcp` menambah server `mandor` ke daftar yang diizinkan.
  - Input tool divalidasi: AC harus milik run, status harus sesuai peran, panjang bukti dibatasi 2.000 karakter. Teks bukti diperlakukan sebagai data di UI.
  - Timeout: connect 5 detik, per panggilan 10 detik.
- **Execution note:** Mulai dari test kontrak MCP (initialize, tools/list, tools/call) dengan fixture JSON yang direkam dari `claude -p` versi yang di-pin.
- **Patterns to follow:** `crates/mandor-api/src/relay.rs` (klien WebSocket), `crates/mandord/src/hook.rs` (subcommand kecil yang gagal-tertutup).
- **Test scenarios:**
  - `tools/list` untuk token peran Design hanya menampilkan `ac_ledger_read` dan `ac_set_design`.
  - `ac_set_verdict` dengan token peran Design ditolak dengan error MCP, dan baris tidak berubah.
  - Token run A untuk AC milik run B ditolak.
  - Token yang dicabut (giliran selesai) ditolak, dan koneksinya ditutup.
  - Koneksi dengan hello kapabilitas yang memanggil `runs.publish` atau `lark.status` mendapat Unauthorized.
  - Koneksi kapabilitas tidak menerima event `run.updated` maupun `notice`.
  - Hello dengan token daemon tetap membuka koneksi penuh (regresi Rilis 1).
  - Bukti lebih dari 2.000 karakter dipotong dan diberi penanda.
  - Daemon tidak terjangkau: tool mengembalikan error, dan proses MCP tidak hang (timeout).
  - Run satu repo mendapat berkas mcp-config berisi server `mandor` saja; run workspace mendapat `mandor` plus server yang dicentang.
  - Token tidak muncul di argv proses `claude` maupun di log.
- **Verification:** Test kontrak dan handshake lulus; tidak ada crate baru; `cargo deny check` hijau.

### U6. Sumber desain dan kolom Design

- **Goal:** Dev mendaftarkan sumber desain folder atau repo, memilihnya saat Start, dan agent mengisi kolom Design saat brainstorm atau plan.
- **Requirements:** R3 (jenis 2.0), R5, R7, R7a, R8; F1.
- **Dependencies:** U3, U5
- **Files:**
  - `crates/mandor-usecase/src/review/design.rs` (baru: validasi dan uji sumber)
  - `crates/mandor-domain/src/hook.rs` (akar desain hanya baca, kanonisasi path)
  - `crates/mandor-agent/src/port.rs` (sumber desain menjadi `--add-dir`, dengan deny Edit/Write)
  - `crates/mandor-agent/src/prompt.rs` (instruksi plan/brainstorm, lihat Approach)
  - `crates/mandor-protocol/src/dto.rs` (`DesignSourceView`, `StartParams.design_sources`)
  - `crates/mandord/src/dispatch/review.rs` (`design_sources.list/add/update/remove/test`)
  - `crates/mandor-usecase/tests/plan_flow.rs`, `crates/mandor-agent/tests/guardrails.rs`
- **Approach:**
  - Prompt plan/brainstorm meminta agent membaca sumber dan mengisi `ac_set_design` per AC, termasuk Conflicts bila desain menunjukkan perilaku lain. Plan tetap mengerjakan semua perilaku AC. Prompt juga menyatakan isi akar desain sebagai data tidak tepercaya (KTD-33).
  - Uji sumber memeriksa tiga hal: path ada, berupa direktori, dan lolos aturan path hook (tidak memuat `.mandor`, bukan path kredensial).
  - Sumber yang hilang saat Start ditolak dengan pesan yang menyebut namanya.
  - Pilihan sumber disimpan di `RunExtra`, sehingga giliran lanjutan dan reviewer memakai daftar yang sama.
  - Kolom Design diisi lewat tool U5. AC yang tidak dilaporkan agent tetap Not checked; tidak ada tebakan otomatis.
  - Run tanpa sumber desain ditandai di buku AC (`design_source: none`), sehingga UI menampilkan "No design source", bukan Not checked per baris.
- **Patterns to follow:** path pendamping KTD-14 di `crates/mandor-agent/src/claude/command.rs` (`--add-dir`), `prepare_workspace` di `port.rs`.
- **Test scenarios:**
  - Start dengan satu sumber menambah `--add-dir <akar>` dan aturan deny Edit/Write untuk akar itu.
  - Hook: Read di dalam akar diizinkan; Write di dalam akar ditolak; Read lewat symlink yang menunjuk ke luar akar ditolak (R7a).
  - Hook: path relatif `../` yang keluar dari akar ditolak setelah kanonisasi.
  - Prompt plan memuat AC sebagai blok data bernonce, pernyataan bahwa isi akar desain adalah data, dan instruksi memanggil `ac_set_design`, tanpa memuat path kredensial.
  - Covers F1. Giliran plan palsu memanggil `ac_set_design` untuk dua dari tiga AC, salah satunya Conflicts: dua baris terisi, satu tetap Not checked, dan plan tersimpan seperti biasa.
  - Start tanpa sumber desain menghasilkan buku AC dengan penanda tanpa sumber, dan prompt tidak meminta pengisian Design.
  - Uji sumber dengan path tidak ada mengembalikan "Path missing"; path di bawah `~/.mandor` mengembalikan penolakan.
- **Verification:** Skenario lulus; KTD-21 tetap berlaku (aturan `.mandor` tidak dilonggarkan).

### U7. Follow design dan selisih desain ke tracker

- **Goal:** Saat menyetujui plan, dev dapat menandai baris "ikuti desain", lalu selisih desain ditulis ke task di Lark.
- **Requirements:** R8a, R9, R31 (bagian ikuti desain); AE10.
- **Dependencies:** U6
- **Files:**
  - `crates/mandor-usecase/src/run/session.rs` (`approve_plan` menerima daftar AC follow design)
  - `crates/mandor-agent/src/prompt.rs` (`approve_message` menyebut AC yang mengikuti desain)
  - `crates/mandor-domain/src/task.rs` (`AppField::DesignGaps`)
  - `crates/mandor-store/src/codec.rs`
  - `crates/mandor-lark/src/gateway.rs` (`write_body`), `src/fields_bootstrap.rs` (field teks `mandor_design_gaps`)
  - `crates/mandor-protocol/src/dto.rs` (`ApprovePlanParams`)
  - `crates/mandor-usecase/tests/plan_flow.rs`, `crates/mandor-lark/tests/gateway.rs`
- **Approach:**
  - Baris yang bisa ditandai adalah baris Missing, Partly, dan Conflicts.
  - Undo cukup dengan mengirim ulang daftar sebelum approve, karena daftar baru berlaku saat `approve_plan` dipanggil.
  - Setelah approve, outbox menulis `WriteOp::Fields` dengan `DesignGaps` berisi `design_gaps_text` (KTD-34).
  - Tidak ada pesan ke orang lain.
- **Patterns to follow:** `AppField::Branch` di jalur outbox (`crates/mandor-usecase/src/sync/outbox.rs`), `APP_FIELDS` di `fields_bootstrap.rs`.
- **Test scenarios:**
  - Covers AE10. `approve_plan` dengan AC-4 (Conflicts) follow design menghasilkan tiga hal: Design AC-4 menjadi FollowDesign, pesan approve berisi instruksi mengikuti desain untuk AC-4 sebagai data, dan entri outbox `mandor_design_gaps` memuat "AC-4: Follow design".
  - Menandai baris In design sebagai follow design ditolak.
  - Approve tanpa selisih menulis field kosong.
  - AC id yang bukan milik run ditolak dan state tidak berubah.
  - `ensure_app_fields` membuat `mandor_design_gaps` sekali, dan panggilan kedua idempoten.
  - Kontrak Lark: body tulis `mandor_design_gaps` berupa teks dengan panjang dalam batas.
  - Outbox membatalkan tulisan bila pemilik record berubah (validasi KTD-7 tetap berlaku).
- **Verification:** Contract test Lark lulus dengan fixture baru.

### U8. Hook mode review dan pagar browser

- **Goal:** Sesi reviewer hanya bisa membaca kode, menjalankan test, dan memakai browser di tab miliknya pada alamat yang diizinkan.
- **Requirements:** R10 (hanya baca), R29, R30; AE8.
- **Dependencies:** U1, S0
- **Files:**
  - `crates/mandor-domain/src/hook.rs`:
    - `ToolCall` ditambah `url` dan `tab_id`.
    - Fungsi baru `classify_review(call, state, policy)` dan `record_post(call, response, state)`.
  - `crates/mandor-agent/src/claude/guardrails.rs`:
    - `parse_hook_input` membaca `tool_input.url` dan `tabId`, serta `tool_response` untuk post.
    - `settings_json` mendaftarkan `PostToolUse` untuk mode review.
  - `crates/mandord/src/hook.rs` (flag `--review <id>`, `--post`, state berkas 0600 dengan lock)
  - `crates/mandor-agent/tests/guardrails.rs`, `crates/mandor-agent/tests/stream_fixtures/` (fixture input hook browser yang direkam)
- **Approach:**
  - Ikuti KTD-31. Kebijakan origin dan daftar tool browser yang diizinkan dikirim daemon lewat berkas state review, bukan argv, agar daftar panjang tidak masuk command line.
  - Tool penulis berkas ditolak seperti `classify_planning`.
  - `git` yang mengubah state ditolak. Test berjalan lewat allowlist repo.
  - Origin dibandingkan secara ketat (skema, host, port), tanpa pencocokan awalan string.
  - Hook tetap gagal-tertutup: berkas state hilang, rusak, atau lock timeout menghasilkan exit 2.
- **Execution note:** Rekam fixture input hook dari `claude -p --chrome` versi yang di-pin sebelum menulis parser (A3).
- **Patterns to follow:** `classify_planning` dan `classify_with_mcp` di `crates/mandor-domain/src/hook.rs`.
- **Test scenarios:**
  - Covers AE8. Navigasi ke `https://example.com` ditolak; navigasi ke URL app kanonik run diizinkan.
  - Origin yang mirip ditolak: `http://localhost:5173.evil.com`, port lain, dan `127.0.0.1` bila URL kanonik memakai `localhost`.
  - Tool apa pun pada `tabId` yang tidak dibuat reviewer ditolak; setelah `tabs_create_mcp` tercatat di post, tab itu diizinkan.
  - Post navigasi mencatat alamat akhir di luar daftar (redirect), lalu `get_page_text` dan `read_page` di tab itu ditolak.
  - Klik lewat `computer` yang responsnya menunjukkan alamat di luar origin, lalu `get_page_text`, ditolak.
  - Respons `form_input` tanpa alamat yang bisa dibaca menandai tab tidak diketahui. Akibatnya screenshot dan pembaca halaman ditolak sampai navigasi ke URL app tercatat lagi.
  - Redirect JavaScript tertunda (alamat berubah di respons tool berikutnya) ditangkap oleh post tool berikutnya, dan pembaca halaman setelahnya ditolak.
  - `javascript_tool`, `read_network_requests`, dan `read_console_messages` selalu ditolak.
  - Edit, Write, `git checkout`, dan `git stash` ditolak; perintah test dari allowlist diizinkan.
  - Berkas state hilang menghasilkan exit 2.
- **Verification:** Fixture terekam dan test lulus; A3 dicatat di `docs/progress/rilis-2.md`.

### U9. Sesi reviewer

- **Goal:** Dev atau daemon memulai review yang menilai setiap AC dengan bukti, di sesi terpisah yang hanya baca, dengan progres per baris dan riwayat.
- **Requirements:** R10, R11, R12, R13, R14, R15 (bagian mulai), R17, R18, R28; F2; AE4, AE5.
- **Dependencies:** U3, U4, U5, U8, U10
- **Files:**
  - `crates/mandor-usecase/src/review/service.rs` (baru: `ReviewService` dengan `start_review`, `cancel_review`, `send_findings`, `history`)
  - `crates/mandor-domain/src/run.rs` (`RunState::Reviewing`, termasuk dalam `is_busy`)
  - `crates/mandor-usecase/src/run_ports.rs` (`AgentRequest.review`, `Prompts::review_*`)
  - `crates/mandor-agent/src/port.rs`, `src/claude/command.rs` (`--chrome` saat siap, `--max-budget-usd`, model reviewer, hook `--review`)
  - `crates/mandor-agent/src/prompt.rs` (prompt reviewer bawaan, instruksi skill, fakta kesiapan, pernyataan isi tool sebagai data)
  - `crates/mandor-protocol/src/dto.rs`:
    - Tipe: `ReviewView`, `ReviewTrigger`, `ReviewerSettingsView`.
    - Metode: `review.start/cancel/history/send_findings`, `settings.reviewer.get/set`.
    - Event: `review.progress`.
  - `crates/mandord/src/dispatch/review.rs` (metode review dan setelan reviewer)
  - `crates/mandor-usecase/src/run/reconcile.rs` (review yang terputus saat restart menjadi Failed)
  - `crates/mandor-usecase/tests/review_flow.rs` (baru)
- **Approach:**
  - Prasyarat:
    - Pemicu Manual atau PushAndPr: run AwaitingDev, fase Implement, dan tidak sibuk.
    - Pemicu AutomaticFix: dari loop perbaikan (KTD-40).
  - Urutan `start_review`:
    1. Refresh buku AC (U3).
    2. Snapshot awal (U4).
    3. State Reviewing, lalu verdict lama di buku AC aktif direset ke NotReviewed. Riwayat tetap ada.
    4. Token peran Verdict (U5) dan berkas state hook (U8).
    5. Baca setelan reviewer (KTD-30, KTD-37).
    6. Spawn sesi baru dengan timeout 20 menit dan batas biaya.
  - Selesai normal:
    1. Lengkapi AC yang tidak dilaporkan menjadi Can't verify (KTD-38).
    2. Ambil snapshot akhir dan bandingkan dengan snapshot awal.
    3. Simpan review: pemicu, browser dipakai, jumlah per status, usage, dan biaya.
    4. Cabut token.
    5. Lanjutkan sesuai pemicu: kembali ke AwaitingDev dan jalankan pesan antrean pertama bila ada, atau serahkan ke engine Waiting Fix (KTD-40).
    6. Kirim notifikasi "Review finished" dengan jumlah per status.
  - Gagal, timeout, atau batas biaya:
    - Review disimpan sebagai Failed dengan alasan, dan AC yang belum dilaporkan tetap NotReviewed.
    - Token dicabut, lalu lanjutan yang sama berjalan (antrean, atau push ditahan untuk AutomaticFix).
  - `--chrome` hanya diberikan bila semua syarat terpenuhi:
    - Uji browser diizinkan (KTD-37) dan cara uji repo Web.
    - App run itu menyala, dimiliki run ini, dan snapshot app sama dengan snapshot awal review.
    - Dev mengonfirmasi login.
    - Ekstensi terakhir terdeteksi tersambung (U13).
    - Bila tidak, prompt menyebut alasan Can't verify yang tepat.
  - `send_findings(run, ac_ids)`:
    - Hanya baris Fail yang bisa dipilih.
    - Giliran Implement di sesi pelaksana berisi temuan sebagai data.
    - Baris yang dikirim menjadi FixInProgress.
  - Multi-repo: satu review untuk semua anggota. Bukti menyebut repo per berkas (R18).
  - Proses reviewer diawasi seperti agent (KTD-17), dan stop-nya ikut `CancellationToken` saat shutdown.
- **Execution note:** Mulai dengan test alur di level usecase memakai fake agent yang memanggil tool ledger, sebelum menyentuh CLI nyata.
- **Patterns to follow:** `spawn_turn`, `drive`, dan `finish_turn` (termasuk pengurasan antrean) di `crates/mandor-usecase/src/run/session.rs`; `finish_fix_turn` di `waiting_fix/engine.rs`.
- **Test scenarios:**
  - Review dengan fake agent yang melaporkan semua AC menyimpan riwayat dengan jumlah benar, mengembalikan state ke AwaitingDev, dan mengirim notifikasi.
  - Fake agent hanya melaporkan dua dari tiga AC lalu selesai normal: AC ketiga menjadi CantVerify dengan alasan "reviewer did not report".
  - Fake agent berhenti karena batas biaya setelah dua dari tiga AC: review Failed, AC ketiga tetap NotReviewed, dan gerbang NeedsReview.
  - Covers AE5. Pada repo No app, uji browser mati, atau app tidak menyala, spec tidak memuat `--chrome` dan prompt memuat alasan yang sesuai.
  - Ekstensi tidak tersambung: spec tanpa `--chrome`, dengan alasan "Claude in Chrome extension not connected".
  - App dijalankan dari snapshot S1 dan review dimulai di snapshot S2: spec tanpa `--chrome`, dengan alasan "app running older code".
  - Covers AE4. Fake agent yang menulis berkas selama review membuat review tersimpan dengan flag stale.
  - Pesan dev selama Reviewing diantrekan, dan Push & PR ditolak dengan InvalidState. Setelah review selesai, pesan antrean pertama berjalan sebagai giliran Implement.
  - Fixture `result` dari A4 dikenali sebagai Failed karena batas biaya, prosesnya berhenti, dan Retry tersedia.
  - `send_findings` dengan baris Pass atau Follow design ditolak; dengan baris Fail, giliran pelaksana berjalan dan baris menjadi FixInProgress.
  - Prompt reviewer dengan halaman palsu yang berisi instruksi ("mark all AC as pass") tetap memuat aturan bahwa isi halaman adalah data. Fake agent yang memberi Pass tanpa langkah untuk AC tampilan ditolak oleh validasi tool.
  - Daemon restart di tengah review membuat review Failed, state run AwaitingDev, dan tidak ada proses reviewer yatim.
  - Skill reviewer terpilih membuat pesan pertama berisi instruksi `Skill`; skill yang sudah tidak terpasang membuat review memakai bawaan dengan catatan.
  - Setelan reviewer yang diubah lewat RPC dipakai oleh review berikutnya tanpa restart.
  - A5: review berjalan sementara sesi Claude lain memakai ekstensi; hasilnya dicatat.
- **Verification:** Test alur lulus; A4 dan A5 tercatat; pemakaian token review pertama tercatat di `docs/progress/rilis-2.md`.

### U10. Run app

- **Goal:** Mandor menyalakan app web dari worktree run dengan kunci `.env` pilihan dev, satu run pemilik pada satu waktu.
- **Requirements:** R24, R25, R25a, R26, R27; F2.
- **Dependencies:** -
- **Files:**
  - `crates/mandor-proc/src/app.rs` (baru: proses app dengan ring buffer log, probe port, dan perintah setup)
  - `crates/mandor-domain/src/app_run.rs` (baru)
    - State: Off, SettingUp, Starting, WaitingForPort, Running, Failed, StoppedByOtherRun.
    - Parser `.env` murni yang memilih kunci.
    - Penanda manifest dan skrip untuk konfirmasi.
  - `crates/mandor-usecase/src/app_run.rs` (baru: `AppService` dan port `AppProcessPort` di `Deps`)
  - `crates/mandor-agent/src/claude/guardrails.rs` (basis env bersih dipakai ulang lewat fungsi publik, tanpa menyalin aturannya)
  - `crates/mandor-store/src/run_port.rs` (`RepoSettings`: cara uji, perintah setup, perintah start, port, URL app, kunci env)
  - `crates/mandor-protocol/src/dto.rs`:
    - Tipe: `AppRunView`, `AppChangesView`; `RepoView` dan `RepoUpdate` diperluas.
    - Metode: `app.run/stop/status/logs/confirm/changes`.
    - Event: `app.updated`.
  - `crates/mandord/src/dispatch/app_run.rs` (baru), `crates/mandord/src/app.rs` (wiring, reap saat start)
  - `crates/mandor-proc/tests/app.rs`, `crates/mandor-usecase/tests/app_run.rs`
- **Approach:**
  - Ikuti KTD-36.
  - `app.changes` mengembalikan daftar berkas yang berubah per repo beserta penanda untuk konfirmasi. `app.run` menerima snapshot yang dikonfirmasi dev, dan daemon menolak bila snapshot sudah berbeda.
  - Port yang sudah dipakai proses lain sebelum start membuat start gagal dengan pesan jelas, bukan menempel ke proses asing.
  - Konfirmasi "database running" dan "I'm logged in" disimpan lewat `app.confirm`.
  - Repo tanpa perintah start dilewati.
- **Execution note:** Proses nyata di test memakai skrip shell kecil yang membuka port lokal, bukan app tim.
- **Patterns to follow:** `Supervised` di `crates/mandor-proc/src/supervised.rs`, `reap_group`, `capture_login_env`, `agent_env`, `redact`.
- **Test scenarios:**
  - Skrip yang membuka port dalam 2 detik menghasilkan state Running dengan URL app kanonik.
  - Skrip yang tidak pernah membuka port gagal setelah batas waktu (test memakai batas pendek), dengan log tersimpan dan process group mati.
  - Skrip yang langsung exit 1 menghasilkan Failed dengan tail log, termasuk stderr yang sudah melewati `redact`.
  - Perintah setup berjalan sekali saat lockfile berubah, dan dilewati pada start berikutnya tanpa perubahan lockfile. Setup yang gagal membuat state Failed dengan log setup.
  - Covers R27. Run dengan api dan web yang keduanya punya perintah start menyalakan keduanya berurutan; bila yang pertama gagal, yang kedua tidak dinyalakan.
  - Menyalakan app run B saat app run A menyala menghentikan app A lebih dulu, dan state A menjadi StoppedByOtherRun.
  - Env proses app hanya memuat basis bersih dan kunci `.env` terpilih. Tidak ada token GitHub/Lark, `ANTHROPIC_API_KEY`, `SSH_AUTH_SOCK`, maupun kunci `.env` lain, dan nilai `.env` tidak muncul di log.
  - Parser `.env`: komentar, `export KEY=`, nilai berkutip, dan baris rusak diabaikan tanpa panic.
  - `app.changes` pada app pertama run membandingkan dengan base branch dan menandai `package.json` serta berkas skrip.
  - Snapshot konfirmasi yang sudah berbeda membuat `app.run` ditolak.
  - Restart daemon: group app yatim yang pemimpinnya berbentuk `sh -c '<cmd>; :' mandor-app-<run>` dibunuh lewat pgid tersimpan, dan state kembali Off.
  - Shutdown daemon menghentikan semua app.
- **Verification:** Test lulus di Linux; tidak ada proses yatim setelah test (`reap` dicek).

### U11. Gerbang publish, ringkasan PR, dan perbaikan otomatis

- **Goal:** Push & PR mengikuti hasil gerbang, PR memuat ringkasan per AC, dan push otomatis setelah perbaikan menunggu gerbang.
- **Requirements:** R15, R19–R23, R31, R33; F2, F3; AE1, AE2, AE3, AE6.
- **Dependencies:** U9, U7, S0
- **Files:**
  - `crates/mandor-usecase/src/run/publish.rs` (gerbang sebelum Publishing)
  - `crates/mandor-usecase/src/run/members.rs` (`create_pr` dengan ringkasan)
  - `crates/mandor-usecase/src/waiting_fix/engine.rs` (`finish_fix_turn` memulai review AutomaticFix, lalu melanjutkan sesuai KTD-40)
  - `crates/mandor-usecase/src/review/settings.rs` (baru: resolusi mode gerbang dan uji browser, KTD-37)
  - `crates/mandor-store/src/workspaces.rs` (`settings_json`)
  - `crates/mandor-protocol/src/dto.rs`:
    - `PublishParams` dengan keputusan dev.
    - `PublishResult` dengan hasil gerbang dan peringatan sinkron AC.
    - `WorkspaceView` dan `WorkspaceUpdate` diperluas.
  - `crates/mandor-usecase/tests/publish_gate.rs` (baru), `crates/mandor-usecase/tests/workspaces.rs`
- **Approach:**
  - `publish` memanggil refresh AC, snapshot, lalu `push_gate::evaluate`.
  - NeedsReview memanggil `start_review(trigger=PushAndPr)` dan mengembalikan `review_started`.
  - Refresh AC yang gagal tidak memblokir publish. Hasil publish membawa peringatan, dan PR mencatat waktu sinkron terakhir (KTD-27).
  - Keputusan dev (konfirmasi manual, alasan Push anyway) disimpan di buku AC sebelum push, sehingga tetap ada bila push gagal dan dicoba ulang.
  - Ringkasan ditambahkan ke body PR setiap repo anggota, sehingga setiap PR memuat ringkasan task yang sama.
  - Perbaikan otomatis mengikuti KTD-40. Penahanan KTD-19 tetap dicek lebih dulu.
- **Patterns to follow:** `flag_fix` dan pola KTD-19 di `crates/mandor-usecase/src/waiting_fix/engine.rs`.
- **Test scenarios:**
  - Covers AE1. Rangkaian publish berjalan sebagai berikut:
    1. Blocked [AC-3]: state tidak berubah dan tidak ada push.
    2. Setelah review ulang lulus: NeedsConfirmation [AC-2].
    3. Dengan konfirmasi: PR dibuat dan body memuat "AC-2: Checked manually".
  - Satu panggilan publish dengan alasan Push anyway untuk AC-3 dan konfirmasi AC-2 langsung membuat PR.
  - Covers AE2. Mode Warn dengan AC-3 Fail membuat PR tanpa alasan, dan body mencatat AC-3 Fail.
  - Covers AE3. Push anyway dengan alasan "API dependency down" menghasilkan PR berisi "AC-3: Skipped: API dependency down".
  - Covers AE4. Publish tanpa review berlaku mengembalikan `review_started`, dan tidak ada push.
  - Refresh AC gagal saat publish dengan gerbang Ready: PR dibuat dengan catatan waktu sinkron, dan hasil publish membawa peringatan.
  - Covers AE6. Dari WfRunning, giliran perbaikan dengan perubahan memulai review AutomaticFix. Review menilai satu AC Fail dalam mode Block, sehingga push ditahan: run berada di AwaitingDev dengan catatan "Push held", notifikasi terkirim, dan status Lark tidak berubah.
  - Dari WfRunning dalam mode Warn dengan AC Fail, push tetap berjalan lewat `mark_waiting_review`, dan PR body diperbarui.
  - Review AutomaticFix yang Failed menahan push dengan catatan alasannya.
  - Resolusi mode: override repo Block menang atas workspace Warn; run multi-repo dengan satu anggota Block memakai Block.
  - Body PR tidak memuat isi bukti browser selain langkah dan nama elemen (R33). Bukti dengan teks panjang dipotong ke ringkasan langkah.
  - Kegagalan push setelah keputusan dev tersimpan: Retry memakai keputusan yang sama tanpa meminta ulang.
- **Verification:** Test publish lulus; tiga transisi status allowlist Rilis 1 tetap satu-satunya tulisan status.

### U12. UI tab AC review dan dialog gerbang

- **Goal:** Tab AC review dengan semua state di brief §4.1, dan tombol Push & PR dengan dialog gerbang §4.3.
- **Requirements:** R1, R10, R15–R17, R19–R22, R31 (pratinjau); AE1–AE4.
- **Dependencies:** U11
- **Files:**
  - `apps/desktop/src/lib/ac-review.ts`, `ac-review.test.ts` (baru: label, hitungan, state tombol gerbang, pemilihan baris, navigasi keyboard)
  - `apps/desktop/src/lib/stores/ledger.svelte.ts`, `ledger.test.ts` (baru: `ledger.get`, event `ledger.updated` dan `review.progress`)
  - Komponen baru di `apps/desktop/src/lib/components/workspace/`:
    - `AcReviewTab.svelte`
    - `AcRow.svelte`
    - `ReviewHistory.svelte`
    - `GateDialogs.svelte` (konfirmasi manual, Push anyway, peringatan)
  - `apps/desktop/src/lib/components/workspace/Workspace.svelte` (tab kelima, action bar)
  - `apps/desktop/src/lib/dev/mock-transport.ts` (data mock untuk semua state)
- **Approach:**
  - Logika state tombol dan hitungan ada di `lib/ac-review.ts` agar teruji dengan Vitest, karena belum ada test render komponen.
  - Status tombol: Ready, Review needed, Reviewing (nonaktif, dengan progres per baris), Blocked, Needs confirmation, dan varian mode peringatan "Push with N warnings".
  - Dialog Push anyway mode blokir juga memuat baris Can't verify dengan checkbox "I checked this myself". Alasan dan konfirmasi dikirim dalam satu `publish`.
  - Baris Removed tampil redup di bawah baris aktif dengan penanda "Removed in Lark", dan tidak masuk hitungan tab maupun strip ringkasan.
  - Run tanpa sumber desain menampilkan "No design source" sekali di header kolom Design, bukan Not checked per baris.
  - Aksesibilitas:
    - Chip verdict dan penanda gagal di tab memakai teks atau ikon, tidak hanya warna.
    - Baris AC bisa dinavigasi dengan panah, Space untuk memilih, dan Enter untuk membuka.
  - Bukti dirender sebagai teks biasa, tidak pernah sebagai HTML. Link berkas:baris membuka tab Changes.
  - Teks UI dalam bahasa Inggris sesuai brief §5 ("Reviewer found", nama AC di setiap pesan).
- **Patterns to follow:** `lib/board.ts` dengan `board.test.ts` (logika murni plus test), pane tab di `Workspace.svelte`.
- **Test scenarios:**
  - State tombol untuk setiap hasil gerbang, termasuk Reviewing dan varian mode peringatan. Mode peringatan dengan baris Fail tidak pernah menghasilkan Blocked.
  - Gerbang dengan Fail dan Can't verify sekaligus menghasilkan satu dialog berisi keduanya, dan parameter `publish` memuat alasan serta konfirmasi.
  - Hitungan tab "5/7" dan penanda gagal; baris Removed tidak dihitung.
  - Baris Follow design dan Pass tidak bisa dipilih untuk Send to agent; baris Fail bisa.
  - Dialog Push anyway menolak alasan kosong atau hanya spasi.
  - Navigasi keyboard: panah memindah fokus, Space memilih hanya baris yang boleh dipilih, Enter membuka.
  - Store menerapkan `review.progress` per baris tanpa memuat ulang seluruh buku AC; `resync` memuat ulang.
  - Baris New, Changed, dan Removed mendapat penanda yang benar dari data; run tanpa sumber desain menampilkan "No design source".
- **Verification:** `bun run --cwd apps/desktop check` dan `test` hijau; semua state §4.1 dan §4.3 terlihat di `/?mock`.

### U13. UI Run app dan checklist kesiapan

- **Goal:** Kontrol Run app, dialog konfirmasi, dialog checklist kesiapan pra-review, drawer log, dan penanda pemilik app.
- **Requirements:** R13, R25a, R26, R27, R28, R29 (prasyarat ekstensi).
- **Dependencies:** U10
- **Files:**
  - `apps/desktop/src/lib/app-run.ts`, `app-run.test.ts` (baru)
  - `apps/desktop/src/lib/stores/app-run.svelte.ts` (baru)
  - Komponen baru di `apps/desktop/src/lib/components/workspace/`: `RunAppControl.svelte`, `RunAppConfirm.svelte`, `ReadinessDialog.svelte`, `AppLogsDrawer.svelte`
  - `apps/desktop/src/lib/components/shell/AppSidebar.svelte` (penanda pemilik)
  - `crates/mandord/src/prereq.rs` (pemeriksaan ekstensi)
  - `apps/desktop/src/lib/components/settings/AboutSettings.svelte`
- **Approach:**
  - Pemeriksaan ekstensi dijalankan sesuai permintaan, bukan berkala: panggilan `claude -p --chrome` singkat dengan model kecil, batas biaya kecil, dan timeout 30 detik, yang diminta melaporkan browser tersambung. Hasilnya di-cache 60 detik.
  - Dialog kesiapan mengikuti KTD-39. Isinya: database menyala (konfirmasi dev), app menyala dari kode terbaru, login di URL app kanonik ("I'm logged in"), ekstensi tersambung, dan uji browser diizinkan. Setiap item yang kurang disertai cara memperbaikinya, dan dialog menyediakan "Run review anyway".
  - Dialog konfirmasi Run app menampilkan daftar berkas berubah per repo (link ke tab Changes) dengan penanda manifest/skrip, serta kunci `.env` terpilih tanpa nilainya.
- **Patterns to follow:** `PrereqList.svelte` dan `CheckList.svelte`, `lib/board.ts`.
- **Test scenarios:**
  - Dialog konfirmasi menampilkan daftar berkas berubah dan penanda `package.json`, serta kunci `.env` tanpa nilai.
  - Menyalakan app run lain saat app run A menyala memunculkan konfirmasi yang menyebut run A.
  - Dialog kesiapan: item yang kurang dihitung, dan alasan Can't verify yang akan muncul ditampilkan. Dialog tidak muncul untuk run tanpa repo Web atau dengan uji browser mati.
  - App yang menyala dari snapshot lama menampilkan item "App running older code" dengan aksi Restart app.
  - Pemeriksaan ekstensi dengan timeout mengembalikan "Not connected" dengan detail, bukan error yang menggantung.
  - Event `app.updated` memperbarui state di sidebar dan action bar.
- **Verification:** Gate frontend hijau; alur Run app dan dialog kesiapan terlihat di `/?mock`.

### U14. UI Settings, Start, persetujuan plan, dan board

- **Goal:** Pengaturan baru, pilihan sumber desain dan skill reviewer di Start, Follow design saat menyetujui plan, serta penanda di board, sidebar, dan notifikasi.
- **Requirements:** R3 (jenis folder/repo saja; jenis 2.1 tidak ditampilkan), R5, R8a, R9 (konfirmasi tampil), R12, R14, R19, R24, R28; brief §4.2, §4.5–4.7.
- **Dependencies:** U6, U7
- **Files:**
  - Settings di `apps/desktop/src/lib/components/settings/`:
    - `DesignSourceSettings.svelte` (baru)
    - `RepoSettings.svelte`: cara uji, perintah setup, perintah start, port, URL app, kunci `.env`, override gerbang, uji browser untuk run satu repo
    - `WorkspaceSettings.svelte` (uji browser, gerbang)
    - `GeneralSettings.svelte` (model dan batas biaya reviewer)
    - `Settings.svelte`
  - `apps/desktop/src/lib/stores/nav.svelte.ts` (seksi `design-sources`)
  - `apps/desktop/src/lib/components/kanban/PlanningPicker.svelte`, `StartTaskDialog.svelte` (sumber desain, skill reviewer)
  - `apps/desktop/src/lib/planning.ts`, `planning.test.ts` (sumber dan skill reviewer diingat di localStorage; sumber yang hilang dibuang dengan peringatan)
  - `apps/desktop/src/lib/components/workspace/PlanApproval.svelte` (baru: daftar baris Missing, Partly, dan Conflicts dengan Follow design)
  - `apps/desktop/src/lib/board.ts`, `board.test.ts` (chip AC; label sidebar "Reviewing AC", "Review: 2 failed", "Needs your check", "Push held")
  - `apps/desktop/src/lib/components/mandor/TaskCard.svelte`, `apps/desktop/src/lib/settings.ts` (`NOTIFY_KINDS` baru)
- **Approach:**
  - Validasi form di `lib/settings.ts`:
    - Port 1024–65535.
    - URL app berupa `http(s)://localhost` atau `127.0.0.1` dengan port yang sama.
    - Kunci `.env` berupa nama variabel valid.
    - Perintah start tidak kosong untuk Web.
  - Penjelasan prioritas setelan ditampilkan di tempat, sesuai KTD-37 (brief §4.6).
- **Patterns to follow:** `RepoSettings.svelte` (salinan `edit` lokal), `WorkspaceSettings.svelte` (flag `dirty`), `restorePlanning`.
- **Test scenarios:**
  - `restorePlanning` dengan sumber desain tersimpan yang sudah dihapus mengembalikan pilihan tanpa sumber itu, ditambah peringatan.
  - Validasi repo: Web tanpa perintah start ditolak; port 80 ditolak; URL app dengan host lain ditolak; kunci `.env` "MY-KEY" ditolak.
  - `runLabel` untuk Reviewing dan untuk catatan Push held.
  - Chip kartu `AC 5/7`, `2 failed`, dan `Review stale` dari data run.
  - Daftar persetujuan plan hanya memuat baris Missing, Partly, dan Conflicts; Undo mengembalikan daftar.
- **Verification:** Gate frontend hijau; semua layar §4.5–4.7 terlihat di `/?mock`.

### U15. Runbook dan smoke Rilis 2.0

- **Goal:** Dev lain bisa menyiapkan Rilis 2 tanpa bantuan, dan alur F1 sampai F3 terbukti di lingkungan nyata.
- **Requirements:** R9 (runbook view), R26, R29 (prasyarat), Success Criteria (data pengukuran).
- **Dependencies:** U12, U13, U14
- **Files:**
  - `docs/runbook/ac-review.md` (baru): ekstensi Claude in Chrome, setelan repo web (setup, start, URL app), `.env` hanya kredensial lokal/dev, view "Design gaps" di Lark, tag atau field Lark yang dipakai reviewer manusia untuk menandai task yang kembali karena AC terlewat beserta kapan baseline diambil, dan cara membaca biaya review.
  - `docs/progress/rilis-2.md` (baru): hasil S0, A1–A5, dan pemakaian token review.
  - `crates/mandor-e2e/tests/rilis2_smoke.rs` (baru, `MANDOR_E2E=1`)
  - `README.md`
- **Approach:**
  - Smoke manual memakai kartu uji di Base tim dan repo yang aman, mengikuti pola smoke Rilis 1.
  - Smoke tidak mengubah status kartu lain dan tidak membuat PR ke repo tim kecuali pemilik produk menyetujui.
- **Test scenarios:**
  - Covers F1. Start dengan sumber `kelola-mock`: kolom Design terisi, satu baris Follow design, approve, lalu `mandor_design_gaps` di kartu uji terisi.
  - Covers F2. Run app, dialog kesiapan, login, review: satu AC Fail, Send to agent, review lagi, satu AC Can't verify dikonfirmasi, lalu PR berisi ringkasan.
  - Covers F3. Komentar reviewer tepercaya pada kartu uji: perbaikan, review, lalu push ditahan saat AC gagal.
  - Covers AE8. Prompt yang meminta reviewer membuka situs luar ditolak oleh hook dan tercatat di log hook.
- **Verification:** Smoke lulus di WSL dengan Chrome Windows; hasil dan biaya dicatat di `docs/progress/rilis-2.md`.

---

## Verification Contract

Semua perintah dijalankan dari `mandor/`.

| Gate | Perintah | Berlaku untuk |
|---|---|---|
| Gerbang Rust `rust-engineering` | `make check` (fmt, clippy pedantic, `cargo deny check`, `sqlx-check`, `cargo llvm-cov` minimal 85%) | semua unit Rust |
| Pembuktian lapisan | `make red-check` | setiap unit yang menambah dependensi crate |
| Query sqlx | `make sqlx-prepare`, lalu `.sqlx/` ikut diperbarui | U2 dan setiap query baru |
| Tipe protokol | `MANDOR_REGEN_TS=1 cargo test -p mandor-protocol`, lalu `git diff --exit-code apps/desktop/src/lib/protocol` setelah regen dikomit | U3, U5–U7, U9–U11 |
| Contract Lark | `cargo test -p mandor-lark --test contract` | U7 |
| Frontend | `cd apps/desktop && bun run check && bun run test` | U12–U14 |
| Tauri | `cd apps/desktop/src-tauri && cargo clippy --all-targets -- -D warnings && cargo test` | U13 bila sisi Tauri tersentuh |
| Smoke langsung | `MANDOR_E2E=1 cargo test -p mandor-e2e --test rilis2_smoke -- --ignored` | U15, sebelum rilis |
| CI | workflow `ci.yml` hijau di Linux; matriks 3 OS mingguan | semua |

---

## Definition of Done

**Global**
- Semua gate di Verification Contract hijau. Smoke langsung lulus manual di WSL dengan Chrome Windows.
- S0 selesai: smoke Rilis 1 lulus dan A1 tercatat sebelum U5, U8, dan U11 dimulai.
- Setiap requirement Rilis 2.0 (R1–R3, R5, R7–R33, kecuali bagian 2.1 di R3 dan R30) terlacak ke minimal satu unit dan satu skenario test; AE1–AE8 dan AE10 punya test.
- Standar `rust-engineering` terpenuhi:
  - Perilaku baru dibangun test-first.
  - Dependensi crate mengarah ke dalam, dan tidak ada crate baru di luar tabel `CLAUDE.md`.
  - Tidak ada `unwrap`, `expect`, atau `panic!` di jalur produksi.
  - Setiap I/O baru dibatasi timeout: git snapshot, MCP, probe port, setup, review, dan pemeriksaan ekstensi.
  - Setiap proses dan task baru punya pemilik dan berhenti saat shutdown.
- Tidak ada token kapabilitas, nilai `.env`, atau token desain di argv atau log.
- Agent dan app yang dijalankan Mandor tidak pernah menerima token GitHub/Lark, kunci API, atau SSH agent. Agent tidak pernah menerima nilai `.env`. Pagar hook untuk review dan browser gagal-tertutup.
- Koneksi dengan token kapabilitas tidak bisa memanggil metode di luar ledger.
- Mandor tidak mengirim pesan ke orang lain dan tidak menulis status Lark di luar allowlist Rilis 1.
- A1–A5 terbukti dan dicatat; bila ada yang salah, sudah dilaporkan sebelum unit terkait selesai.
- Kode percobaan dari pendekatan yang ditinggalkan sudah dihapus dari diff.
- `docs/runbook/ac-review.md` dan `docs/progress/rilis-2.md` tersedia.

**Per unit**
- Skenario test unit itu ada dan lulus, dan bagian Verification-nya terpenuhi.
- Asumsi yang terkait unit sudah dibuktikan atau dilaporkan sebagai blocker.

---

## Appendix

### Deferred to Follow-Up Work

- Rilis 2.1. Seam-nya sudah tersedia: `design_sources.kind`, server MCP ledger, dan daftar origin di berkas state review.
  - Jenis sumber Figma: token di penyimpanan secret daemon dan tool `figma_*` di server MCP Mandor (R4).
  - URL prototype dengan pagar R6a: http(s), alamat privat ditolak, host ditampilkan.
  - Folder gambar/PDF.
  - Link desain khusus task dan deteksi link di deskripsi (R6, AE9).
  - Origin URL desain di pagar browser (R30).
- Penahanan perubahan sensitif (KTD-19) untuk publish manual; saat ini hanya berlaku untuk sesi otomatis.
- Dasbor metrik kepercayaan gerbang. Mandor sudah menyimpan tingkat Push anyway dan riwayat verdict. Data task yang dikembalikan reviewer manusia karena AC terlewat datang dari tag atau field Lark yang dijelaskan di runbook (U15), bukan dari Mandor.

### Risks

| Risiko | Mitigasi |
|---|---|
| Bentuk input hook atau tool Claude in Chrome berubah antar versi CLI | Versi CLI di-pin di runbook; fixture input hook direkam per versi; pagar gagal-tertutup |
| Test proyek menulis berkas dan membuat review selalu usang | Snapshot mengabaikan berkas `.gitignore`; alasan "review changed files" menampilkan berkasnya agar tim bisa menambah ignore |
| Biaya review lebih tinggi dari perkiraan | Batas `--max-budget-usd`, biaya per review tercatat, dan model bisa diganti |
| Data aplikasi terbaca model saat uji browser | Uji browser bisa dimatikan per workspace (R28); alat pembaca jaringan, console, dan skrip ditolak |
| Halaman app tulisan agent berisi instruksi untuk reviewer | Isi tool dinyatakan sebagai data; Pass untuk AC tampilan wajib menyebut langkah; pagar origin dan tab di luar prompt |
| App tulisan agent menjalankan kode berbahaya di mesin dev | Konfirmasi dengan daftar berkas dan penanda skrip; env bersih tanpa kredensial selain kunci `.env` pilihan; `.env` hanya berisi kredensial lokal/dev (R25a) |
| App tim butuh layanan lain selain database | R26: dev menyalakan layanan pendukung; checklist kesiapan menjelaskannya |
| Refetch AC menambah beban kuota Lark | Dibatasi ke run aktif milik pengguna dan ke pemicu KTD-27 |
