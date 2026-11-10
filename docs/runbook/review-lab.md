# Runbook: review lab (fase 1)

Review lab menguji AC yang tidak bisa dibuktikan review biasa: AC yang butuh akun role lain, kegagalan request yang disimulasikan, atau berkas uji. Lab berjalan otomatis setelah review biasa, per kartu, dengan akun uji yang sudah Anda setujui. Fase 1 tidak mengubah data: setiap request tulis diblokir. Rencana: `docs/plans/2026-11-10-001-feat-review-lab-plan.md`.

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
2. AC yang dinilai Can't verify dengan kebutuhan lab (`role`, `simulation`, `file`) menjadi calon lab. AC yang butuh data (`data`) menunggu fase 2.
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

## 5. Risiko yang diketahui

| Risiko | Penanganan |
|---|---|
| Login app tidak berbentuk satu form | Perlengkapan tidak bisa dipakai; gunakan akun uji yang langsung masuk |
| Perubahan data lewat frame WebSocket atau GET yang punya efek samping | Tidak ditangkap proxy. Jangan setujui perlengkapan untuk app yang menulis lewat GET atau WebSocket |
| `CONNECT` ke origin HTTPS | Proxy tidak bisa melihat isi terowongan TLS; blokir tulis hanya berlaku untuk origin HTTP. App lokal biasanya HTTP |
| Terowongan ke database atau layanan jauh lewat loopback | Tidak terdeteksi; pastikan app lokal memakai database lokal |
| Memori: sampai 4 Chromium per sesi lab ditambah app | Batas 4 role; pantau memori WSL dan VM |
| Agent tidak jujur menandai `simulated` | Bukti menyimpan langkah dan data yang disiapkan; periksa sebelum push |
| Isi halaman dibaca model AI | Hanya akun uji dan data lokal; jangan memakai akun produksi |

## 6. Masalah umum

| Gejala | Penyebab dan tindakan |
|---|---|
| "Playwright is not installed" | Pasang Playwright MCP (bagian 1) atau isi `MANDOR_PLAYWRIGHT_MCP` |
| "No test login is prepared for unattended review" | Siapkan dan setujui perlengkapan (bagian 2) |
| "Test login failed for admin: …" | Selector, URL, atau password salah; atau app belum menyala. Perbaiki perlengkapan, simpan, setujui ulang |
| Usulan gagal "The agent finished without a valid proposal" | Repo tidak memuat cukup petunjuk (seed, README). Isi form manual |
| Lab tidak berjalan setelah review | Perlengkapan belum disetujui, tidak ada calon lab, atau batas biaya/waktu run habis (lihat chat run) |
