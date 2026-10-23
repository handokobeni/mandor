---
title: Mode VM lembur lewat SSH - Plan
type: feat
date: 2026-10-23
topic: mandor-vm-overnight-mode
artifact_contract: ce-unified-plan/v1
artifact_readiness: requirements-only
product_contract_source: ce-brainstorm
execution: code
---

# Mode VM lembur lewat SSH - Plan

## Goal Capsule

- **Objective:** agent mengerjakan kartu semalaman di VM yang selalu menyala, atas nama developer masing-masing, tanpa bergantung pada laptop pribadi.
- **Product authority:** dokumen ini, dari brainstorm dan review dokumen dengan pemilik produk (2026-10-23). Merinci ulang R19–R24 di `docs/plans/2026-10-18-001-feat-korlap-lark-lembur-plan.md` untuk mesin lembur di VM; R-ID di bawah berlaku lokal untuk dokumen ini.
- **Fondasi:** U1–U4 di `docs/plans/2026-10-23-001-feat-daemon-os-service-installer-plan.md` (status `busy`, drain, `mandord install`, `connect` lewat systemd) dipakai apa adanya di VM.
- **Open blockers:** tidak ada untuk planning. Kesesuaian dengan ketentuan Anthropic (Dependencies) harus dipastikan sebelum implementasi.

---

## Product Contract

### Summary

VM Linux bersama yang selalu menyala menjadi mesin lembur: tiap developer punya user Linux dan daemon sendiri di sana, atas nama dirinya. Siang hari developer bekerja dengan daemon laptop seperti biasa dan mengantrekan kartu To Do dari tab Overnight; VM mengerjakan antrean satu kartu per developer sampai Waiting Review, mengerjakan Waiting Fix-nya, dan pagi hari developer membaca laporan lalu memilih lanjut di VM atau mengambil alih ke laptop.

### Problem Frame

Lembur di laptop pribadi tidak realistis: laptop dibawa pulang, ditutup, kehabisan baterai, atau dipakai untuk hal lain, dan menjaga Windows serta WSL tetap terjaga semalaman rapuh. Rencana mesin lembur di Rilis 1 (R19–R24) mendefinisikan perilaku antrean, pagar, dan laporan pagi, tapi belum menentukan di mesin mana itu berjalan. Tim juga belum punya baseline seberapa banyak kartu rutin yang bisa diselesaikan agent tanpa didampingi.

### Actors

- A1. **Developer:** pemilik kartu dan pemilik identitas (Lark, GitHub, langganan Claude). Bekerja di laptop siang hari, mengantrekan kartu untuk VM.
- A2. **Admin VM:** menyiapkan VM, user Linux per developer, akses SSH, dan memasang Mandor.
- A3. **Daemon laptop:** daemon developer di laptop (WSL atau Linux), seperti sekarang.
- A4. **Daemon VM:** daemon developer yang sama identitas Lark-nya, berjalan di user Linux developer di VM sebagai service.
- A5. **Reviewer manusia:** memeriksa PR di Waiting Review dan bisa memindahkan kartu ke Waiting Fix.

### Key Decisions

- **Identitas Lark per developer, klaim per daemon.** Klaim kartu, author PR, dan kuota Claude tetap milik developer, sehingga PIC dan filter penulis tepercaya tidak berubah. Klaim juga mencatat daemon mana (laptop atau VM) yang memegang kartu, karena dua daemon developer yang sama tidak boleh sama-sama menganggap kartu itu miliknya. Pindah dari VM bersama ke satu VM per developer nanti tidak butuh desain ulang.
- **VM bersama dulu, daemon per user Linux.** Satu VM untuk tim pada tahap awal; tiap developer punya user Linux dan daemon sendiri.
- **Laptop siang, VM malam, dua daemon per developer.** Hanya kartu To Do, atau kartu yang diparkir lembur sebelumnya, yang dikirim ke VM (R21 Rilis 1). Run yang setengah jalan tidak dipindahkan.
- **Login langganan Claude milik developer.** Developer login sendiri ke Claude Code di user Linux-nya di VM; tidak ada API key.
- **Satu kartu sekaligus per developer.** Antrean dan perbaikan tiap developer di VM dikerjakan berurutan, sehingga beban VM dan kuota bisa diprediksi.
- **Review AC di VM dengan browser headless.** Kartu bisa sampai Waiting Review tanpa Chrome di VM. Pagi hari developer boleh menjalankan review ulang dengan browser-nya sendiri.
- **Waiting Fix kartu VM dikerjakan langsung di VM**, tanpa menunggu jam lembur, sehingga satu putaran review tidak memakan satu hari.
- **Pagar lembur berlaku untuk semua run di VM**, termasuk "Start now" dan Waiting Fix di siang hari, karena run di VM tidak didampingi.
- **Tab Overnight terpisah.** Board tetap untuk pekerjaan laptop; antrean, run VM, dan laporan pagi ada di tab Overnight.
- **Satu rilis.** R1–R20 dibangun bersama; tidak ada tahap yang menunggu baseline.

### Requirements

**Koneksi dan penyiapan**

- R1. App di laptop tersambung ke daemon VM lewat SSH, bersamaan dengan daemon laptop, memakai akses SSH developer sendiri. Host key VM dipin saat pertama tersambung, dan perubahan host key menghentikan koneksi dengan peringatan.
- R2. Admin bisa menyiapkan VM, user Linux per developer, dan daemon sebagai service dengan runbook, tanpa app desktop di VM.
- R3. Developer login Lark, Claude Code, dan GitHub untuk daemon VM-nya sekali lewat terminal SSH mengikuti runbook. App menampilkan status ketiga login itu beserta langkah memperbaikinya.
- R4. Daemon VM berjalan sebagai service yang hidup lagi sendiri setelah crash atau reboot VM, dan pembaruannya menunggu run selesai (fondasi U1–U4 plan service OS).
- R5. Daemon tiap developer di VM terpisah dari developer lain: token, repo, worktree, container, dan app dev satu developer tidak bisa diakses user Linux developer lain atau agent-nya. Daemon dan app dev hanya mendengarkan di loopback dan dijangkau laptop lewat SSH.

**Antrean dan jadwal**

- R6. Dari tab Overnight, developer mengirim kartu To Do (atau kartu yang diparkir lembur) ke antrean VM, mengurutkan, dan mengeluarkannya sebelum dikerjakan.
- R7. Developer mengatur jam lembur (mulai dan selesai) dalam zona waktunya. Di luar jam itu antrean menunggu, kecuali developer menekan "Start now" pada sebuah kartu. Kartu yang masih berjalan saat jam lembur berakhir diselesaikan, tapi kartu baru tidak dimulai.
- R8. Daemon VM mengerjakan satu kartu sekaligus per developer, berurutan sesuai antrean; Waiting Fix kartu VM mendapat giliran sebelum kartu antrean berikutnya.
- R9. Sebelum mengerjakan kartu, daemon VM memeriksa ulang bahwa kartu masih memenuhi syarat antrean; kartu yang tidak lagi memenuhi dikeluarkan dan dicatat di laporan pagi dengan alasannya.
- R10. VM tidak bisa diaktifkan sebelum developer mengisi batas pemakaian harian VM (token). Semua run VM, termasuk "Start now" dan Waiting Fix, dihitung ke batas itu. Saat batas tercapai, run dihentikan, kartunya diparkir dengan alasan "usage limit", dan tidak ada kartu VM lain yang dimulai sampai hari berikutnya. Kartu yang terhenti karena kuota langganan habis diparkir dengan alasan yang sama.

**Eksekusi dan pagar**

- R11. Kartu lembur dikerjakan sampai PR di Waiting Review, termasuk review AC dengan browser headless terhadap app dev di VM.
- R12. Bila gerbang Push & PR tidak siap (ada AC Fail atau "Can't verify" yang belum dikonfirmasi), VM tidak melakukan push. Kartu diparkir dengan alasan "AC gate", dan laporan pagi menampilkan baris AC-nya untuk diputuskan developer.
- R13. Semua run di VM mematuhi pagar lembur Rilis 1 (R22/R22a): tidak merge ke main, tidak push ke branch lain, status Lark tidak melewati Waiting Review, tidak menyentuh database atau environment staging, dan tidak mengirim pesan.
- R14. Agent di VM tidak bisa membaca token Lark, kredensial GitHub, atau token daemon. Satu-satunya kredensial yang dipakai agent adalah login Claude Code developer. Push, pembuatan PR, dan penulisan Lark hanya dilakukan daemon, ke branch kartu itu.
- R15. Agent yang buntu memarkir kartu dengan catatan pertanyaan lalu lanjut ke kartu berikutnya.

**Kepemilikan kartu antar dua daemon**

- R16. Hanya daemon yang memegang klaim kartu yang boleh menjalankan run, memproses komentar Waiting Fix, dan memperbarui klaim untuk kartu itu. Daemon lain milik developer yang sama menampilkan kartu itu hanya-baca.
- R17. Board di laptop menandai kartu yang dipegang daemon VM sebagai "On VM" dengan tautan ke tab Overnight, bukan sebagai kartu yang diklaim di tempat lain.

**Pagi hari dan lanjutan**

- R18. Laporan pagi di tab Overnight menampilkan per kartu: hasil (Waiting Review, diparkir beserta alasan, dikeluarkan, gagal), PR, hasil review AC, pertanyaan parkir, dan pemakaian. Ringkasan per hari mencatat jumlah tiap hasil, pemakaian terhadap batas, dan jumlah kartu yang lolos review tanpa pernah masuk Waiting Fix.
- R19. Developer bisa menjalankan review AC ulang dengan browser di laptop terhadap app dev kartu itu di VM.
- R20. Waiting Fix kartu VM dikerjakan di VM begitu kartu masuk Waiting Fix, dengan semua pagar lembur. Developer bisa memilih "Take over to laptop"; klaim berpindah ke daemon laptop sebelum laptop mulai bekerja, dan kartu tidak bisa kembali ke antrean VM. Developer menjawab pertanyaan kartu yang diparkir lalu mengembalikannya ke antrean, yang melanjutkan branch yang sama.

### Key Flows

- F1. **Penyiapan:** admin membuat user Linux developer, memberi akses SSH, dan memasang daemon sebagai service. Developer login Lark, Claude Code, dan GitHub lewat terminal SSH, lalu menambahkan VM di app laptop; app tersambung lewat SSH dan menampilkan status login.
- F2. **Mengantrekan:** siang hari developer membuka tab Overnight, menambahkan beberapa kartu To Do ke antrean VM, mengisi batas pemakaian harian dan jam lembur, lalu menutup laptop.
- F3. **Malam:** pada jam lembur daemon VM mengambil kartu pertama, memeriksa ulang syaratnya, mengklaim, mengerjakan, menjalankan review AC headless, membuka PR bila gerbang AC siap, memindahkan kartu ke Waiting Review, lalu mengambil kartu berikutnya. Kartu yang buntu atau tertahan gerbang AC diparkir, lalu VM lanjut ke kartu berikutnya. Saat batas pemakaian tercapai, VM berhenti mengambil kartu sampai hari berikutnya.
- F4. **Pagi:** developer membuka app, membaca laporan pagi, menjalankan review ulang dengan browser bila perlu, memutuskan kartu yang tertahan gerbang AC, menjawab pertanyaan kartu yang diparkir, dan mengembalikannya ke antrean atau mengambil alih ke laptop.
- F5. **Perbaikan:** reviewer memindahkan kartu ke Waiting Fix. Daemon VM langsung mengerjakan perbaikannya, kecuali developer sudah memilih "Take over to laptop".

### Acceptance Examples

- AE1. **Covers R7, R8.** Given developer mengantrekan tiga kartu dengan jam lembur 19:00–06:00, when pukul 19:00, then VM mengerjakan kartu pertama; kartu kedua baru mulai setelah kartu pertama selesai atau diparkir.
- AE2. **Covers R7.** Given kartu dimulai pukul 05:40 dan jam lembur berakhir 06:00, when pukul 06:00, then kartu itu terus dikerjakan sampai selesai atau diparkir, dan kartu berikutnya tidak dimulai.
- AE3. **Covers R7, R13.** Given pukul 14:00 dan developer menekan "Start now" pada kartu di antrean, when kartu dikerjakan, then kartu langsung mulai dan semua pagar lembur tetap berlaku, termasuk larangan mengirim pesan.
- AE4. **Covers R9.** Given kartu di antrean sudah dipindahkan orang lain ke In Progress di Lark, when jam lembur tiba, then kartu dikeluarkan dari antrean dan laporan pagi mencatat alasannya.
- AE5. **Covers R10.** Given batas pemakaian harian tercapai saat kartu kedua berjalan, when batas tercapai, then agent dihentikan, kartu diparkir dengan alasan "usage limit", dan kartu ketiga tidak dimulai sampai hari berikutnya.
- AE6. **Covers R12.** Given review AC headless menghasilkan satu baris "Can't verify", when kartu siap di-push, then VM tidak push, kartu diparkir dengan alasan "AC gate", dan baris itu tampil di laporan pagi.
- AE7. **Covers R5, R14.** Given dua developer punya daemon di VM yang sama, when agent milik developer A berjalan, then agent itu tidak bisa membaca token, repo, atau worktree developer B, tidak bisa menjalankan container yang memasang home developer B, dan tidak bisa membaca token Lark atau kredensial GitHub developer A sendiri.
- AE8. **Covers R16, R17.** Given kartu dipegang daemon VM, when daemon laptop developer yang sama melihat kartu itu (termasuk saat kartu di Waiting Fix), then daemon laptop tidak menjalankan run atau memproses komentarnya, dan board menampilkan "On VM".
- AE9. **Covers R20.** Given kartu VM masuk Waiting Fix pukul 11:00, when kartu masuk Waiting Fix, then daemon VM langsung mengerjakan perbaikannya tanpa menunggu jam lembur.
- AE10. **Covers R20.** Given kartu lembur di Waiting Fix dan developer memilih "Take over to laptop", when developer membuka kartu di laptop, then klaim sudah berpindah ke daemon laptop, run berjalan di laptop pada branch yang sama, dan opsi kirim ke VM tidak tersedia untuk kartu itu.

### Success Criteria

- Developer bisa menutup laptop sore hari dan pagi hari menemukan kartu yang diantrekan sudah di Waiting Review atau diparkir dengan alasan jelas, tanpa intervensi malam hari.
- Laporan pagi merekam angka per hari (kartu sampai Waiting Review, diparkir per alasan, gagal, pemakaian, kartu yang lolos review tanpa pernah masuk Waiting Fix). Target ditetapkan dari angka bulan pertama.
- Setelah bulan pertama, tim memutuskan dari angka itu dan dari pengalaman developer di pagi hari: lanjut dengan VM bersama, pindah ke satu VM per developer, atau berhenti.
- Admin bisa menyiapkan developer baru di VM dari runbook saja.

### Scope Boundaries

- Memindahkan run yang setengah jalan dari laptop ke VM.
- Akun bot tim, API key Claude, atau daemon bersama untuk banyak developer.
- Daemon native Windows (tanpa WSL).
- Pembuatan VM otomatis atau infrastruktur cloud sebagai kode; admin menyiapkan VM lewat runbook.
- Login yang dipandu dari app; login lewat terminal SSH dengan runbook.
- Menjalankan lebih dari satu kartu VM sekaligus per developer.
- Chrome dengan Claude in Chrome di VM.
- Auto-resume run yang terputus oleh crash daemon (tetap Resume manual seperti Rilis 1).

### Dependencies / Assumptions

- Fondasi U1–U4 di plan service OS (`docs/plans/2026-10-23-001-feat-daemon-os-service-installer-plan.md`) selesai lebih dulu.
- **Harus dipastikan sebelum implementasi:** ketentuan Anthropic yang berlaku mengizinkan login langganan Claude Code milik developer berjalan tanpa didampingi di user Linux-nya pada VM bersama. Hasil dan sumbernya dicatat. Bila tidak diizinkan, keputusan "tanpa API key" dibuka lagi.
- VM Linux dengan systemd dan runtime container per user (rootless); tidak ada developer atau agent di grup `docker` bersama.
- Sumber daya VM cukup untuk satu run per developer aktif sekaligus (app dev, container, dan browser headless).
- Admin VM bersama secara teknis bisa membaca token setiap developer. Risiko ini diterima di tahap VM bersama dengan kontrol: admin sesedikit mungkin, dan runbook memuat langkah mencabut token saat admin atau developer keluar. Risiko ini hilang bila pindah ke satu VM per developer.
- Repo yang dikerjakan lembur punya test dan app dev yang bisa berjalan di Linux tanpa database staging.

### Outstanding Questions

**Deferred to Planning**

- Cara app memegang dua koneksi daemon (laptop dan VM lewat SSH) sekaligus, dan bagaimana RPC, event, board, serta tab Overnight dirutekan per daemon; app sekarang hanya memegang satu.
- Bentuk identitas daemon di klaim Lark dan cara "Take over to laptop" memindahkannya.
- Login OAuth Lark dari terminal SSH (callback daemon di `127.0.0.1:53682` di VM lewat port forward SSH).
- Mekanisme isolasi agent dari kredensial developer di VM (user OS terpisah, container, atau sandbox) yang memenuhi R5 dan R14 sambil tetap memakai login Claude Code developer.
- Pengganti Claude in Chrome untuk review AC headless, dan cara review ulang dari browser laptop (R19) bekerja: Claude in Chrome di laptop terhadap ledger AC dan worktree di VM, serta app dev yang dinyalakan ulang saat dibutuhkan, bukan dibiarkan menyala.
- Penanda di PR atau kartu bahwa pekerjaan berasal dari run VM tanpa pendampingan, agar reviewer tahu.
- Beban polling Lark yang berlipat karena dua daemon per developer.
- Urutan kerja relatif terhadap plan service OS: U1–U4, lalu mode VM ini, sebelum installer Windows dan keep-awake (U5–U7).

### Sources

- Plan Rilis 1, R19–R24 dan catatan ketentuan Anthropic: `docs/plans/2026-10-18-001-feat-korlap-lark-lembur-plan.md`
- Plan service OS (fondasi U1–U4): `docs/plans/2026-10-23-001-feat-daemon-os-service-installer-plan.md`
- Klaim kartu (`open_id|ts_ms|nonce`, `may_claim`): `crates/mandor-domain/src/claim.rs`, `crates/mandor-usecase/src/sync/claim.rs`
- Gerbang Push & PR (`GateMode::Block`): `crates/mandor-domain/src/push_gate.rs`
- Satu koneksi daemon di app (`LinkMode::Local`, `LinkMode::Wsl`): `apps/desktop/src-tauri/src/daemon_link.rs`, `apps/desktop/src-tauri/src/lib.rs`
- Review dengan Claude in Chrome (`--chrome`, allowlist `mcp__claude-in-chrome`): `crates/mandor-agent/src/port.rs`
- Callback OAuth Lark di daemon: `crates/mandor-lark/src/client.rs`, `crates/mandord/src/settings.rs`
