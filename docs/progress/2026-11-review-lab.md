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

## Catatan

- `cargo deny` merah karena RUSTSEC-2026-0459 (SQLite bundled di `libsqlite3-sys` 0.37 lewat sqlx 0.9.0, juga merah di `main`). Mengabaikan advisory di `deny.toml` diblokir classifier sebagai bypass CI; menunggu keputusan pemilik produk.
- Stop condition KTD-L4: login kelola-v2 dua langkah bila akun punya lebih dari satu corporate. Keputusan pemilik produk 2026-11-10: akun uji kelola-v2 masing-masing satu corporate, login di host tenant; desain satu form tetap.
- Playwright MCP 0.0.83 dipasang di `~/.local` (`~/.local/bin/playwright-mcp`) beserta Chromium 1247; tool kode bernama `browser_run_code_unsafe`. Daemon menemukan biner itu otomatis di laptop.
- RUSTSEC-2026-0459 diabaikan di `deny.toml` dengan alasan (disetujui pemilik produk 2026-11-10).
- Review keamanan otomatis: `CONNECT` ke origin HTTP di luar sesi data bisa melewati blokir tulis. Diperbaiki: terowongan ke origin HTTP hanya saat tulis diizinkan (KTD-L6 diperbarui).
