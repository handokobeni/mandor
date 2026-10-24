# Progres: paket Linux native (tahap 2 service OS)

Plan: `docs/plans/2026-10-23-003-feat-linux-native-packages-plan.md`. Branch `feat/linux-native-packages`.

| Unit | Status | Catatan |
|---|---|---|
| U1 | selesai | `tauri.linux.conf.json` (resource `resources/mandord`, bundle `deb`/`rpm`/`appimage`); `build.rs` menolak build release Linux tanpa biner asli (dicek: pesan jelas); placeholder di CI desktop Linux dan README. |
| U2 | selesai | Job `paket Linux` di `ubuntu-22.04`: dependensi Tauri + `patchelf`, `libfuse2`, `rpm`; build tiga paket; hash daemon tertanam di app dan `mandord` di setiap paket dicek (`dpkg-deb -x`, `rpm2cpio`, `--appimage-extract`); `SHA256SUMS`; artifact `mandor-linux-packages`; publish melampirkannya. |
| U3 | kode selesai; spike A1 menunggu desktop Ubuntu | `linux_keep_awake_json`: GNOME → `gnome-session-inhibit --inhibit suspend:idle --app-id dev.mandor.app --reason …` + `cat`; desktop lain → `systemd-inhibit --what=idle --mode=block --who=Mandor` + `cat`; tanpa jenis `sleep`; program dicari di `/usr/bin` dan `/bin`. Test daemon baru: penjaga pembungkus dengan `cat` anak berhenti saat stdin ditutup. |
| U4 | selesai | `Runner` (WSL/lokal) menjalankan skrip probe/pasang yang sama; `provision_with` dengan hash dan batas waktu yang bisa diatur; `INSTALL_SCRIPT` menolak `~/.cache/mandor` berupa symlink; `LinkMode::LocalInstalled` memakai `~/.local/bin/mandord` dengan fallback PATH dan meneruskan `MANDOR_KEEP_AWAKE_CMD` (setsid); `ClientConfig::from_vars` (default Linux bila daemon dibawa); `provision_run` mengembalikan `runner`. |
| U5 | selesai | `backgroundView` membaca `runner`; teks Linux untuk service, linger, keep-awake; tombol "Remove service…" dengan konfirmasi (command `daemon_uninstall`, `uninstall` tanpa `--force`) dan pesan hasil. Dicek di `/?mock&linux`. |
| U6 | runbook selesai; uji langsung menunggu | `docs/runbook/setup-linux.md`. |

### Penyimpangan dari plan

- Daemon di paket Linux dibungkus header `MANDORD-RESOURCE\n` (17 byte). `linuxdeploy` menambahkan `RUNPATH $ORIGIN` ke setiap ELF di AppDir sehingga hash daemon di AppImage berubah (terlihat di run `release` 39970771819); `NO_STRIP` tidak mencegahnya. `build.rs` dan app melepas header sebelum menghitung hash dan memasang; Windows tetap membawa biner mentah. Test: `resource_berheader_dilepas_sebelum_dipasang`.

- KTD-L2 menyebut hash dan berkas sementara ditangani di Rust untuk pelaksana lokal. Implementasi memakai skrip probe dan pasang yang sama dengan WSL (`sha256sum`, `set -C`, direktori dan berkas `0700`), dijalankan langsung lewat `/bin/sh` dengan `HOME` pengguna. Keputusannya identik dan satu skrip teruji di dua jalur.

## Uji langsung (menunggu desktop Ubuntu 24.04 GNOME)

Belum dijalankan: spike A1 (run kontrol lalu kedua kandidat penahan), AE1–AE4, pemasangan `.rpm` dan AppImage.

## Run `release` (2026-10-23)

Run 39972532129 di `d60951f` hijau: `.deb` 14 MB, `.rpm` 14 MB, AppImage 91 MB (membawa WebKitGTK sendiri), `SHA256SUMS`; hash daemon tertanam di app dan resource di ketiga paket diperiksa job. Kegagalan sebelumnya: `rpm2cpio | cpio` gagal tanpa pesan (diganti `rpm -qp --dump`), dan AppImage mengubah daemon (header di atas).
