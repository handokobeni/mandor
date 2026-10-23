# Progres: daemon sebagai service OS

Plan: `docs/plans/2026-10-23-001-feat-daemon-os-service-installer-plan.md`. U1–U4: PR #9. U5–U9: PR #10 (bercabang dari PR #9).

| Unit | Status | Catatan |
|---|---|---|
| U1 | selesai | `RunState::is_working` dan `any_working` (termasuk Preparing dan Publishing); `status` menambah `busy`, `managed_by`, `keep_awake` (opsional, daemon lama tidak mengirimnya); `--version` mencetak `mandord <versi>`. |
| U2 | selesai | `DrainGate` di `mandor-usecase` (std Mutex, tidak melewati `.await`; penghitung `in_flight`), dipegang `App` dan dibagi ke `RunService`; gerbang di semua titik mulai kerja (start, pesan, approve/continue plan, resume, takeover, publish, review, fix, `spawn_turn`); RPC `daemon.drain` dan `daemon.undrain`. |
| U3 | selesai | Keputusan murni di `mandor-domain/src/service.rs` (tabel plan, unit systemd dengan escape `%`/`"`/`\\`/`$`, parse dan bandingkan versi). `mandord install [--check] [--keep-awake-cmd]` dan `uninstall [--force]` di `mandord/src/install.rs`: salin atomik `create_new` + rename, tolak symlink dan direktori bukan milik pengguna, drain lewat `DaemonClient` (dipindah dari `mcp_client` ke `mandor-api/src/client.rs`), undrain di setiap error setelah drain, verifikasi `/proc/<pid>/exe` sebelum SIGTERM, linger sebagai peringatan. `status` juga melaporkan `draining`. Belum teruji otomatis: drain `busy` dari daemon dengan run yang bekerja dan daemon lama tanpa `daemon.drain` (keduanya butuh sesi Lark; logika busy teruji di `mandor-usecase/tests/drain.rs`). |
| U4 | selesai | `connect` menjalankan `systemctl --user start mandord.service` (timeout 15 detik) bila unit ada, lalu menunggu `daemon.json` seperti biasa; unit tidak ada atau `systemctl` gagal jatuh ke setsid. Daemon setsid mewarisi environment `connect`, termasuk `MANDOR_KEEP_AWAKE_CMD`. |
| U5 | selesai, menunggu uji Windows | `tauri.windows.conf.json` membawa `resources/mandord`; `build.rs` menanam SHA-256 (placeholder → hash kosong; build release Windows tanpa biner asli gagal). Workflow `release`: job installer menunggu job daemon dan memeriksa hash tertanam (lulus, hash `94b4336c…`). Hook NSIS `--uninstall-cleanup`. `scripts/check-versions.sh` di CI. |
| U6 | selesai, menunggu uji Windows | `provision.rs`: probe hash + `install --check` lewat `wsl.exe -e`, verifikasi hash resource, aliran stdin ke `~/.cache/mandor` (noclobber), `install`; salinan penjaga per versi di `%LOCALAPPDATA%`. Relay memakai `~/.local/bin/mandord` dan meneruskan `MANDOR_KEEP_AWAKE_CMD`. |
| U7 | selesai, menunggu uji Windows | Daemon: pemantau keep-awake (masa tenggang 5 menit, jalankan ulang dengan jeda, `failing` beserta pesan), teruji dengan daemon sungguhan. App: mode `--keep-awake` (keepawake, thread utama, sampai stdin EOF), `.wslconfig` aktif/nonaktif dengan cadangan sekali dan tanpa jejak. Spike A5 di WSL: interop dari konteks systemd user berjalan tanpa `WSL_INTEROP`, proses Windows hidup selama stdin terbuka dan keluar saat EOF. |
| U8 | selesai | Settings → Background work; provisioning sebelum token pertama; retry pembaruan tertunda saat tidak sibuk; keep-awake milik app untuk daemon lama. Dicek visual di mode mock. |
| U9 | runbook selesai; uji langsung menunggu | `docs/runbook/setup-wsl.md` ditulis ulang untuk satu installer. Uji langsung Windows (installer, keep-alive, AE1/AE4 `powercfg`, uninstall) dijalankan bersama pemilik produk; installer uji: `C:\Users\hando\Downloads\mandor-uji\mandor_0.1.0_x64-setup.exe`. |

### Penyimpangan dari plan

- Hook uninstall memakai `mandord uninstall` **tanpa** `--force` (KTD-48 menyebut `--force`): R3 (pekerjaan tidak diputus) didahulukan; bila ada agent bekerja, daemon dibiarkan dan pengguna diberi langkah manual.
- `install`/`uninstall` hanya dikompilasi di Linux (verifikasi pid memakai `/proc`; macOS memakai launchd di tahap 3).

### Matrix CI (Linux, macOS, Windows)

Perbaikan yang ditemukan matrix: bug hook akar desain lewat symlink (lolos di macOS dan lewat alias di Linux), test `guardrails` bergantung config git sistem, lint Windows `mandor-agent`, URL sqlite `sqlx-check` di Windows. Run terakhir yang berjalan: semua job hijau kecuali `make check (windows-latest)` di `sqlx-check`; perbaikannya belum terbukti di Windows karena run berikutnya ditolak billing GitHub Actions (batas pembayaran).


## Runbook

`docs/runbook/setup-wsl.md` diperbarui: aktifkan systemd, `mandord install` dari biner rilis atau hasil build, arti setiap `result`, `install --check`, pembaruan, migrasi dari daemon setsid, `uninstall` (dan `--force`), serta pemecahan masalah. `uninstall` kini melaporkan status linger yang sebenarnya.

## Temuan saat uji

- `connect` dan `install` dengan `MANDOR_HOME` selain `~/.mandor` sempat memakai unit systemd global, yang melayani `~/.mandor`; `connect` lalu menunggu daemon yang tidak pernah muncul (terlihat dari test `daemon.rs` setelah service sungguhan terpasang). Kini service hanya dipakai untuk direktori data bawaan; `install` dengan `MANDOR_HOME` lain hanya menyalin biner dan memberi peringatan.

## Gerbang

`make check` hijau (cakupan baris 89,9%), `bun run check` dan `bun run test` (100) hijau, TS protokol diregenerasi (`StatusResult` opsional `busy`, `managed_by`, `draining`, `keep_awake`; `DrainResult` baru).

## Uji langsung (2026-10-23, WSL Ubuntu dengan systemd user, mesin pemilik produk)

| Skenario | Hasil |
|---|---|
| `install --check` sebelum pasang | `binary: differs` (biner manual 18 Oktober), `service: missing`, `systemd: true`, `linger: true`, `running: detached`. |
| `install` dengan daemon setsid build lama (tanpa `daemon.drain`) berjalan, tanpa run yang bekerja | `{"result":"updated","service":"systemd","linger":true,"apps_stopped":true}` dalam 0,65 detik; jalur `runs.list` untuk daemon lama dipakai; daemon lama berhenti bersih; unit `active` dan `enabled`. |
| `status` lewat `~/.local/bin/mandord connect` | `managed_by: systemd`, `busy: false`, `draining: false`. |
| `kill -9` daemon | systemd menyalakan lagi dengan pid baru dalam ~2 detik (`Restart=always`); `status` tetap `managed_by: systemd`. |
| `systemctl --user stop`, lalu `mandord token` | daemon dinyalakan lewat service (unit kembali `active`), `managed_by: systemd`; sesi Lark tersambung beberapa detik setelah start. |
| `uninstall` dengan service berjalan, tanpa run yang bekerja | `{"result":"uninstalled",...}`; service `inactive` dan `not-found`, unit dan `~/.local/bin/mandord` terhapus, daemon berhenti, `~/.mandor` (database, `lark.json`) tetap. |
| `uninstall` kedua kali | tetap `uninstalled`, exit 0. |
| `install` ulang setelah uninstall | `installed`, service `active`, `managed_by: systemd`, sesi Lark tersambung. |

Setelah uji, daemon pemilik produk berjalan sebagai service dari `~/.local/bin/mandord` (build branch `feat/daemon-os-service`).

## Drain saat ada run yang bekerja

Diuji end-to-end di `crates/mandord/tests/install.rs` dengan daemon sungguhan dan RPC sungguhan: database diisi run berstatus `running` atau `publishing` sebelum daemon start (tanpa sesi Lark, rekonsiliasi tidak mengubahnya). `install` dan `uninstall` menghasilkan `deferred`, biner dan unit tidak disentuh, daemon tetap hidup, `status` melaporkan `busy: true` dan `draining: false`. Mutation check: mematikan pengecekan run di `DrainGate::drain` membuat kedua test gagal. Tanpa memakai kuota Claude atau mengubah kartu Lark.

Daemon versi lama tanpa `daemon.drain` (jalur `runs.list`) teruji langsung saat `install` pertama di mesin pemilik produk, tanpa run yang bekerja.

## Uji langsung Windows (U9, 2026-10-23, installer `aeae608`)

| Skenario | Hasil |
|---|---|
| Pasang installer di atas daemon build branch (setsid lama sudah diganti service) lalu buka app | lulus: biner `~/.local/bin/mandord` berganti dari `5e94ea18…` ke `4a3be871…` (sama dengan artifact `mandord-linux-x86_64`); service di-restart 16:55:23; unit berisi `MANDOR_KEEP_AWAKE_CMD` ke `%LOCALAPPDATA%\Mandor\keepawake\0.1.0\mandor-keepawake.exe`; `~/.cache/mandor` bersih; `status`: `managed_by: systemd`, `keep_awake: off`, `busy: false`, sesi Lark tersambung. |
| Settings → Background work setelah pasang | lulus: service, linger, keep-awake Ready; keep-alive WSL "Needs attention" dengan tombol Keep WSL on; pemberitahuan "Running dev apps stopped while the daemon updated" (app dev dari sesi daemon lama dihentikan saat pembaruan). |
| Keep WSL on | lulus: `.wslconfig` (sebelumnya hanya `[wsl2]`) menjadi `[general] instanceIdleTimeout=-1` dan `[wsl2] vmIdleTimeout=-1`; cadangan `.wslconfig.mandor-bak` identik dengan berkas awal (sha256 `6b39ebf9…`); baris berubah Ready dengan tombol Turn off, ringkasan "Ready for overnight work", petunjuk `wsl --shutdown`. |
| `wsl --shutdown`, buka app, Quit app dari tray, tunggu tanpa terminal WSL | lulus: VM WSL boot 17:02:52, service `active (running)` sejak 17:02:54 dan masih hidup pukul 17:15 (uptime VM 12,5 menit, tidak ada boot ulang); `mandor.exe` tidak berjalan. |
| AE4: `powercfg /requests` saat agent bekerja (kartu uji PGS-1512 `reczz28QooW8PMLJ` di Base salinan, batas sleep AC sementara 2 menit) | lulus: `SYSTEM` dan `AWAYMODE` menampilkan `…\AppData\Local\mandor\keepawake\0.1.0\mandor-keepawake.exe`; Background work "Keeping the laptop awake"; `status` `keep_awake: held`. |
| AE1: Quit app saat agent bekerja (agent menjalankan `time.sleep(240)` lalu test) | lulus: dipantau tiap ~17 detik dari WSL; 17:40:15–17:43:59 `busy: true`, `keep_awake: held`, `mandor.exe` tidak berjalan, tidak ada event Kernel-Power (sleep) di log System Windows; run selesai sendiri 17:44. |
| Pelepasan keep-awake | lulus: `busy: false` sejak 17:44:16, `keep_awake: off` sejak 17:49:25 (masa tenggang 5 menit); `mandor-keepawake.exe` tidak lagi berjalan di Windows (17:53). |

Temuan: penjaga juga mengambil permintaan `AWAYMODE` (crate `keepawake`), sehingga selama agent bekerja tombol Sleep manual memasukkan PC ke away mode, bukan sleep. Menunggu keputusan pemilik produk apakah dipertahankan.
| AE7: uninstall dari Windows (tidak ada agent bekerja) | lulus sebagian: service `inactive`/`not-found`, unit dan `~/.local/bin/mandord` terhapus, tidak ada proses `mandord`, `~/.mandor` (database, `lark.json`) tetap, folder app di Program Files terhapus, cadangan `.wslconfig.mandor-bak` tetap. **Bug**: `.wslconfig` menjadi kosong, padahal isi awalnya `[wsl2]` (bagian milik pengguna ikut dihapus karena kosong setelah kunci Mandor dibuang); melanggar "isi lain tetap". `%LOCALAPPDATA%\Mandor\keepawake` tertinggal (sesuai runbook: dibersihkan manual). |

### Perbaikan dari uji Windows

- `disable` `.wslconfig` memakai isi cadangan: bagian yang sudah ada sebelum `enable` pertama tidak dihapus walau kosong; hanya bagian buatan Mandor yang ikut dibuang. Test kasus nyata (`[wsl2]` tanpa baris baru) dan mutation check.
- Penjaga keep-awake hanya `idle(true)` (`ES_SYSTEM_REQUIRED`, sesuai A4); `sleep(true)` yang menambah `ES_AWAYMODE_REQUIRED` dibuang, sehingga Sleep yang dipilih pengguna tetap menidurkan PC. Perlu diverifikasi ulang dengan `powercfg /requests` (hanya `SYSTEM`) pada installer berikutnya.
| Uji ulang uninstall (20:25) | tidak sah: yang terpasang ternyata installer pertama (`mandor-uji\mandor_0.1.0_x64-setup.exe`, exe `2168ec2f…`), bukan `92a8b25` (exe `51e47f51…`); dibuktikan dengan mengekstrak exe dari tiap installer (7-Zip) dan membandingkan dengan salinan penjaga. Service dan biner tetap terlepas bersih. |
| `--uninstall-cleanup` exe `92a8b25` langsung di Windows, `USERPROFILE` ke folder uji | lulus: `.wslconfig` aktif + cadangan `[wsl2]` → `[wsl2]`; dengan kunci lain → `[wsl2]` `memory=8GB`; tanpa cadangan (berkas dibuat Mandor) → kosong. |
| AE4 ulang dengan installer `92a8b25` (penjaga `51e47f51…`) | lulus: `powercfg /requests` saat agent bekerja menampilkan `mandor-keepawake.exe` hanya di `SYSTEM`; `AWAYMODE: None.`. |
