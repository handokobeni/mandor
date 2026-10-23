# Runbook: review AC (Rilis 2.0)

Panduan menyiapkan review Acceptance Criteria (AC) di Mandor: sumber desain, app web untuk diuji di browser, gerbang Push & PR, dan cara membaca hasil serta biaya review. Ikuti dulu `setup-lark-app.md` (dan `setup-wsl.md` bila memakai WSL).

## 1. Ringkasan alur

1. **Start**: pilih sumber desain (opsional) dan skill reviewer. Fase plan membandingkan setiap AC dengan desain dan mengisi kolom Design (In design, Partly, Missing, Conflicts).
2. **Approve plan**: di tab Plan, tandai AC yang desainnya lebih baru dengan **Follow design**. AC itu keluar dari gerbang, dan selisih desain ditulis ke field `mandor_design_gaps` di kartu Lark.
3. **Review**: tombol **Push & create PR** memulai review bila AC belum direview. Reviewer membaca kode dan test, dan bila app web berjalan, mencoba app di Chrome Anda. Hasil per AC: Pass, Fail, atau Can't verify.
4. **Gerbang**: AC Fail memblokir push (mode Block) atau menjadi peringatan (mode Warn). AC Can't verify harus Anda konfirmasi. **Push anyway** butuh alasan dan dicatat di PR.
5. **PR**: deskripsi PR berisi ringkasan AC.

Mandor tidak pernah mengubah status Lark di luar allowlist Rilis 1 dan tidak mengirim pesan ke siapa pun.

## 2. Ekstensi Claude in Chrome

Reviewer memakai browser keluarga Chrome milik Anda (Chrome, Edge, atau Chromium) lewat ekstensi Claude in Chrome.

1. Pasang ekstensi Claude in Chrome dari Chrome Web Store dan login dengan akun Claude yang sama dengan `claude` di mesin daemon.
2. Di WSL, Chrome Windows tersambung ke `claude` di WSL lewat native messaging. Jalankan `claude --chrome` sekali dari terminal WSL untuk memastikan sambungan.
3. Di Mandor, buka **Settings → About → Claude in Chrome → Check connection**. Status "Connected" disimpan 60 detik; checklist kesiapan memakai hasil ini.

Pagar yang berlaku selama review:

- Reviewer hanya memakai tab yang ia buat sendiri, bukan tab Anda.
- Navigasi hanya ke origin app run ini (port atau App URL di setelan repo). Alamat lain ditolak hook dengan pesan "that address is outside the app allowed for this review".
- Reviewer tidak menjalankan skrip di halaman dan tidak mengunggah berkas.
- Isi halaman dibaca model AI. Jangan login dengan akun produksi atau data pelanggan sungguhan.

## 3. Setelan repo web

**Settings → Repos → pilih repo**:

| Setelan | Isi |
|---|---|
| Test mode | **Web** bila repo punya app yang bisa dijalankan; **No app** membuat AC UI dinilai Can't verify |
| Setup command | Opsional, mis. `bun install`. Dijalankan bila lockfile berubah sejak app terakhir |
| Start command | Wajib untuk Web, mis. `bun run dev` |
| Port | Port yang dibuka app, mis. `5173`. Port yang sudah dipakai program lain membuat app Failed |
| App URL | Opsional. Alamat tempat Anda login (cookie login terikat ke host ini) |
| .env keys | Nama kunci yang diteruskan dari `.env` repo. Nilai tidak pernah ditampilkan dan tidak disalin ke worktree |
| Browser testing | Izinkan reviewer menguji app di browser (run satu repo). Run workspace memakai setelan workspace |
| Push & PR gate | Use workspace, Block, atau Warn. Run multi-repo memakai yang paling ketat |

Catatan:

- `.env` hanya boleh berisi kredensial lokal atau dev. Mandor membaca nilai yang dipilih saat menyalakan app; agent tidak pernah menerimanya.
- Database dan layanan pendukung tetap Anda nyalakan sendiri. Mandor tidak menyiapkan database atau akun.
- Hanya satu app run yang menyala dalam satu waktu. Sidebar menampilkan run pemilik app.
- Snapshot app mengikuti `.gitignore`. Berkas ignored yang ditulis agent (mis. di `node_modules`) tidak masuk konfirmasi perubahan.

**Run app** di workspace run menyalakan app dari worktree run itu. Sebelum review pertama dengan browser, dialog kesiapan meminta Anda membuka app, login, lalu menandai **I'm logged in**.

## 4. Sumber desain

**Settings → Design sources → Add source**: nama dan path folder (atau repo lokal) berisi desain, mis. prototipe HTML atau ekspor desain. Mandor hanya membaca folder ini, dan agent tidak bisa menulis ke dalamnya. Sumber yang path-nya hilang ditandai **Path missing** dan tidak bisa dipilih di Start.

## 5. View "Design gaps" di Lark

Field `mandor_design_gaps` (Text) dibuat otomatis saat login. Isinya satu baris per AC yang berbeda dari desain, mis. `AC-4: Conflicts with design: amber dot, not red badge`, atau `AC-5: Follow design: PM please update the AC`. Field dikosongkan bila tidak ada selisih.

Membuat view untuk desainer dan PM:

1. Buka tabel Sprint Tasks → **+ View** → Grid, beri nama **Design gaps**.
2. **Filter**: `mandor_design_gaps` **is not empty**.
3. Tampilkan kolom judul, Assignee, Task Status, dan `mandor_design_gaps`. Urutkan menurut waktu ubah terbaru.
4. Bagikan view ke desainer. Mandor tidak mengirim notifikasi ke desainer; view ini yang dipantau.

## 6. Mengukur task yang kembali karena AC terlewat

Riwayat status saja tidak menunjukkan alasan task dikembalikan, jadi reviewer manusia perlu menandainya.

1. Pemilik Base menambah field **Return reason** (Single select) di Sprint Tasks dengan opsi `AC missed` dan `Other`. Mandor tidak menulis field ini.
2. Setiap kali reviewer memindahkan task ke Waiting Fix, ia mengisi Return reason.
3. **Baseline**: hitung task Rilis 1 (dikerjakan lewat Mandor) dengan Return reason `AC missed` selama empat minggu **sebelum** tim mulai memakai review AC. Tetapkan target penurunan dan jendela pengukuran (panjang yang sama) sebelum Rilis 2 dipakai, lalu catat keduanya di `docs/progress/rilis-2.md`.
4. Bandingkan dengan jumlah yang sama setelah Rilis 2 dipakai.

## 7. Membaca biaya review

- **Settings → General → AC reviewer**: model reviewer (kosong = model agent pelaksana), batas biaya per review (bawaan US$2,00), dan batas waktu (bawaan 15 menit). Review yang melewati batas berakhir Failed; AC yang belum dinilai tetap Not reviewed.
- Tab **AC review → Review history**: setiap review mencatat pemicu (Manual, Push & PR, Automatic fix), status, apakah browser dipakai (beserta alasan bila tidak), biaya dalam USD, serta token input dan output.
- Biaya berasal dari laporan `claude` di akhir sesi. Bila batas biaya tercapai, token diambil dari `modelUsage`.

## 8. Masalah umum

| Gejala | Penyebab dan tindakan |
|---|---|
| Review tanpa browser, alasan "No app for this run" | Test mode repo No app, atau app run ini tidak menyala. Nyalakan **Run app** |
| Alasan tanpa browser "Claude in Chrome extension not connected" | Ekstensi belum login atau native messaging belum tersambung. Jalankan `claude --chrome` sekali, lalu **Check connection** di Settings → About |
| App Failed: port already in use | Program lain memakai port. Hentikan program itu atau ganti port |
| Review stale | Kode berubah setelah verdict. Jalankan review lagi sebelum push |
| Push held (Waiting Fix) | Review otomatis menemukan AC Fail. Buka tab AC review, kirim temuan ke agent atau push anyway dengan alasan |
| Sumber desain Path missing | Folder dipindah atau dihapus. Perbaiki path di Settings → Design sources |
