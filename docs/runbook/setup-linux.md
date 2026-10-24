# Runbook: Mandor di Linux

> Cara menjalankan app untuk pengembangan: lihat [`README.md`](../../README.md).

Daemon `mandord`, git, gh, dan Claude Code berjalan langsung di Linux. Satu paket
membawa UI dan daemon dengan versi yang sama; app memasang dan memperbarui
daemon sendiri sebagai service systemd user, sehingga agent tetap bekerja saat
jendela ditutup, app di-Quit, atau app crash.

Didukung: distro dengan systemd (Ubuntu 22.04+, Fedora 39+), desktop GNOME atau
KDE, x86_64.

## 1. Unduh dan periksa paket

Ambil dari GitHub Release (tag `v*`) atau artifact **mandor-linux-packages** run
workflow `release`. Simpan paket dan `SHA256SUMS` di satu folder, lalu:

```bash
sha256sum -c SHA256SUMS --ignore-missing
```

Paket belum ditandatangani; periksa checksum sebelum memasang.

## 2. Pasang

```bash
# Ubuntu / Debian
sudo apt install ./mandor_<versi>_amd64.deb

# Fedora / RHEL / openSUSE
sudo dnf install ./mandor-<versi>-1.x86_64.rpm

# Distro lain: AppImage
chmod +x mandor_<versi>_amd64.AppImage
./mandor_<versi>_amd64.AppImage
```

AppImage butuh FUSE 2 (`libfuse2`). Tanpa FUSE: `./mandor_<versi>_amd64.AppImage --appimage-extract-and-run`.

## 3. Siapkan akun dan service (sekali)

```bash
gh auth login     # kredensial git/gh untuk push dan PR oleh daemon
claude            # login langganan Claude Code
git config --global user.name "Nama"; git config --global user.email "email"

# Service tetap hidup setelah logout (lembur tanpa sesi)
sudo loginctl enable-linger "$USER"
```

Buka Mandor. Saat pertama kali, app memeriksa hash daemon yang dibawa paket,
memasangnya ke `~/.local/bin/mandord`, mendaftarkan
`~/.config/systemd/user/mandord.service`, lalu tersambung.

## 4. Siap ditinggal lembur

Buka **Settings → Background work**. Lembur aman bila ketiga baris "Ready":

| Baris | Arti |
|---|---|
| Daemon runs as a service | systemd mengelola daemon dan menyalakannya lagi bila crash. |
| Linger is on | Service hidup walau Anda logout. |
| Keep-awake ready | Selama agent, review, atau publish bekerja, daemon menahan suspend karena idle (bukan layar), termasuk setelah app di-Quit, dan melepasnya lima menit setelah selesai. Di GNOME lewat `gnome-session-inhibit`, di desktop lain lewat `systemd-inhibit --what=idle`. Suspend yang Anda pilih sendiri tetap berjalan. |

Pengaturan daya: komputer dicolok; di laptop, atur "lid close" agar tidak
suspend saat dicolok (GNOME: `gsettings set org.gnome.settings-daemon.plugins.power lid-close-ac-action 'nothing'`). Mandor tidak menangani lid atau baterai.

## 5. Pembaruan

Pasang paket versi baru, lalu buka app. Bila ada agent yang bekerja, daemon
tidak diganti: Background work menampilkan "Update waiting for running agents"
dan app mencoba lagi saat semua selesai.

## 6. Melepas

Skrip paket tidak bisa membersihkan service milik pengguna, jadi lakukan
berurutan:

1. **Settings → Background work → Remove service… → Remove.** Service berhenti,
   unit dan `~/.local/bin/mandord` dihapus. Bila ada agent bekerja, tunggu
   sampai selesai lalu ulangi.
2. Quit Mandor (membuka app lagi memasang ulang service).
3. Hapus paket: `sudo apt remove mandor` atau `sudo dnf remove mandor`, atau
   hapus berkas AppImage.

Tanpa app: `~/.local/bin/mandord uninstall` (ditunda bila ada pekerjaan
berjalan) atau `~/.local/bin/mandord uninstall --force`. Data di `~/.mandor`
tidak dihapus; hapus manual bila tidak dipakai lagi.

## Pemecahan masalah

| Gejala | Langkah |
|---|---|
| "Daemon unreachable" | `~/.local/bin/mandord token` harus mencetak token; `systemctl --user status mandord`. |
| "Daemon setup failed: the bundled daemon does not match this app" | Paket rusak atau diubah; periksa `SHA256SUMS` dan pasang ulang. |
| Service gagal start berulang | `journalctl --user -u mandord -n 50`; biasanya daemon lain masih memegang `~/.mandor/daemon.lock` (`pgrep -a mandord`). |
| Keep-awake "not set up" | Program penahan tidak ada: pasang `gnome-session-bin` (GNOME) atau pastikan `systemd-inhibit` ada. Buka app lagi. |
| Keep-awake "failing" | `journalctl --user -u mandord -n 50`. Coba manual: `gnome-session-inhibit --list` (GNOME) atau `systemd-inhibit --list` saat agent bekerja. |
| Komputer tetap suspend saat agent bekerja | Lihat baris keep-awake; pastikan desktop GNOME/KDE yang didukung, dan bahwa suspend bukan karena lid atau baterai. |
| Service berhenti setelah logout | Linger mati: `sudo loginctl enable-linger "$USER"`. |
| Versi glibc terlalu lama | Paket dibangun di Ubuntu 22.04; distro yang lebih tua tidak didukung. |
