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

# Pasang daemon (pilih salah satu):
# a) Dari GitHub Release (tag v*): unduh mandord-linux-x86_64.tar.gz, lalu
mkdir -p ~/.local/bin && tar -xzf mandord-linux-x86_64.tar.gz -C ~/.local/bin
# b) Bangun sendiri
git clone <repo mandor> ~/src/mandor && cd ~/src/mandor
cargo build --release --locked --bin mandord
install -m 0755 target/release/mandord ~/.local/bin/mandord

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
   lewat shell login;
   daemon dijalankan terlepas bila belum ada.

## 3. Daemon tetap hidup saat UI ditutup

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

### Opsional: systemd user service

Bila systemd aktif di distro (`/etc/wsl.conf` berisi `[boot] systemd=true`):

```ini
# ~/.config/systemd/user/mandord.service
[Unit]
Description=mandord

[Service]
ExecStart=%h/.local/bin/mandord serve
Restart=on-failure

[Install]
WantedBy=default.target
```

```bash
systemctl --user enable --now mandord
loginctl enable-linger "$USER"
```

## 4. Verifikasi

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
