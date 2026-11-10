# Progres review lab (plan 2026-11-10-001)

Branch fase 1: `feat/review-lab-phase1`.

| Unit | Status | Catatan |
|---|---|---|
| U1 | selesai | `mandor-domain::lab`: kebutuhan lab, bukti, temuan, rekomendasi, validasi perlengkapan, calon lab per kartu |
| U2 | selesai | migrasi `0014_review_lab.sql`, `lab_codec`, `lab_store`, `LabSettings` di `ReviewHistory` |
| U3 | selesai | `lab_secrets` (0600, di bawah `~/.mandor/lab`), RPC `lab.get/set/approve`, digest SHA-256, `lab.*` ditolak untuk token kapabilitas |
| U17 | selesai | `mandor-domain::lab::proxy_decision`, `mandord::lab_proxy` (hyper), dipasang ke review di U4 |
| U4 | selesai | Playwright MCP untuk review di laptop dan VM (Chrome dihapus dari review); `LabPort` (`ProdLab`) untuk perlengkapan, login daemon, proxy; klien login `mandor-agent::playwright_login` diuji dengan Playwright sungguhan |
| U5 | selesai | `ac_set_verdict` dengan langkah, screenshot, data disiapkan, `simulated`, `lab_need`; Pass lewat browser wajib langkah; tulis yang diblokir proxy membuat Can't verify `lab_need: data`; penyamaran password dan nilai cookie; `ac_report_finding`, `ac_recommend_look` |
| U6 | selesai | `TurnKind::Lab`, `classify_lab` (evaluate, upload dari staging, tanpa Bash, baca dibatasi), parser `mcp__playwright[_<role>]__`, state halaman per server (berkas state lama tetap terbaca), spec agent giliran lab dengan satu server per role, prompt reviewer lab |
| U7 | selesai | `review/lab.rs`: rencana lab saat review mulai, sesi lab per kartu berurutan (login semua role, staging berkas uji, proxy yang sama), `finish_lab_turn`, baris tak dilaporkan kembali dengan catatan, batas biaya/waktu per run, catatan bila perlengkapan belum disetujui, pembatalan di tengah lab; review perbaikan otomatis memakai ekor yang sama |
| U8 | selesai | `TurnKind::LabSetup` + `classify_lab_setup` (baca worktree utama tanpa `.env`, tanpa shell/browser/tulis); `review/lab_setup.rs` (`propose_lab`, 100 sen, 600 detik, satu giliran per repo); tool MCP `lab_propose` (hanya untuk peran `lab_setup`), field rahasia ditolak daemon (`secret_field`), usulan divalidasi U1 dan kiriman ulang sekali; RPC `lab.propose`, `LabView.proposal/proposing/proposal_error` |
| U9 | selesai | Protokol: `AcRowView.lab_need/evidence_detail/look_recommendation`, `Evidence.lab` (badge Lab), RPC `ledger.look_decision`, `review.findings`, screenshot bukti; UI: `RepoLab.svelte` (akun per role, form login, POST aman, folder berkas uji, Propose from repo, Save, Approve dengan ringkasan), `AcRow` (badge Lab/Simulated request/Prepared with SQL, langkah bernomor, data disiapkan, request diblokir, screenshot bukti, Approve/Reject rekomendasi), "Findings outside AC"; `lib/lab.ts` + Vitest; dicek di `/?mock&ac=lab` |
| U10 | sebagian | Runbook `docs/runbook/review-lab.md` (Playwright MCP 0.0.83 dipin, `browser_run_code_unsafe`, risiko), `ac-review.md` dan README diperbarui; `make check` hijau (cakupan baris 91%). Uji langsung di kelola PGS-1477–1483 menunggu developer (pasang ulang daemon, isi password akun uji, setujui perlengkapan) |

## Catatan

- `cargo deny` merah karena RUSTSEC-2026-0459 (SQLite bundled di `libsqlite3-sys` 0.37 lewat sqlx 0.9.0, juga merah di `main`). Mengabaikan advisory di `deny.toml` diblokir classifier sebagai bypass CI; menunggu keputusan pemilik produk.
- Stop condition KTD-L4: login kelola-v2 dua langkah bila akun punya lebih dari satu corporate. Keputusan pemilik produk 2026-11-10: akun uji kelola-v2 masing-masing satu corporate, login di host tenant; desain satu form tetap.
- Playwright MCP 0.0.83 dipasang di `~/.local` (`~/.local/bin/playwright-mcp`) beserta Chromium 1247; tool kode bernama `browser_run_code_unsafe`. Daemon menemukan biner itu otomatis di laptop.
- RUSTSEC-2026-0459 diabaikan di `deny.toml` dengan alasan (disetujui pemilik produk 2026-11-10).
- Review keamanan otomatis: `CONNECT` ke origin HTTP di luar sesi data bisa melewati blokir tulis. Diperbaiki: terowongan ke origin HTTP hanya saat tulis diizinkan (KTD-L6 diperbarui).
- Draf usulan perlengkapan (U8) hanya di memori dan tidak menimpa perlengkapan tersimpan; setelah restart developer cukup menekan "Propose from repo" lagi.
- Review keamanan otomatis: folder berkas uji bisa dialihkan lewat symlink di tengah jalur. Diperbaiki: tiap bagian jalur diperiksa tanpa mengikuti symlink dan jalur kanonis harus tetap di worktree.
- Review keamanan otomatis atas U8: pengusul membaca checkout developer (berisi `.env` tak terlacak) dan Grep/Glob tanpa path bisa menjangkau berkas env. Diperbaiki: pengusul berjalan di salinan bersih branch default (`git worktree add --detach`, hanya berkas ter-commit, dihapus setelah giliran), dan pola `glob` Grep / `pattern` Glob yang mengenai env atau kredensial ditolak di giliran lab dan pengusul.
- Review keamanan otomatis lanjutan: pola glob berbasis teks bisa dilewati (`.[e]nv`, `**/*.local`). Diganti tata bahasa sempit: tanpa `[ ? { ! ,` dan spasi, segmen akhir ber-wildcard wajib berakhir ekstensi allowlist, Grep di giliran lab wajib memakai `glob`/`type`.
- Review keamanan otomatis atas push PR #28: selama sesi lab, token Verdict bisa menimpa baris di luar calon lab kartu (mis. Fail review biasa). Diperbaiki: `ac_set_verdict`, `ac_recommend_look`, dan `ac_set_visual` hanya menerima calon lab kartu yang sedang diuji.
- Progres lab hanya di memori (plan: `LabCursor` di `RunRecord`); setelah daemon restart, review yang terputus ditutup rekonsiliasi seperti review biasa.
