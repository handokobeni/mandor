# Progres: paket Linux native (tahap 2 service OS)

Plan: `docs/plans/2026-10-23-003-feat-linux-native-packages-plan.md`. Branch `feat/linux-native-packages`.

| Unit | Status | Catatan |
|---|---|---|
| U1 | selesai | `tauri.linux.conf.json` (resource `resources/mandord`, bundle `deb`/`rpm`/`appimage`); `build.rs` menolak build release Linux tanpa biner asli (dicek: pesan jelas); placeholder di CI desktop Linux dan README. |
| U2 | selesai, menunggu run `release` | Job `paket Linux` di `ubuntu-22.04`: dependensi Tauri + `patchelf`, `libfuse2`, `rpm`; build tiga paket; hash daemon tertanam di app dan `mandord` di setiap paket dicek (`dpkg-deb -x`, `rpm2cpio`, `--appimage-extract`); `SHA256SUMS`; artifact `mandor-linux-packages`; publish melampirkannya. |
| U3 | kode selesai; spike A1 menunggu desktop Ubuntu | `linux_keep_awake_json`: GNOME → `gnome-session-inhibit --inhibit suspend:idle --app-id dev.mandor.app --reason …` + `cat`; desktop lain → `systemd-inhibit --what=idle --mode=block --who=Mandor` + `cat`; tanpa jenis `sleep`; program dicari di `/usr/bin` dan `/bin`. Test daemon baru: penjaga pembungkus dengan `cat` anak berhenti saat stdin ditutup. |
| U4 | selesai | `Runner` (WSL/lokal) menjalankan skrip probe/pasang yang sama; `provision_with` dengan hash dan batas waktu yang bisa diatur; `INSTALL_SCRIPT` menolak `~/.cache/mandor` berupa symlink; `LinkMode::LocalInstalled` memakai `~/.local/bin/mandord` dengan fallback PATH dan meneruskan `MANDOR_KEEP_AWAKE_CMD` (setsid); `ClientConfig::from_vars` (default Linux bila daemon dibawa); `provision_run` mengembalikan `runner`. |
| U5 | selesai | `backgroundView` membaca `runner`; teks Linux untuk service, linger, keep-awake; tombol "Remove service…" dengan konfirmasi (command `daemon_uninstall`, `uninstall` tanpa `--force`) dan pesan hasil. Dicek di `/?mock&linux`. |
| U6 | runbook selesai; uji langsung menunggu | `docs/runbook/setup-linux.md`. |

### Penyimpangan dari plan

- KTD-L2 menyebut hash dan berkas sementara ditangani di Rust untuk pelaksana lokal. Implementasi memakai skrip probe dan pasang yang sama dengan WSL (`sha256sum`, `set -C`, direktori dan berkas `0700`), dijalankan langsung lewat `/bin/sh` dengan `HOME` pengguna. Keputusannya identik dan satu skrip teruji di dua jalur.

## Uji langsung (menunggu desktop Ubuntu 24.04 GNOME)

Belum dijalankan: spike A1 (run kontrol lalu kedua kandidat penahan), AE1–AE4, pemasangan `.rpm` dan AppImage.
