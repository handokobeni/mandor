# Progres: Izin agent tanpa penolakan diam-diam

Plan: `docs/plans/2026-11-06-001-feat-agent-permission-approval-plan.md`. Branch `feat/agent-permission-approval`.

| Unit | Status | Catatan |
|---|---|---|
| U1 | selesai | `mandor-domain/src/permit/shell.rs`: pengurai ketat menjadi perintah sederhana (kata + redirect) atau Unparseable untuk substitusi, pembungkus (`eval`, `sh -c`, `xargs`, `env`, …), here-doc, assignment di depan, ekspansi di luar kutip tunggal, glob, brace, tilde di awal kata atau setelah `=`/`:`. `allowlist.rs`: pencocok `Bash(…)` per kata (kutipan tidak bisa menyamarkan batas kata; `Bash(cd *)` butuh argumen; `Bash(git diff:*)` batas kata, `Bash(git diff*)` awalan kata terakhir). Perbaikan dari review keamanan otomatis: pencocokan per kata, `>&1x` ditolak. Temuan "redirect tulis lolos allowlist" sesuai R1; target redirect diperiksa di U2 (R16). |
| U2 | selesai | `permit/decide.rs`: `decide(scope, call, cwd, review, facts)` dengan urutan KTD-A8 (pagar VM, akar desain, pemeriksa giliran, path tulis yang diselesaikan, pengurai, argumen berbahaya, fakta path, allowlist). Fakta filesystem lewat trait `PathFacts` (`canonical`, `repo_root`). Folder kerja, `cd`, `git -C`/`make -C`/`--dir` harus ada (bila tidak: tolak), di dalam akar run, dan bukan repo bersarang (bila tidak: tanpa keputusan). Program pembuat/pemindah path bersama pindah folder → tanpa keputusan. Target redirect dan kandidat path (KTD-A7) diselesaikan lewat leluhur terdalam yang ada; kredensial → tolak; direktori terlindung → tolak untuk tulis, tanpa keputusan untuk baca. `request_key` (KTD-A12, input kanonis tanpa `description`, varian dengan/tanpa jenis giliran). `permit/args.rs` (KTD-A3, termasuk pengurai script `sed`), `permit/redact.rs` (KTD-A18; `redact_text`, `redact_json`). Helper `node_runs_code`, `rg_runs_program`, `dangerous_git_opt`, `in_protected_dir`, `is_write_tool` dibagi dari hook. `serde_json` (sudah di allow list) ditambahkan ke `mandor-domain`. Test: 28 domain permit. |
| U3 | belum | |
| U4 | belum | |
| U5 | belum | |
| U6 | belum | |
| U7 | belum | |
| U8 | belum | |
| U9 | belum | |

## Keputusan pelaksanaan

- Kandidat path yang tidak bisa diselesaikan (mis. `..` setelah komponen yang belum ada) menjadi tanpa keputusan, bukan tolak: kandidat sering bukan path (pola grep, nama branch). Folder kerja, target `cd`/`-C`/`--dir`, dan target redirect yang tidak bisa diselesaikan tetap ditolak (KTD-A6, R26).
- Kandidat path di luar run tidak membatalkan izin otomatis; R16 hanya memeriksa path terlarang R11. Batas run berlaku untuk folder kerja dan target pindah folder (R15).
