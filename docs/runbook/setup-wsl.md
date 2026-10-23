# Runbook: Mandor di Windows dengan WSL

> Cara menjalankan app untuk pengembangan: lihat [`README.md`](../../README.md).

Logika berjalan di WSL (daemon `mandord`, git, gh, Claude Code), UI native di
Windows. Satu installer Windows membawa UI dan daemon dengan versi yang sama;
app memasang dan memperbarui daemon di WSL sendiri. Daemon berjalan sebagai
service systemd user, sehingga agent tetap bekerja saat jendela ditutup, app
di-Quit, atau app crash.

## 1. Siapkan WSL (sekali)

Pakai WSL 2 dari Microsoft Store dan distro Ubuntu. Lembur setelah app di-Quit
membutuhkan Windows 11.

```bash
# systemd (butuh sudo sekali)
printf '[boot]\nsystemd=true\n' | sudo tee -a /etc/wsl.conf
```

Lalu dari PowerShell: `wsl --shutdown`, dan buka WSL lagi. Periksa dengan
`systemctl --user status` (tanpa error).

```bash
# Service tetap hidup setelah sesi WSL terakhir berakhir
sudo loginctl enable-linger "$USER"

# Login alat yang dipakai daemon dan agent, di dalam WSL
gh auth login     # kredensial git/gh tersimpan di WSL, bukan Windows
claude            # login langganan Claude Code
git config --global user.name "Nama"; git config --global user.email "email"
```

Daemon tidak memakai ssh-agent sesi atau Git Credential Manager Windows: push
dan pembuatan PR berjalan di bawah systemd tanpa sesi terminal. `gh auth login`
dengan protokol HTTPS sudah cukup.

Repo yang dikerjakan harus berada di sistem berkas WSL (mis. `~/work/app`), bukan
`/mnt/c/...`, agar git dan agent cepat dan izin berkas benar.

## 2. Pasang app di Windows

1. Jalankan installer `mandor_<versi>_x64-setup.exe`. Installer dibuat workflow
   `release`: lampiran GitHub Release untuk tag `v*`, atau artifact
   **mandor-windows-installer** dari run manual (Actions → release → Run
   workflow). Installer belum ditandatangani, jadi SmartScreen menampilkan
   peringatan: pilih **More info → Run anyway**.
2. Bila distro bukan default, set variabel environment pengguna
   `MANDOR_LINK=wsl:Ubuntu-24.04` (nama dari `wsl -l -v`).
3. Buka app. Saat pertama kali, app:
   - memeriksa hash biner daemon yang dibawa installer;
   - menyalinnya ke WSL dan menjalankan `mandord install` (biner di
     `~/.local/bin/mandord`, unit `~/.config/systemd/user/mandord.service`);
   - menyalin penjaga keep-awake ke `%LOCALAPPDATA%\Mandor\keepawake\<versi>`;
   - lalu tersambung.

Bila systemd belum aktif, app tetap tersambung dengan cara lama (daemon
dijalankan terlepas) dan Settings → Background work menampilkan langkah
mengaktifkan systemd. Setelah systemd aktif, buka app lagi: service terpasang.

## 3. Siap ditinggal lembur

Buka **Settings → Background work**. Lembur aman bila keempat baris "Ready":

| Baris | Arti |
|---|---|
| Daemon runs as a service | systemd mengelola daemon dan menyalakannya lagi bila crash. |
| Linger is on | Service hidup walau tidak ada sesi WSL. |
| WSL stays on without Mandor | `.wslconfig` berisi `instanceIdleTimeout=-1` dan `vmIdleTimeout=-1`. Tekan **Keep WSL on** (mengubah setelan semua distro dan menahan memori VM; Mandor membuat cadangan `.wslconfig.mandor-bak` dulu), lalu `wsl --shutdown` saat tidak ada agent yang bekerja. **Turn off** menghapus hanya dua kunci itu. |
| Keep-awake ready | Selama ada agent, review, atau publish yang bekerja, daemon menjalankan penjaga yang menahan sleep (bukan layar), termasuk setelah app di-Quit, dan melepasnya lima menit setelah selesai. |

Di Windows 10, `vmIdleTimeout` tidak berlaku: biarkan Mandor di tray (jangan
Quit) saat lembur.

Atur juga daya Windows: laptop dicolok, dan **Control Panel → Power Options →
Choose what closing the lid does → When I close the lid: Do nothing** (saat
dicolok). Mandor tidak menangani lid ditutup atau baterai habis.

## 4. Pembaruan

Pasang installer versi baru lalu buka app. Bila ada agent yang bekerja, daemon
tidak diganti: Background work menampilkan "Update waiting for running agents"
dan app mencoba lagi saat semua selesai. App dev yang sedang menyala berhenti
saat daemon diperbarui. Selama pembaruan tertunda dan daemon lama belum bisa
menahan sleep, biarkan Mandor terbuka.

## 5. Melepas

Uninstall **Mandor** dari Windows (Settings → Apps). Sebelum berkas app dihapus,
uninstaller menghapus dua kunci keep-alive dari `.wslconfig` dan menjalankan
`mandord uninstall` di WSL (service dihentikan, unit dan biner dihapus). Bila ada
agent yang bekerja, daemon dibiarkan terpasang dan uninstaller menampilkan
langkahnya. Data di `~/.mandor` (database, login Lark) tidak dihapus.

Pembersihan manual bila perlu:

```bash
~/.local/bin/mandord uninstall          # ditunda bila ada pekerjaan berjalan
~/.local/bin/mandord uninstall --force  # tetap melepas
rm -rf ~/.mandor                        # data Mandor, bila tidak dipakai lagi
sudo loginctl disable-linger "$USER"    # bila linger tidak dipakai hal lain
```

Di Windows: hapus `instanceIdleTimeout` dan `vmIdleTimeout` dari
`%UserProfile%\.wslconfig` (cadangan di `.wslconfig.mandor-bak`), dan folder
`%LOCALAPPDATA%\Mandor`.

## 6. Tanpa installer (build sendiri)

```bash
git clone <repo mandor> ~/src/mandor && cd ~/src/mandor
cargo build --release --locked --bin mandord
./target/release/mandord install          # atau: install --check
```

`mandord install` mencetak satu baris JSON:

| `result` | Arti |
|---|---|
| `installed` | Pemasangan pertama; service menyala. |
| `updated` | Biner diganti dengan versi ini; daemon lama di-restart. |
| `service_only` | Biner sudah sama; hanya unit yang ditulis ulang. |
| `up_to_date` | Tidak ada yang berubah. Peringatan "newer" berarti daemon terpasang lebih baru dan tidak diturunkan. |
| `deferred` | Ada agent, review, atau publish yang berjalan; tidak ada yang diubah. |
| `error` | Lihat `message`; daemon kembali menerima pekerjaan. |

Bila sebelumnya daemon dijalankan manual atau lewat `connect` (setsid),
`install` menghentikannya setelah memastikan prosesnya `mandord`, lalu
menyalakan service. `MANDOR_HOME` selain `~/.mandor` tidak memakai service.

## Pemecahan masalah

| Gejala | Langkah |
|---|---|
| "Daemon unreachable" | `wsl -- sh -lc '~/.local/bin/mandord token'` harus mencetak token. |
| "Daemon setup failed: the bundled daemon does not match this app" | Berkas app rusak atau diubah; pasang ulang installer. |
| "Could not check the daemon" | WSL lambat start atau tidak tersedia; buka `wsl` sekali lalu buka app lagi. |
| Keep-awake "failing" | Penjaga di `%LOCALAPPDATA%\Mandor\keepawake` tidak bisa dijalankan dari WSL. Cek interop: `cmd.exe /c ver` dari WSL harus berhasil. Buka app lagi untuk menyalin ulang penjaga. |
| `powercfg /requests` tidak menampilkan Mandor saat agent bekerja | Lihat baris keep-awake di Background work dan `journalctl --user -u mandord -n 50`. |
| "Version mismatch" | Versi app dan mandord berbeda mayor; pasang installer terbaru. |
| Login Lark tidak kembali ke app | Browser Windows memanggil `127.0.0.1:53682`; pastikan port tidak dipakai aplikasi lain. |
| Service gagal start berulang | `journalctl --user -u mandord -n 50`; biasanya daemon lain masih memegang `~/.mandor/daemon.lock` (`pgrep -a mandord`). |
| Agent berhenti saat laptop tidur | Keep-awake tidak aktif atau lid ditutup; run ditandai gagal saat daemon start dan bisa dilanjutkan (Resume). |
