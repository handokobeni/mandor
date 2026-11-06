# Izin agent dan kartu persetujuan

Plan: `docs/plans/2026-11-06-001-feat-agent-permission-approval-plan.md`.

## Tiga keputusan hook

Setiap tool call agent melewati hook `mandord hook --permit <berkas> --permit-sha <hex>`:

| Keputusan | Kapan | Akibat |
|---|---|---|
| Izin eksplisit | Perintah Bash terurai penuh, setiap bagian (termasuk `cd x && …`, pipe, redirect, path berisi `( ) [ ]`) ada di allowlist giliran, folder kerja dan target `cd`/`-C`/`--dir` di dalam worktree run dan bukan repo bersarang, tidak ada argumen berbahaya, dan tidak menulis di luar run | Berjalan tanpa bertanya |
| Tolak | Larangan keras: `git push`/`commit`/`remote`, `gh`, berkas kredensial, penulisan ke `.claude`, `.git`, `.husky`, `.githooks`, Lark; aturan hanya-baca plan dan reviewer; folder yang tidak bisa diselesaikan; berkas izin hilang atau digest beda; hook melewati 5 detik | Ditolak dengan alasan `mandor: …`, tercatat di Chat |
| Tanpa keputusan | Selain itu (perintah di luar allowlist, substitusi `$(…)`, `eval`, `xargs`, here-doc, `NAME=value cmd`, `awk`/`sed`/`find`/`git` dengan opsi yang bisa menjalankan atau menulis, repo bersarang, symlink keluar run) | Claude Code memakai aturannya sendiri; di laptop permintaan yang perlu izin menjadi kartu |

Berkas izin ada di `~/.mandor/agent/permit/` (laptop, 0600) atau `<group>/runs/permit/` (VM, 0640 dalam folder 2750). Agent tidak boleh membaca atau menulis folder ini.

## Kartu persetujuan (laptop, giliran implementasi dan plan)

- Kartu muncul di tab Chat run dengan input lengkap, folder kerja, deskripsi agent (tidak diverifikasi), dan sisa waktu. Tombol: **Deny**, **Allow once**, **Allow this exact request for this run**.
- "Allow for this run" berlaku untuk tool, input persis (tanpa `description`), dan folder yang sama, hanya di jenis giliran yang sama, sampai run ditutup.
- Deny berlaku untuk permintaan persis sama di run itu, lintas giliran.
- Kartu yang tidak dijawab 10 menit menjadi expired dan ditolak. Setelah itu permintaan berikutnya ditolak langsung sampai tab Chat run dibuka.
- Maksimum 5 kartu menunggu per run dan input 64 KiB; lebih dari itu ditolak langsung.
- Badge "Needs approval" tampil di papan dan daftar run; notifikasi `Agent needs your approval to run a tool` bisa dimatikan di Settings › General.
- Kartu menunggu dibatalkan saat giliran selesai atau dihentikan, run ditutup, atau daemon restart.

## Reviewer dan VM

Reviewer AC dan semua giliran VM tidak punya kartu. Permintaan di luar allowlist ditolak. Penolakan reviewer tercatat di Chat; penolakan VM tercatat di laporan pagi kartu ("N requests were denied overnight").

## Masalah umum

- **Semua perintah ditolak "permit file digest does not match"**: berkas izin ditimpa giliran lain di folder yang sama. Hentikan dan jalankan ulang giliran.
- **"Mandor did not finish checking this request"**: hook tidak sempat mencatat bukti pemeriksaan (mis. melewati tenggat). Ulangi perintah; bila berulang, periksa beban disk.
- **Kartu tidak muncul**: pastikan daemon dan app versi terbaru (protokol `approvals.*`), dan run berjalan di laptop, bukan VM.
