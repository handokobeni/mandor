# Mandor

Mandor adalah papan kerja untuk agent Claude Code yang tersinkron dua arah dengan Lark Base Sprint Tasks. Isinya:

- `mandord`: daemon Rust yang memegang semua logika (sinkron Lark, git, agent, SQLite).
- `apps/desktop`: client Tauri v2 + Svelte 5 yang hanya berisi UI.

Semua perintah di dokumen ini dijalankan dari folder `mandor/`.

## Prasyarat

| Alat | Versi | Catatan |
|---|---|---|
| Rust | 1.95 (`rust-toolchain.toml`) | `rustup` memasang otomatis |
| Bun | 1.3+ | untuk frontend |
| Dependensi Tauri (Linux/WSL) | | `sudo apt-get install -y libwebkit2gtk-4.1-dev librsvg2-dev libayatana-appindicator3-dev` |
| `gh` | | login: `gh auth login` |
| Claude Code (`claude`) | | login dengan langganan, bukan API key |
| Git | | repo yang dikerjakan harus punya `origin/HEAD` (`git remote set-head origin --auto`) |

Di WSL, jendela app tampil lewat WSLg (bawaan Windows 11), jadi tidak perlu build Windows untuk mencoba.

## Menjalankan app (mode pengembangan)

1. Pasang dependensi frontend (sekali):
   ```bash
   bun install
   ```
2. Build daemon:
   ```bash
   cargo build --bin mandord
   ```
3. Jalankan app dan arahkan ke daemon hasil build:
   ```bash
   cd apps/desktop
   MANDOR_LINK=local:$PWD/../../target/debug/mandord bun run tauri dev
   ```
   App menyalakan daemon sendiri di latar bila belum berjalan.

Alternatif tanpa `MANDOR_LINK`: pasang daemon ke PATH, lalu jalankan `bun run tauri dev` saja.
```bash
cargo build --release --bin mandord
install -m 0755 target/release/mandord ~/.local/bin/mandord
```

### Meninjau tampilan tanpa daemon

```bash
cd apps/desktop && bun run dev
```

Buka `http://localhost:1420/?mock` di browser. Data contoh mencakup semua state run dan flag kartu, tanpa daemon dan tanpa Lark. Mode ini hanya ada di mode dev.

## Pertama kali membuka app

1. **Settings → Lark**: isi App ID, App Secret, `app_token` Base, ID tabel Sprint Tasks, dan ID tabel Acceptance Criteria, lalu **Save**. Cara membuat Lark Custom App ada di `../docs/runbook/setup-lark-app.md`.
2. Klik **Log in with Lark** lalu setujui akses di browser.
3. **Settings → Repo → Add**: path repo yang akan dikerjakan agent (di mesin daemon).
4. Di papan, pilih task berstatus To Do dengan Development Readiness "Ready To Dev", lalu klik **Start**.

Gunakan salinan uji Base saat mencoba, agar data tim tidak berubah.

## Saat kode berubah

| Yang berubah | Yang perlu dilakukan |
|---|---|
| UI (`apps/desktop/src`) | Tidak ada; `tauri dev` me-reload otomatis |
| Native Tauri (`apps/desktop/src-tauri`) | Tidak ada; `tauri dev` build ulang dan membuka ulang app |
| Daemon (`crates/`) | `cargo build --bin mandord`, lalu `pkill -TERM -x mandord`; app tersambung ulang dan menyalakan daemon baru |
| Protokol (`crates/mandor-protocol`) | `MANDOR_REGEN_TS=1 cargo test -p mandor-protocol` untuk memperbarui tipe TypeScript |
| Query SQL (`crates/mandor-store`) | `make sqlx-prepare` |

Agent yang sedang berjalan ikut berhenti saat daemon di-restart. Run-nya ditandai "failed" dan bisa dilanjutkan dengan tombol **Resume** (sesi Claude yang sama).

## Lokasi data dan log

| Path | Isi |
|---|---|
| `~/.mandor/mandor.db` | SQLite: cache task, run, chat, outbox |
| `~/.mandor/mandord.log` | Log daemon (JSON); level lewat `MANDOR_LOG`, mis. `MANDOR_LOG=debug` |
| `~/.mandor/lark.json` | Pengaturan Lark (0600) |
| `~/.mandor/secrets/lark-token.json` | Token user Lark (0600) |
| `~/.mandor/daemon.json` | Port, token, dan pid daemon yang berjalan (0600) |
| `~/.local/share/mandor/workspaces/` | Worktree per task, di luar `~/.mandor` karena agent dilarang membaca folder itu (tempat token) |

`MANDOR_HOME` mengganti lokasi `~/.mandor`, berguna untuk mencoba dengan data terpisah; worktree lalu ada di `$MANDOR_HOME/workspaces/`.

## Variabel environment

| Variabel | Default | Fungsi |
|---|---|---|
| `MANDOR_LINK` | `local` (PATH) di Linux/macOS, `wsl` di Windows | Cara UI menjangkau daemon: `local:/path/mandord` atau `wsl:<distro>` |
| `MANDOR_HOME` | `~/.mandor` | Direktori data daemon |
| `MANDOR_LOG` | `info` | Filter log daemon |
| `MANDOR_CLAUDE_BIN` | `claude` | Biner Claude Code yang dipakai agent |

## Test dan gerbang

```bash
make check        # fmt, clippy pedantic, cargo deny, cek sqlx offline, test + coverage
make red-check    # bukti lapisan crate ditegakkan
make clean-build  # bersihkan folder build daemon dan Tauri bila sudah menumpuk
cd apps/desktop && bun run check && bun run test
cd apps/desktop/src-tauri && cargo clippy --all-targets -- -D warnings && cargo test
```

Git hook (fmt + clippy sebelum commit, `make check` sebelum push) aktif setelah sekali menjalankan:

```bash
git config core.hooksPath .githooks
```

CI di `.github/workflows/ci.yml` menjalankan gerbang yang sama di Linux untuk setiap push ke `main` dan setiap pull request, serta pengecekan advisory harian. Linux, macOS, dan Windows berjalan seminggu sekali dan bisa dipicu manual dari tab Actions (**Run workflow**), karena menit macOS dan Windows di repo private memakan kuota lebih banyak.

Smoke test terhadap Lark, GitHub, dan Claude sungguhan (Rilis 1 dan Rilis 2): lihat `crates/mandor-e2e/README.md`.

## Review AC (Rilis 2)

Sumber desain, app web untuk diuji di Chrome, gerbang Push & PR, view "Design gaps" di Lark, dan biaya review: lihat `docs/runbook/ac-review.md`.

## Run grup kartu satu flow

Beberapa kartu satu epic milik Anda dikerjakan sebagai satu run, satu branch, dan satu PR ("Start together" di dialog Start): lihat `docs/runbook/grouped-runs.md`.

## Build rilis

```bash
cargo build --release --bin mandord          # daemon
cd apps/desktop && bun run tauri build       # installer app desktop
```

Installer Windows tidak bisa dibangun dari WSL. Workflow `release` di GitHub Actions membangun biner `mandord` untuk WSL, lalu installer Windows (NSIS) yang membawa biner itu sebagai resource dengan SHA-256 tertanam di app; app memasang dan memperbarui daemon di WSL sendiri. Jalankan manual dari tab Actions (hasil berupa artifact), atau push tag `v*` untuk membuat GitHub Release beserta lampirannya.

Versi app (`tauri.conf.json`, `package.json`, `apps/desktop/src-tauri/Cargo.toml`) dan `mandord` harus sama: `./scripts/check-versions.sh` (dijalankan CI).

Build app di Windows tanpa biner daemon (pengembangan) butuh placeholder; app build dev tidak pernah memasangnya:

```bash
printf 'placeholder\n' > apps/desktop/src-tauri/resources/mandord
```

Untuk Windows dengan daemon di WSL, ikuti `docs/runbook/setup-wsl.md`.
