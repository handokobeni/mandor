# mandor — Aturan Proyek (Mengikat)

Repo ini membangun **Mandor**, penulisan ulang Korlap: daemon Rust `mandord` + client Tauri v2/Svelte 5, dengan sinkron Lark Base. Kode, dokumen rencana (`docs/plans/`), progres (`docs/progress/`), desain (`docs/design/`), dan runbook (`docs/runbook/`) ada di repo ini; jalankan `make`, `cargo`, dan `bun` dari root repo. Cara menjalankan app: `README.md`. Rencana aktif: `docs/plans/2026-10-18-001-feat-korlap-lark-lembur-plan.md` (Rilis 1), `docs/plans/2026-10-21-003-feat-ac-review-plan.md` (Rilis 2), `docs/plans/2026-10-23-001-feat-daemon-os-service-installer-plan.md` (daemon sebagai service OS), dan `docs/plans/2026-10-23-002-feat-vm-overnight-mode-plan.md` (Rilis 3, lembur di VM).

Source Korlap asli (MIT) ada di luar repo ini, di folder `korlap/` di sebelahnya (`../korlap` dari root repo), dan **hanya** menjadi referensi perilaku. Jangan mengubahnya, dan aturan di `korlap/CLAUDE.md` **tidak berlaku** untuk kode baru.

---

## ATURAN WAJIB: standar `rust-engineering`

**Setiap kode Rust di repo ini wajib mengikuti skill `rust-engineering`.** Ini aturan yang tidak boleh dilanggar.

- Sebelum menulis, mengubah, mereview Rust, atau menambah crate: muat skill `rust-engineering` dan baca file panduan yang sesuai dengan yang sedang dikerjakan (lihat tabel "When to read which file" di skill).
- Pelanggaran tidak boleh diterima dengan alasan apa pun, termasuk "test sudah lulus", "sementara dulu", atau "nanti diperbaiki". Bila sebuah tugas tidak bisa diselesaikan tanpa melanggar standar, **berhenti dan tanyakan** ke pemilik produk.
- Perubahan dianggap selesai hanya bila checklist "Before you finish any change" di skill terpenuhi dan `make check` hijau.

### Keputusan cakupan (sudah ditetapkan, jangan ditanyakan ulang)

**Jenis layanan:** produksi. `mandord` adalah daemon lokal di mesin dev, bukan HTTP API cloud. Semua aturan kode berlaku penuh. Aturan berikut **tidak berlaku** karena khusus HTTP API cloud:

| Aturan skill | Status | Alasan |
|---|---|---|
| Rate limit per client (429), global load shed (503) | N/A | Hanya client lokal milik pengguna yang terhubung, dengan token |
| Endpoint `/metrics` Prometheus, `/healthz`, `/readyz` | N/A | Diganti method RPC `status` dan log `tracing` |
| Load test harness | N/A | Tidak ada trafik jaringan publik |
| 12-factor / serverless container, cold start container | N/A | Didistribusikan sebagai app desktop; start daemon tetap harus cepat dan tidak memblok pada layanan luar |
| Konsistensi kontrak HTTP API | Disesuaikan | Berlaku sebagai satu envelope JSON-RPC 2.0 yang konsisten di `mandor-protocol` |
| Stateless | Disesuaikan | Daemon menyimpan state di SQLite, bukan di memori proses; state di memori harus bisa dibangun ulang saat restart |

**Tier arsitektur:** ports (ada aturan bisnis nyata). Lapisan adalah crate workspace dan dependensi selalu mengarah ke dalam:

```
mandord (app) ──► mandor-api, mandor-store, mandor-lark, mandor-git, mandor-agent, mandor-proc ──► mandor-usecase ──► mandor-domain
```

| Crate | Isi | Boleh bergantung pada |
|---|---|---|
| `mandor-domain` | Aturan murni: derivasi kolom kanban, state machine alur kerja, keputusan klaim, filter penulis tepercaya, urutan kursor, allowlist transisi status, klasifikasi perintah hook | Tidak ada crate workspace; tidak ada crate I/O (tokio, sqlx, axum, reqwest) |
| `mandor-usecase` | Orkestrasi dan trait port dalam satu bundel `Deps` | `mandor-domain` |
| `mandor-store` | Adapter penyimpanan (sqlx SQLite) | usecase, domain; **satu-satunya** pengguna `sqlx` |
| `mandor-lark` | Adapter Lark (reqwest) | usecase, domain |
| `mandor-git`, `mandor-agent`, `mandor-proc` | Adapter git/gh, Claude CLI, dan helper proses | usecase, domain |
| `mandor-api`, `mandor-protocol` | Server axum WebSocket dan relay stdio, DTO protokol | `mandor-protocol`; **satu-satunya** pengguna `axum` selain app |
| `mandord` | `main`, `Config::from_env()`, wiring manual, subcommand `connect` dan `hook` | semua crate; **satu-satunya** pengguna `anyhow` |
| `mandor-e2e` | Smoke test manual Rilis 1 (`MANDOR_E2E=1`), menggerakkan biner `mandord` lewat relay | tidak ada crate workspace |

### Crate tambahan di luar allow list (sudah dijustifikasi)

| Crate | Alasan | Dipakai di |
|---|---|---|
| `sqlx` fitur `sqlite` | Allow list hanya menyebut Postgres; daemon lokal tidak punya Postgres | `mandor-store` |
| `process-wrap` | Process group (Unix) dan Job Object (Windows) tanpa `unsafe` di kode kita | `mandor-proc` |
| `ts-rs` | Generate tipe TypeScript dari tipe protokol | `mandor-protocol` |
| `fd-lock` | Lock file eksklusif single-instance lintas OS | `mandord` |
| `subtle` | Perbandingan token waktu-konstan | `mandor-api` |
| `tokio-tungstenite` | Client WebSocket untuk relay `mandord connect` dan test; sudah ada di graf lewat fitur `ws` axum | `mandor-api` |
| `sha2`, `base64` | PKCE S256 untuk OAuth Lark (hash dan base64url) | `mandor-lark` |
| `tempfile` (dev) | Direktori sementara untuk test | test semua crate |

Crate lain di luar allow list skill: sebutkan alasannya, cek `cargo deny`, lalu tambahkan ke tabel ini **dalam perubahan yang sama**. Crate Tauri hanya boleh di `apps/desktop/src-tauri`.

### Ringkasan aturan yang paling sering terlanggar

- Test-first (TDD): red → green → refactor lewat antarmuka publik.
- Error: `thiserror` per crate library, `anyhow` hanya di `mandord`; adapter menerjemahkan error infrastruktur ke error domain.
- Tidak ada `unwrap`/`expect`/`panic!`/`todo!` di jalur produksi; `unsafe` dilarang.
- Tidak ada I/O blocking atau CPU berat di thread async; tidak ada lock `std` yang ditahan melewati `.await`.
- Setiap I/O (proses, HTTP, database, file) dibatasi timeout.
- Setiap task yang di-spawn punya pemilik (`JoinSet`/`TaskTracker`) dan berhenti bersih saat shutdown (`CancellationToken`).
- Token dan secret memakai `secrecy::SecretString` dan tidak pernah masuk argv maupun log.
- Konfigurasi dibaca sekali di `main` lewat `Config::from_env()`; tidak ada `std::env::var` di tempat lain.
- Log memakai `tracing`; tidak ada `println!`/`eprintln!`/`dbg!`.

### Gerbang

`make check` = `cargo fmt --check` + `cargo clippy --workspace --all-targets -- -D warnings` (pedantic) + `cargo deny check` + test + coverage (`cargo llvm-cov --fail-under-lines`) + cek `sqlx` offline. CI memblok merge bila merah.

---

## Aturan lain

- Komunikasi dengan pemilik produk dalam Bahasa Indonesia.
- Frontend memakai Bun; test frontend memakai Vitest (`bun run --cwd apps/desktop test`).
- Rencana adalah sumber kebenaran: Product Contract > Planning Contract > unit. Perubahan perilaku produk harus ditanyakan dulu.
