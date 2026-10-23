# Progres: daemon sebagai service OS

Plan: `docs/plans/2026-10-23-001-feat-daemon-os-service-installer-plan.md`. Cakupan saat ini: U1–U4 (U5–U9 menyusul).

| Unit | Status | Catatan |
|---|---|---|
| U1 | selesai | `RunState::is_working` dan `any_working` (termasuk Preparing dan Publishing); `status` menambah `busy`, `managed_by`, `keep_awake` (opsional, daemon lama tidak mengirimnya); `--version` mencetak `mandord <versi>`. |
| U2 | selesai | `DrainGate` di `mandor-usecase` (std Mutex, tidak melewati `.await`; penghitung `in_flight`), dipegang `App` dan dibagi ke `RunService`; gerbang di semua titik mulai kerja (start, pesan, approve/continue plan, resume, takeover, publish, review, fix, `spawn_turn`); RPC `daemon.drain` dan `daemon.undrain`. |
| U3 | selesai | Keputusan murni di `mandor-domain/src/service.rs` (tabel plan, unit systemd dengan escape `%`/`"`/`\\`/`$`, parse dan bandingkan versi). `mandord install [--check] [--keep-awake-cmd]` dan `uninstall [--force]` di `mandord/src/install.rs`: salin atomik `create_new` + rename, tolak symlink dan direktori bukan milik pengguna, drain lewat `DaemonClient` (dipindah dari `mcp_client` ke `mandor-api/src/client.rs`), undrain di setiap error setelah drain, verifikasi `/proc/<pid>/exe` sebelum SIGTERM, linger sebagai peringatan. `status` juga melaporkan `draining`. Belum teruji otomatis: drain `busy` dari daemon dengan run yang bekerja dan daemon lama tanpa `daemon.drain` (keduanya butuh sesi Lark; logika busy teruji di `mandor-usecase/tests/drain.rs`). |
| U4 | selesai | `connect` menjalankan `systemctl --user start mandord.service` (timeout 15 detik) bila unit ada, lalu menunggu `daemon.json` seperti biasa; unit tidak ada atau `systemctl` gagal jatuh ke setsid. Daemon setsid mewarisi environment `connect`, termasuk `MANDOR_KEEP_AWAKE_CMD`. |

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
