# Runbook: Lark Custom App, Base uji, dan profil gh

> Cara menjalankan app untuk pengembangan: lihat [`README.md`](../../README.md).

Panduan ini menyiapkan mandor agar membaca dan menulis Sprint Tasks atas nama
setiap dev (OAuth per user, KTD-5). Secret app disimpan daemon di
`~/.mandor/lark.json` (izin 0600); token user di `~/.mandor/secrets/lark-token.json`
(0600). Jangan menempelkan App ID/Secret ke chat atau issue.

## 1. Buat Custom App

1. Buka Lark Developer Console (`https://open.larksuite.com/app`) → **Create
   Custom App**. Beri nama, mis. `mandor`.
2. **Credentials & Basic Info**: catat **App ID** dan **App Secret**.
3. **Security Settings → Redirect URLs**: tambahkan
   `http://127.0.0.1:53682/callback`. Port ini tetap; bila diganti, samakan dengan
   "Callback port" di Settings mandor.
4. **Permissions & Scopes** (user token):
   - `bitable:app`: baca/tulis record dan field Base.
   - Baca komentar dokumen: `docs:document.comment:read` (bila tidak tersedia di
     tenant Anda, pakai `drive:drive:readonly`).
   - `contact:contact.base:readonly` dan `contact:user.base:readonly`: nama pengguna untuk kartu dan komentar (endpoint `users/batch` butuh keduanya)
     (tanpa scope ini, mandor menampilkan open_id).
   - `offline_access`: refresh token, agar tidak perlu login ulang tiap 2 jam.
5. **Version Management & Release**: buat versi dan ajukan ke admin tenant.
   Scope user token berlaku setelah versi disetujui.

App bertindak sebagai **user yang login**, jadi tidak perlu menambahkan bot ke
Base; akses mengikuti izin dev itu sendiri.

## 2. Siapkan Base

Untuk uji coba, salin Base Sprint Tasks (**… → Duplicate**) beserta tabel
Acceptance Criteria. Catat dari URL Base:

- `app_token`: segmen setelah `/base/` (mis. `FNFlbS3jPa3Yq4sjm8Illj18gnb`).
- ID tabel Sprint Tasks dan Acceptance Criteria: parameter `table=tbl…` saat
  tabel dibuka.

Saat login pertama, mandor membuat field milik app bila belum ada (idempoten):
`mandor_branch`, `mandor_pr`, `mandor_owner`, `mandor_comment_cursor`,
`mandor_modified` (ModifiedTime, untuk polling inkremental), dan `Fix Notes`.
Field `kanban_column`, `repo_id`, dan `order_within_column` yang sudah ada dipakai
sebagai cermin tampilan. Field milik manusia (judul, deskripsi, AC, assignee) tidak
pernah ditulis app.

Bila akun Anda tidak boleh menambah field, minta pemilik Base membuat field di atas
dengan nama dan tipe persis sama (Text, kecuali `mandor_pr` = URL dan
`mandor_modified` = Modified time).

## 3. Isi pengaturan di mandor

1. Buka app → **Settings → Lark**.
2. Isi App ID, App Secret, `app_token`, ID tabel Sprint Tasks, ID tabel Acceptance
   Criteria. Simpan.
3. Tekan **Log in with Lark**. Browser terbuka; setujui akses. Setelah halaman
   "Logged in to Lark", kembali ke app; papan memuat task yang di-assign ke Anda.

Bila port callback dipakai aplikasi lain, app menampilkan pesan "callback port … is already
in use". Tutup aplikasi itu atau ganti port di Lark Console dan di
pengaturan.

Bila status menunjukkan "Lark session expired", refresh token sudah tidak
berlaku: login ulang.

## 4. Repo dan profil gh

1. Login `gh` di mesin tempat daemon berjalan (WSL bila memakai Windows):
   `gh auth login`. Untuk beberapa akun, beri nama profil dan isi
   **gh profile** di pengaturan repo.
2. **Settings → Repo → Add**: path repo di mesin daemon. Default branch
   dibaca dari `origin/HEAD`; bila gagal, jalankan
   `git remote set-head origin --auto` di repo itu.
3. Opsional per repo:
   - **Trusted reviewers/QA** (open_id): komentar mereka diteruskan ke agent
     saat Waiting Fix. Assignee dan reporter selalu tepercaya.
   - **Read-only companion paths**: repo lain yang boleh dibaca agent (read-only).
   - **Extra agent allowlist**: pola tool untuk agent, mis. `Bash(just *)`.
4. Pastikan branch `main` dilindungi ruleset GitHub. Daemon hanya push ke branch
   task, tetapi ruleset adalah lapisan pengaman terakhir.

## 5. Claude Code

Login sekali dengan langganan Anda (bukan API key) di mesin daemon: jalankan
`claude` lalu ikuti login. mandor menjalankan `claude -p` tanpa modifikasi, tanpa
`bypassPermissions`, dengan hook `mandord hook` yang menolak push, `gh`, akses
kredensial, dan perubahan konfigurasi agent. Daemon membuang `ANTHROPIC_API_KEY`
dari environment agent agar selalu memakai login langganan.

## Pemecahan masalah

| Gejala | Penyebab umum |
|---|---|
| Papan kosong setelah login | Task belum di-assign ke akun Anda, atau `mandor_modified` belum ada (poll penuh tiap 5 menit tetap memuatnya). |
| Kartu "sync failed" | Lihat detail di kartu; tekan **Retry** setelah penyebab diperbaiki. Tulisan selalu divalidasi ulang. |
| Kartu "overwritten by Lark" | Status di Lark berubah sebelum tulisan app; Lark menang. |
| Komentar Waiting Fix tidak terbaca | API komentar ditolak (scope). Isi field **Fix Notes** sebagai cadangan. |
| Log daemon | `~/.mandor/mandord.log` (JSON). Token tidak pernah dicatat. |
