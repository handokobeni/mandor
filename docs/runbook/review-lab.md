# Runbook: review lab

Review lab menguji AC yang tidak bisa dibuktikan review biasa: AC yang butuh akun role lain, kegagalan request yang disimulasikan, atau berkas uji. Lab berjalan otomatis setelah review biasa, per kartu, dengan akun uji yang sudah Anda setujui. Fase 1 tidak mengubah data: setiap request tulis diblokir. Fase 2 (bagian 5) menambah **lab dengan data uji**: lab boleh mengubah data di penyimpanan lokal yang Anda setujui, lalu Mandor memulihkannya dan membuktikan datanya kembali seperti semula. Rencana: `docs/plans/2026-11-10-001-feat-review-lab-plan.md`.

## 1. Prasyarat

| Komponen | Versi dan lokasi |
|---|---|
| Playwright MCP | **0.0.83** (dipin). Tool kode bernama `browser_run_code_unsafe`; versi lain bisa mengganti nama tool atau flag |
| Chromium Playwright | revisi **1247** di `~/.cache/ms-playwright` |
| Biner | Laptop: dicari otomatis di `~/.local/bin/playwright-mcp`, lalu `/usr/local/bin/playwright-mcp`, lalu `/usr/local/bin/mcp-server-playwright`. VM: `MANDOR_PLAYWRIGHT_MCP` (bawaan `/usr/local/bin/mcp-server-playwright`) |

Pemasangan di laptop (sekali):

```sh
npm install --prefix ~/.local -g @playwright/mcp@0.0.83
~/.local/bin/playwright-mcp --version
```

Pasang Chromium yang dipakai MCP itu dengan `npx playwright install chromium` dari versi `playwright` yang ikut terpasang bersama `@playwright/mcp@0.0.83` (lihat `~/.local/lib/node_modules/@playwright/mcp/package.json`), lalu pastikan revisi 1247 ada di `~/.cache/ms-playwright`.

`MANDOR_PLAYWRIGHT_MCP` harus path absolut bila diisi. Tanpa biner Playwright, review berjalan tanpa browser dengan alasan "Playwright is not installed". Review tidak lagi memakai Claude in Chrome.

## 2. Menyiapkan perlengkapan lab

**Settings → Repos → pilih repo → Review lab** (hanya repo dengan Test mode Web):

1. **Propose from repo**: agent pengusul membaca salinan bersih branch default repo (hanya berkas ter-commit, tanpa `.env`) dan mengusulkan akun uji, form login, path POST baca-saja, dan folder berkas uji. Batas 100 sen dan 10 menit. Agent tidak punya shell, browser, maupun akses kredensial; usulan yang memuat password ditolak.
2. **Review the proposal** memuat usulan ke form. Periksa setiap field. Usulan hanya ada di memori daemon; setelah daemon restart, minta usulan lagi.
3. Isi **password** setiap akun. Password disimpan di `~/.mandor/lab/<repo>/private/secrets.json` (0600) dan tidak pernah ditampilkan lagi. Field kosong mempertahankan password tersimpan.
4. **Save lab equipment**, lalu **Approve…**. Dialog menampilkan akun, URL login dan path POST login, tanda sudah login, path POST baca-saja, dan folder berkas uji. Setiap perubahan setelah disetujui membatalkan persetujuan.

Aturan perlengkapan:

| Field | Aturan |
|---|---|
| Role | Huruf kecil, angka, `-`; maks. 16 karakter; maks. 4 akun; tepat satu **Default** (dipakai review biasa) |
| Login page URL, page that needs a login | Di origin app repo (loopback, mis. `http://localhost:5173`) |
| Selector | `#id`, `[name=x]`, atau `text=...`; maks. 120 karakter |
| Login request path | Path POST form login; hanya request ini yang lolos proxy saat login |
| Read-only POST | Path POST yang hanya membaca, mis. `/api/invoices/search`; tanpa query string |
| Test files folder | Relatif terhadap repo. Disalin ke staging milik daemon: hanya berkas biasa ≤ 50 MB (maks. 500 berkas), tanpa dotfile dan symlink; folder yang jalurnya melewati symlink ditolak |

Login berbentuk satu form. App yang meminta pilihan tambahan setelah login (mis. memilih corporate) harus memakai akun uji yang langsung masuk; untuk kelola-v2, setiap akun uji memiliki satu corporate.

Di VM, `unattended_login` tetap menjadi sumber login review biasa sampai perlengkapan repo disetujui.

## 3. Saat review berjalan

1. Review biasa login dengan akun **Default** lewat Playwright headless. Semua request lewat proxy review: origin selain app ditolak, request tulis dijawab `418` dengan header `X-Mandor-Proxy: blocked`, kecuali path POST login dan path baca-saja.
2. AC yang dinilai Can't verify dengan kebutuhan lab (`role`, `simulation`, `file`) menjadi calon lab. AC yang butuh data (`data`) diuji lab dengan data uji (bagian 5) bila perlengkapan data disetujui.
3. Setelah review biasa, lab berjalan per kartu, berurutan: login semua role (satu browser per role, `playwright_<role>`), salin berkas uji, lalu giliran reviewer lab. Batas bawaan per sesi 300 sen / 20 menit; per run 1500 sen / 90 menit (disimpan di setelan daemon `lab`; belum ada layar untuk mengubahnya di fase 1).
4. Reviewer lab boleh menjalankan JavaScript di halaman (memperlambat atau menggagalkan request, membuat berkas lewat `DataTransfer`) dan mengunggah berkas dari staging. Ia tidak punya shell, tidak bisa membaca `.env` atau berkas kredensial, dan pencarian berkas memakai pola sederhana seperti `**/*.ts`.
5. AC yang tidak dilaporkan lab kembali ke hasil review biasa dengan catatan "The review lab did not report this criterion."

Tanpa perlengkapan yang disetujui, lab tidak berjalan dan chat run mencatat bahwa penyiapan diperlukan; AC calon lab tetap Can't verify.

## 4. Membaca hasil

Di tab **AC review**:

| Tanda | Arti |
|---|---|
| **Lab** | Verdict dicatat reviewer lab |
| **Simulated request** | Keadaan awal dibuat dengan mengubah request di halaman; tetap Pass penuh |
| **Prepared with SQL** | (fase 2) keadaan awal dibuat lewat SQL; tetap Pass penuh |
| Langkah bernomor, Prepared, screenshot | Bukti tiap AC: apa yang dilakukan, akun dan simulasi yang dipakai |
| Blocked by the review proxy | Request yang ditolak proxy. Request tulis yang diblokir membuat verdict Can't verify "needs prepared data" |
| Reviewer recommends | Untuk baris *Looks different*: **Approve** pada rekomendasi "Difference accepted" menerima perbedaan; **Reject** atau rekomendasi "follow the prototype" membiarkan baris menahan push |
| Findings outside AC | Masalah lain yang terlihat saat menguji; tidak mengubah verdict maupun gerbang |

Password akun uji dan nilai cookie sesi disamarkan (`[redacted]`) di semua teks bukti.

## 5. Lab dengan data uji (fase 2)

Lab data menyiapkan keadaan awal dengan mengubah data lokal (mis. memasang KB di aspek, mengubah batas di Corporate Config, menyiapkan corporate kedua), menguji AC, lalu memulihkan data. Hanya penyimpanan **lokal** milik app yang direview: database PostgreSQL di mesin ini dan folder (mis. folder unggahan). Staging bersama tidak pernah dipakai.

### 5.1 Menyiapkan skrip di repo

Buat folder skrip yang ter-commit di repo, mis. `review-fixtures/lab`. Saat disetujui, Mandor menyalin folder itu ke `~/.mandor/lab/<repo>/private/scripts/<digest>` dan semua perintah berjalan dari salinan itu; mengubah skrip di repo membatalkan persetujuan. Perintah adalah nama berkas di folder itu ditambah argumen sederhana, tanpa `..`, pipa, atau variabel shell.

Setiap penyimpanan punya empat perintah:

| Perintah | Tugas | Kontrak |
|---|---|---|
| Snapshot | Menyimpan isi penyimpanan ke `$MANDOR_SNAPSHOT_DIR` | Dijalankan saat app berhenti; hasil dikencangkan ke 0600 |
| Restore | Mengembalikan isi dari `$MANDOR_SNAPSHOT_DIR` | Harus bisa diulang |
| Fingerprint | Mencetak sidik isi ke stdout | Sama bila data sama; berubah bila data berubah. Mandor juga menyertakan daftar tabel, jadi perubahan skema terdeteksi |
| Test change | Mengubah sedikit data | Hanya dipakai saat persetujuan untuk membuktikan sidik dan restore bekerja |

Lingkungan perintah penyimpanan: `MANDOR_STORE`, `MANDOR_SNAPSHOT_DIR`, `MANDOR_STORE_PATH` (folder), dan untuk database `PGHOST`, `PGPORT`, `PGDATABASE`, `PGUSER`, `PGPASSWORD` dari role **pemeliharaan** (`PGSSLMODE=disable`). Variabel `PG*`, `DATABASE_URL`, dan `MANDOR_*` dari lingkungan daemon tidak diteruskan. Batas waktu tiap perintah 5 menit; keluaran dipotong 64 KiB dan password disamarkan.

**Seed** adalah skrip opsional untuk menyiapkan data yang lebih rumit (mis. memuat factory repo). Seed berjalan dengan role **lab**, menerima `MANDOR_SEED` dan `MANDOR_WORKTREE` (worktree run), dan tidak pernah menerima kredensial pemeliharaan. Selama seed berjalan, Mandor memeriksa `pg_stat_activity`; seed yang membuka koneksi dengan role lain ditolak.

### 5.2 Role database

| Role | Hak | Dipakai oleh |
|---|---|---|
| Lab (mis. `mandor_lab`) | Hanya `SELECT`, `INSERT`, `UPDATE`, `DELETE` pada tabel app. Bukan superuser, tanpa `CREATEDB`/`CREATEROLE`/`BYPASSRLS`, bukan anggota role lain, tidak memiliki objek, tanpa `CREATE` atau `TRIGGER` | Tool `lab_sql` reviewer dan seed |
| Pemeliharaan (mis. `postgres`) | Superuser atau anggota `pg_read_all_stats` | Snapshot, restore, sidik, dan pemeriksaan klien lain |

Contoh role lab untuk satu database:

```sql
CREATE ROLE mandor_lab LOGIN PASSWORD '...';
GRANT CONNECT ON DATABASE kelola TO mandor_lab;
GRANT USAGE ON SCHEMA public TO mandor_lab;
GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA public TO mandor_lab;
GRANT USAGE, SELECT ON ALL SEQUENCES IN SCHEMA public TO mandor_lab;
```

Server harus lokal: `inet_server_addr()` berupa loopback, socket Unix, atau alamat privat (database di Docker terlihat dengan IP kontainer). Password kedua role disimpan di `secrets.json` (0600) dan tidak pernah ditampilkan lagi.

### 5.3 Menyetujui

**Settings → Repos → Review lab → Lab with test data**: isi folder skrip, tambah penyimpanan (**Add database** atau **Add folder**) dan seed, isi password, **Save lab equipment**, lalu **Approve…**. Persetujuan menjalankan uji bolak-balik per penyimpanan (bisa beberapa menit; app harus berhenti):

1. sidik → snapshot → test change → sidik harus berubah → restore → sidik harus sama dengan awal;
2. memeriksa server lokal, hak role pemeliharaan, dan hak minimum role lab;
3. menjalankan setiap seed, lalu restore dan memastikan sidik kembali.

Kegagalan mana pun menolak persetujuan dengan alasan dalam bahasa Inggris.

### 5.4 Saat lab data berjalan

| Mesin | Kapan |
|---|---|
| Laptop | AC calon data ditunda dengan alasan "Lab with test data deferred". Tombol **Run lab (N)** di tab AC review memulai review baru dengan lab data diizinkan setelah konfirmasi |
| VM (lembur) | Otomatis setelah review biasa |

Urutan satu sesi: kunci per penyimpanan diambil (di `MANDOR_LAB_LOCK_DIR`, bawaan `~/.mandor/lab/locks`; di VM folder bersama mode 1777 agar daemon tiap dev saling menghormati) → app run dihentikan → sidik dan snapshot → app dijalankan lagi oleh run → reviewer lab boleh memakai `lab_sql` dan `lab_seed` → app dihentikan → restore → sidik dibandingkan → kunci dilepas → folder snapshot dihapus.

- `lab_sql` menjalankan satu pernyataan dengan role lab; maks. 1000 baris, 64 KiB, 2000 karakter per sel. Pernyataan dengan `$`, `\`, `E''`, `U&`, DDL, `COPY`, `SET ROLE`, dan sejenisnya ditolak. Verdict yang keadaannya disiapkan dengan SQL bertanda **Prepared with SQL**.
- Selama sesi berjalan, Start/Restart/Stop app menunggu sampai data pulih; banner "The review lab is using the local data" tampil di workspace.
- App yang menyala dari luar run (mis. `pnpm dev` di terminal) membuat lab tidak berjalan: "lab needs the app started from this run".

### 5.5 Restore gagal atau menunggu

| Banner | Arti dan tindakan |
|---|---|
| **Restoring the lab data is waiting** | Klien database lain masih terhubung. Hentikan app atau klien itu; Mandor mencoba lagi tiap 10 detik. Stop app tetap boleh |
| **The lab data was not restored** | Restore gagal atau sidik berbeda. Review, start app, dan giliran agent untuk repo itu ditahan. **Retry restore** menjalankan restore lagi dari snapshot. Bila Anda memulihkan data sendiri, **Data restored…** melepas penahanan dan menghapus snapshot |

Saat daemon start, sesi yang tertinggal (`snapshotting`, `running`, `restoring`, `restore_waiting`) dipulihkan otomatis dari snapshotnya. Restore gagal dan menunggu lebih dari 10 menit muncul di laporan pagi.

## 6. Risiko yang diketahui

| Risiko | Penanganan |
|---|---|
| Login app tidak berbentuk satu form | Perlengkapan tidak bisa dipakai; gunakan akun uji yang langsung masuk |
| Perubahan data lewat frame WebSocket atau GET yang punya efek samping | Tidak ditangkap proxy. Jangan setujui perlengkapan untuk app yang menulis lewat GET atau WebSocket |
| `CONNECT` ke origin HTTPS | Proxy tidak bisa melihat isi terowongan TLS; blokir tulis hanya berlaku untuk origin HTTP. App lokal biasanya HTTP |
| Terowongan ke database atau layanan jauh lewat loopback | Tidak terdeteksi; pastikan app lokal memakai database lokal |
| Memori: sampai 4 Chromium per sesi lab ditambah app | Batas 4 role; pantau memori WSL dan VM |
| Agent tidak jujur menandai `simulated` | Bukti menyimpan langkah dan data yang disiapkan; periksa sebelum push |
| Isi halaman dibaca model AI | Hanya akun uji dan data lokal; jangan memakai akun produksi |
| Skrip snapshot atau restore yang salah | Uji bolak-balik saat persetujuan; sidik dibandingkan setelah setiap lab. Sidik yang tidak mencakup sebagian data tidak bisa mendeteksi sisa di bagian itu |
| Trigger atau fungsi `SECURITY DEFINER` milik app | Bisa dipanggil role lab lewat DML biasa; role lab tidak boleh membuat atau memiliki objek |
| Data lab tertinggal di layanan luar (antrian, email, cache Redis) | Tidak di-snapshot; jangan setujui lab data untuk app yang menulis ke penyimpanan yang tidak terdaftar |

## 7. Masalah umum

| Gejala | Penyebab dan tindakan |
|---|---|
| "Playwright is not installed" | Pasang Playwright MCP (bagian 1) atau isi `MANDOR_PLAYWRIGHT_MCP` |
| "No test login is prepared for unattended review" | Siapkan dan setujui perlengkapan (bagian 2) |
| "Test login failed for admin: …" | Selector, URL, atau password salah; atau app belum menyala. Perbaiki perlengkapan, simpan, setujui ulang |
| Usulan gagal "The agent finished without a valid proposal" | Repo tidak memuat cukup petunjuk (seed, README). Isi form manual |
| Lab tidak berjalan setelah review | Perlengkapan belum disetujui, tidak ada calon lab, atau batas biaya/waktu run habis (lihat chat run) |
| Persetujuan gagal "did not change its fingerprint" | Sidik tidak mencerminkan data yang diubah test change; perbaiki skrip sidik |
| "the maintenance role of … must be a superuser or a member of pg_read_all_stats" | `GRANT pg_read_all_stats TO <role>` atau pakai superuser lokal |
| "lab only runs against local databases" | `PGHOST` mengarah ke server jauh; arahkan ke database lokal |
| "local data is locked by another lab" | Run lain (atau daemon dev lain di VM) sedang memakai penyimpanan yang sama; tunggu |
| Banner restore gagal tidak hilang | Retry restore, atau periksa data lalu Data restored… |
