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
- **Product authority:** dokumen ini, dari brainstorm dengan pemilik produk (2026-10-23). Merinci ulang R19–R24 di `docs/plans/2026-10-18-001-feat-korlap-lark-lembur-plan.md` untuk mesin lembur di VM; R-ID di bawah berlaku lokal untuk dokumen ini.
- **Fondasi:** U1–U4 di `docs/plans/2026-10-23-001-feat-daemon-os-service-installer-plan.md` (status `busy`, drain, `mandord install`, `connect` lewat systemd) dipakai apa adanya di VM.
- **Open blockers:** tidak ada. Baseline keberhasilan ditetapkan setelah bulan pertama (lihat Success Criteria).

---

## Product Contract

### Summary

VM Linux bersama yang selalu menyala menjadi mesin lembur: tiap developer punya user Linux dan daemon sendiri di sana, atas nama dirinya. Siang hari developer bekerja dengan daemon laptop seperti biasa dan mengantrekan kartu To Do dari tab Overnight; VM mengerjakan antrean satu kartu per developer sampai Waiting Review, dan pagi hari developer membaca laporan lalu memilih lanjut di VM atau mengambil alih ke laptop.

### Problem Frame

Lembur di laptop pribadi tidak realistis: laptop dibawa pulang, ditutup, kehabisan baterai, atau dipakai untuk hal lain, dan menjaga Windows serta WSL tetap terjaga semalaman rapuh. Rencana mesin lembur di Rilis 1 (R19–R24) mendefinisikan perilaku antrean, pagar, dan laporan pagi, tapi belum menentukan di mesin mana itu berjalan. Tim juga belum punya baseline seberapa banyak kartu rutin yang bisa diselesaikan agent tanpa didampingi.

### Actors

- A1. **Developer:** pemilik kartu dan pemilik identitas (Lark, GitHub, langganan Claude). Bekerja di laptop siang hari, mengantrekan kartu untuk VM.
- A2. **Admin VM:** menyiapkan VM, user Linux per developer, akses SSH, dan memasang Mandor.
- A3. **Daemon laptop:** daemon developer di laptop (WSL atau Linux), seperti sekarang.
- A4. **Daemon VM:** daemon developer yang sama identitasnya, berjalan di user Linux developer di VM sebagai service.
- A5. **Reviewer manusia:** memeriksa PR di Waiting Review dan bisa memindahkan kartu ke Waiting Fix.

### Key Decisions

- **Identitas per developer, bukan akun bot.** Klaim kartu, author PR, dan kuota Claude tetap milik developer, sehingga model klaim, PIC, dan filter penulis tepercaya tidak berubah. Pindah dari VM bersama ke satu VM per developer nanti tidak butuh desain ulang.
- **VM bersama dulu, daemon per user Linux.** Satu VM untuk tim pada tahap awal; tiap developer punya user Linux dan daemon sendiri.
- **Laptop siang, VM malam, dua daemon per developer.** Hanya kartu To Do, atau kartu yang diparkir lembur sebelumnya, yang dikirim ke VM (R21 Rilis 1). Run yang setengah jalan tidak dipindahkan.
- **Login langganan Claude milik developer.** Developer login sendiri ke Claude Code di user Linux-nya di VM; tidak ada API key. Batas pemakaian per malam diukur dari pemakaian yang tercatat.
- **Satu kartu lembur sekaligus per developer.** Antrean tiap developer dikerjakan berurutan, sehingga beban VM dan kuota bisa diprediksi.
- **Review AC malam dengan browser headless.** Kartu bisa sampai Waiting Review tanpa Chrome di VM. Pagi hari developer boleh menjalankan review ulang dengan browser-nya sendiri.
- **Pagar lembur berlaku untuk semua run di VM**, termasuk "Start now" di siang hari, karena run di VM tidak didampingi.
- **Tab Overnight terpisah.** Board tetap untuk pekerjaan laptop; antrean, run VM, dan laporan pagi ada di tab Overnight.

### Requirements

**Koneksi dan penyiapan**

- R1. App di laptop tersambung ke daemon VM lewat SSH, bersamaan dengan daemon laptop, memakai akses SSH developer sendiri.
- R2. Admin bisa menyiapkan VM, user Linux per developer, dan daemon sebagai service dengan runbook, tanpa app desktop di VM.
- R3. Developer menyelesaikan login Lark, Claude Code, dan GitHub untuk daemon VM-nya dari laptop, tanpa membuka desktop VM.
- R4. Daemon VM berjalan sebagai service yang hidup lagi sendiri setelah crash atau reboot VM, dan pembaruannya menunggu run selesai (fondasi U1–U4 plan service OS).
- R5. Daemon tiap developer di VM terpisah dari daemon developer lain: token, repo, worktree, dan app dev satu developer tidak bisa diakses user Linux developer lain.

**Antrean dan jadwal**

- R6. Dari tab Overnight, developer mengirim kartu To Do (atau kartu yang diparkir lembur) ke antrean VM, mengurutkan, dan mengeluarkannya sebelum dikerjakan.
- R7. Developer mengatur jam lembur (mulai dan selesai). Di luar jam itu antrean menunggu, kecuali developer menekan "Start now" pada sebuah kartu.
- R8. Daemon VM mengerjakan satu kartu sekaligus per developer, berurutan sesuai antrean.
- R9. Sebelum mengerjakan kartu, daemon VM memeriksa ulang bahwa kartu masih memenuhi syarat antrean; kartu yang tidak lagi memenuhi dikeluarkan dan dicatat di laporan pagi dengan alasannya.
- R10. Lembur tidak bisa diaktifkan sebelum batas pemakaian per malam diisi. Kartu yang mencapai batas, atau yang terhenti karena kuota langganan habis, diparkir dengan alasan itu.

**Eksekusi dan pagar**

- R11. Kartu lembur dikerjakan sampai PR di Waiting Review, termasuk review AC dengan browser headless terhadap app dev di VM.
- R12. Semua run di VM mematuhi pagar lembur Rilis 1 (R22/R22a): tidak merge ke main, tidak push ke branch lain, status Lark tidak melewati Waiting Review, tidak menyentuh database atau environment staging, tidak mengirim pesan, dan agent berjalan tanpa akses ke kredensial developer.
- R13. Agent yang buntu memarkir kartu dengan catatan pertanyaan lalu lanjut ke kartu berikutnya.

**Pagi hari dan lanjutan**

- R14. Laporan pagi di tab Overnight menampilkan per kartu: hasil (Waiting Review, diparkir, dikeluarkan, gagal), PR, hasil review AC, pertanyaan parkir, dan pemakaian; serta ringkasan per malam.
- R15. Developer bisa menjalankan review AC ulang dengan browser di laptop terhadap app dev kartu itu di VM.
- R16. Developer menjawab pertanyaan kartu yang diparkir lalu mengembalikannya ke antrean, yang melanjutkan branch yang sama.
- R17. Perbaikan setelah Waiting Review (Waiting Fix) secara default dikerjakan di VM. Developer bisa memilih "Take over to laptop"; setelah itu kartu dikerjakan di laptop dan tidak bisa kembali ke antrean VM.
- R18. Board di laptop menandai kartu yang sedang ditangani VM sebagai "On VM" dengan tautan ke tab Overnight, bukan sebagai kartu yang diklaim di tempat lain.

### Key Flows

- F1. **Penyiapan:** admin membuat user Linux developer, memberi akses SSH, dan memasang daemon sebagai service. Developer menambahkan VM di app laptop, app tersambung lewat SSH, lalu memandu login Lark, Claude Code, dan GitHub untuk daemon VM.
- F2. **Mengantrekan:** siang hari developer membuka tab Overnight, menambahkan beberapa kartu To Do ke antrean VM, mengisi batas pemakaian dan jam lembur, lalu menutup laptop.
- F3. **Malam:** pada jam lembur daemon VM mengambil kartu pertama, memeriksa ulang syaratnya, mengklaim, mengerjakan, menjalankan review AC headless, membuka PR, memindahkan kartu ke Waiting Review, lalu mengambil kartu berikutnya. Kartu yang buntu atau mencapai batas diparkir.
- F4. **Pagi:** developer membuka app, membaca laporan pagi, menjalankan review ulang dengan browser bila perlu, menjawab pertanyaan kartu yang diparkir, dan mengembalikannya ke antrean atau mengambil alih ke laptop.
- F5. **Perbaikan:** reviewer memindahkan kartu ke Waiting Fix. Daemon VM mengerjakan perbaikannya, kecuali developer sudah memilih "Take over to laptop".

### Acceptance Examples

- AE1. **Covers R7, R8.** Given developer mengantrekan tiga kartu dengan jam lembur 19:00–06:00, when pukul 19:00, then VM mengerjakan kartu pertama; kartu kedua baru mulai setelah kartu pertama selesai atau diparkir.
- AE2. **Covers R7, R12.** Given pukul 14:00 dan developer menekan "Start now" pada kartu di antrean, when kartu dikerjakan, then kartu langsung mulai dan semua pagar lembur tetap berlaku, termasuk larangan mengirim pesan.
- AE3. **Covers R9.** Given kartu di antrean sudah dipindahkan orang lain ke In Progress di Lark, when jam lembur tiba, then kartu dikeluarkan dari antrean dan laporan pagi mencatat alasannya.
- AE4. **Covers R10.** Given batas pemakaian per malam tercapai saat kartu kedua berjalan, when batas tercapai, then agent dihentikan, kartu diparkir dengan alasan "usage limit", dan kartu ketiga tidak dimulai.
- AE5. **Covers R17.** Given kartu lembur di Waiting Fix dan developer memilih "Take over to laptop", when developer membuka kartu di laptop, then run berjalan di laptop pada branch yang sama dan opsi kirim ke VM tidak tersedia untuk kartu itu.
- AE6. **Covers R5.** Given dua developer punya daemon di VM yang sama, when agent milik developer A berjalan, then agent itu tidak bisa membaca token, repo, atau worktree developer B.
- AE7. **Covers R18.** Given kartu sedang dikerjakan VM, when developer melihat board di laptop, then kartu bertanda "On VM" dan membuka tab Overnight saat diklik.

### Success Criteria

- Developer bisa menutup laptop sore hari dan pagi hari menemukan kartu yang diantrekan sudah di Waiting Review atau diparkir dengan pertanyaan jelas, tanpa intervensi malam hari.
- Laporan pagi merekam angka per malam (kartu sampai Waiting Review, diparkir, gagal, pemakaian, kartu yang lolos review tanpa perbaikan besar). Target keberhasilan ditetapkan dari angka bulan pertama.
- Admin bisa menyiapkan developer baru di VM dari runbook saja.

### Scope Boundaries

- Memindahkan run yang setengah jalan dari laptop ke VM.
- Akun bot tim, API key Claude, atau daemon bersama untuk banyak developer.
- Daemon native Windows (tanpa WSL).
- Pembuatan VM otomatis atau infrastruktur cloud sebagai kode; admin menyiapkan VM lewat runbook.
- Menjalankan lebih dari satu kartu lembur sekaligus per developer.
- Chrome dengan Claude in Chrome di VM.
- Auto-resume run yang terputus oleh crash daemon (tetap Resume manual seperti Rilis 1).

### Dependencies / Assumptions

- Fondasi U1–U4 di plan service OS (`docs/plans/2026-10-23-001-feat-daemon-os-service-installer-plan.md`) selesai lebih dulu.
- VM Linux dengan systemd, Docker, dan sumber daya cukup untuk satu run per developer aktif sekaligus (app dev, Docker, dan browser headless).
- Login langganan Claude Code di VM oleh developer sendiri sesuai ketentuan Anthropic yang dicatat di plan Rilis 1 (binary Claude Code tanpa modifikasi, login dev sendiri).
- Admin VM bersama secara teknis bisa membaca token setiap developer. Risiko ini diterima di tahap VM bersama dan hilang bila pindah ke satu VM per developer.
- Repo yang dikerjakan lembur punya test dan app dev yang bisa berjalan di Linux tanpa database staging.

### Outstanding Questions

**Deferred to Planning**

- Cara login OAuth Lark dari laptop ke daemon VM (callback daemon di `127.0.0.1:53682` di VM; port forward SSH atau alur lain).
- Mekanisme isolasi agent dari kredensial developer di VM (user OS terpisah, container, atau sandbox) yang memenuhi R5 dan R12.
- Pengganti Claude in Chrome untuk review AC headless dan cara review pagi dari browser laptop menjangkau app dev di VM.
- Bentuk serah-terima "Take over to laptop" (checkout branch PR di daemon laptop, perlakuan klaim antar dua daemon dengan identitas yang sama).
- Urutan kerja relatif terhadap plan service OS: U1–U4, lalu mode VM ini, sebelum installer Windows dan keep-awake (U5–U7).

### Sources

- Plan Rilis 1, R19–R24 dan catatan ketentuan Anthropic: `docs/plans/2026-10-18-001-feat-korlap-lark-lembur-plan.md`
- Plan service OS (fondasi U1–U4): `docs/plans/2026-10-23-001-feat-daemon-os-service-installer-plan.md`
- Mode koneksi app saat ini (`LinkMode::Local`, `LinkMode::Wsl`): `apps/desktop/src-tauri/src/daemon_link.rs`
- Callback OAuth Lark di daemon: `crates/mandor-lark/src/client.rs`, `crates/mandord/src/settings.rs`
