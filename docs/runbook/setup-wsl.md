# Runbook: Mandor di Windows dengan WSL

> Cara menjalankan app untuk pengembangan: lihat [`README.md`](../../README.md).

Logika berjalan di WSL (daemon `mandord`, git, gh, Claude Code), UI native di
Windows (R16, KTD-2, KTD-18). UI bicara ke daemon lewat relay stdio
`wsl.exe -d <distro> -- mandord connect`; tidak ada port yang dibuka ke Windows.

## 1. Di dalam WSL

```bash
# Toolchain dan dependensi
sudo apt-get update && sudo apt-get install -y git build-essential pkg-config
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Aktifkan systemd di distro (sekali; butuh sudo), lalu dari PowerShell
# jalankan `wsl --shutdown` dan buka WSL lagi.
printf '[boot]\nsystemd=true\n' | sudo tee -a /etc/wsl.conf

# Ambil biner daemon (pilih salah satu):
# a) Dari GitHub Release (tag v*): unduh mandord-linux-x86_64.tar.gz, lalu
mkdir -p ~/tmp/mandord && tar -xzf mandord-linux-x86_64.tar.gz -C ~/tmp/mandord
# b) Bangun sendiri
git clone <repo mandor> ~/src/mandor && cd ~/src/mandor
cargo build --release --locked --bin mandord

# Pasang sebagai service systemd user: menyalin biner ke ~/.local/bin,
# menulis ~/.config/systemd/user/mandord.service, menyalakannya, dan
# mencoba mengaktifkan linger. Jalankan dari biner hasil langkah di atas:
~/tmp/mandord/mandord install          # atau: ./target/release/mandord install

# Login alat yang dipakai daemon dan agent
gh auth login
claude            # login langganan Claude Code
```

Pastikan `~/.local/bin` ada di `PATH` shell login (`~/.profile`; Ubuntu
menambahkannya otomatis bila direktori ada). Relay menjalankan
`wsl.exe -- sh -lc 'exec mandord connect'`, jadi hanya `~/.profile` yang dibaca,
bukan `~/.bashrc`.

Repo yang dikerjakan harus berada di sistem berkas WSL (mis. `~/work/app`), bukan
`/mnt/c/...`, agar git dan agent cepat dan izin berkas benar.

## 2. Di Windows

1. Pasang app mandor dengan installer Windows (`mandor_<versi>_x64-setup.exe`).
   Installer dibuat workflow `release` di GitHub Actions: lampiran GitHub Release
   untuk tag `v*`, atau artifact **mandor-windows-installer** dari run manual
   (Actions → release → Run workflow). Installer belum ditandatangani, jadi
   Windows SmartScreen menampilkan peringatan: pilih **More info → Run anyway**.
2. Bila distro bukan default, set variabel environment pengguna
   `MANDOR_LINK=wsl:Ubuntu-24.04` (nama dari `wsl -l -v`). Default: distro WSL
   default.
3. Jalankan app. Ia memanggil `mandord token` lalu `mandord connect` di WSL
   lewat shell login. Bila daemon belum berjalan, `connect` menyalakannya lewat
   service (atau terlepas, bila service tidak terpasang).

## 3. Daemon sebagai service

`mandord install` mencetak satu baris JSON. Hasil yang mungkin:

| `result` | Arti |
|---|---|
| `installed` | Pemasangan pertama; service menyala. |
| `updated` | Biner diganti dengan versi ini; daemon lama di-restart. |
| `service_only` | Biner sudah sama; hanya unit yang ditulis ulang. |
| `up_to_date` | Tidak ada yang berubah. Bila ada peringatan "newer", daemon terpasang lebih baru dan tidak diturunkan. |
| `deferred` | Ada agent, review, atau publish yang sedang berjalan; tidak ada yang diubah. Jalankan lagi setelah selesai. |
| `error` | Lihat `message`; bila terjadi setelah daemon bersiap update, daemon kembali menerima pekerjaan. |

`"service":"none"` berarti systemd user tidak tersedia: biner tetap disalin dan
app memakai cara lama (daemon dijalankan terlepas oleh `mandord connect`).
`"linger":false` dengan peringatan berarti service berhenti saat sesi terakhir
Anda di WSL berakhir; aktifkan dengan `sudo loginctl enable-linger "$USER"`.

Memeriksa tanpa mengubah apa pun:

```bash
mandord install --check     # binary, service, systemd, linger, running
systemctl --user status mandord
journalctl --user -u mandord -n 50
```

Memperbarui: jalankan `install` dari biner versi baru. Bila ada pekerjaan yang
sedang berjalan hasilnya `deferred`; pembaruan tidak pernah memutus agent.
App dev yang sedang menyala ikut berhenti saat daemon di-restart.

Bila sebelumnya daemon dijalankan manual atau lewat `connect` (setsid), `install`
menghentikannya setelah memastikan prosesnya memang `mandord`, lalu menyalakan
service. Unit yang ditulis manual dengan nama yang sama ditimpa.

## 4. Daemon tetap hidup saat UI ditutup

Dengan service, daemon dan agent tetap berjalan saat jendela ditutup, app
di-Quit, atau app crash, dan systemd menyalakan daemon lagi bila mati.

Menutup jendela hanya menyembunyikannya ke system tray. App menjaga satu proses
`wsl.exe -- sleep infinity` agar VM WSL tidak berhenti selama app di tray.

Bila app benar-benar keluar (menu tray **Quit**), WSL bisa berhenti setelah
`vmIdleTimeout` dan daemon ikut berhenti. Agar agent tetap berjalan lebih lama,
atur di `%UserProfile%\.wslconfig`:

```ini
[wsl2]
vmIdleTimeout=-1
```

lalu `wsl --shutdown` sekali.

## 5. Melepas

```bash
mandord uninstall           # ditunda (deferred) bila ada pekerjaan berjalan
mandord uninstall --force   # tetap melepas walau ada pekerjaan berjalan
```

`uninstall` menghentikan dan menonaktifkan service, menghapus unit dan
`~/.local/bin/mandord`. Data Anda di `~/.mandor` (database, setelan dan login
Lark) tidak dihapus; hapus manual bila memang tidak dipakai lagi. Linger tidak
dimatikan; matikan dengan `sudo loginctl disable-linger "$USER"` bila perlu.
Bila Anda menambahkan kunci di `.wslconfig`, hapus juga kuncinya.

## 6. Verifikasi

1. Buka app → status bar menampilkan versi mandord.
2. Jalankan sebuah task, tutup jendela, tunggu lebih lama dari `vmIdleTimeout`
   default, buka lagi dari tray: agent masih berjalan dan chat berlanjut.
3. `wsl -- pgrep -a mandord` menunjukkan satu daemon.

## Pemecahan masalah

| Gejala | Langkah |
|---|---|
| "Daemon unreachable" | `wsl -- sh -lc 'mandord token'` harus mencetak token. Bila "not found", perbaiki `PATH` di `~/.profile`. |
| "Version mismatch" | Versi app dan mandord berbeda mayor; perbarui keduanya. |
| Login Lark tidak kembali ke app | Browser Windows memanggil `127.0.0.1:53682`; pastikan port tidak dipakai aplikasi Windows lain. |
| Agent berhenti saat laptop tidur | Perilaku normal WSL; run ditandai gagal saat daemon start dan bisa dilanjutkan. |
| `install` selalu `deferred` | Ada run yang bekerja; tunggu selesai. Bila daemon tidak menjawab tapi lock dipegang, cek `systemctl --user status mandord` dan log di `~/.mandor/mandord.log`. |
| `"service":"none"` padahal systemd sudah diaktifkan | `systemctl --user show-environment` harus berhasil di shell yang sama. Setelah mengubah `/etc/wsl.conf`, jalankan `wsl --shutdown` dari Windows. |
| `error`: "not a regular file/directory" atau "not owned by you" | `~/.local/bin/mandord` atau direktorinya berupa symlink atau milik user lain; perbaiki lalu jalankan `install` lagi. |
| Service gagal start berulang | `journalctl --user -u mandord -n 50`; biasanya daemon lain masih memegang `~/.mandor/daemon.lock` (`pgrep -a mandord`). |
