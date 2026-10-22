# Usulan: workspace multi-repo (Rilis 1)

Status: **disetujui pemilik produk (2026-10-21) dan sudah dikerjakan** (lihat `docs/progress/rilis-1.md`). Mengubah
perilaku produk dan beberapa KTD di `docs/plans/2026-10-18-001-feat-korlap-lark-lembur-plan.md`.

## Masalah

Satu kartu sering mengubah beberapa repo terpisah sekaligus, misalnya `kelola-api` dan `kelola-web` di
`~/Work/integro-kelola/kelola-v2`. Folder induk itu bukan repo git dan tidak akan dijadikan monorepo. Saat ini
satu run = satu repo = satu worktree = satu branch = satu PR, dan repo lain hanya bisa dibaca (path pendamping, KTD-14).

Kerja di satu repo saja dan monorepo sudah tertangani, dan perilakunya tidak berubah.

## Keputusan pemilik produk (2026-10-21)

1. Status Lark pindah ke **Waiting Review setelah semua PR dibuat**.
2. Konfigurasi Claude di folder induk ikut dibawa, **termasuk MCP server**.

## Usulan

### Konsep

- **Workspace**: nama ditambah folder induk ditambah daftar repo yang sudah terdaftar di Mandor. Contoh: `kelola-v2` =
  `{kelola-api, kelola-web, kelola-app}`, folder induk `~/Work/integro-kelola/kelola-v2`.
- Repo tunggal dan monorepo tetap ditambahkan seperti sekarang. Workspace hanya pengelompokan opsional.

### Start

- Dialog Start memilih target: **satu repo** (seperti sekarang) atau **workspace**. Untuk workspace, dev mencentang repo
  yang ikut. Bawaannya semua; minimal satu.
- Mandor membuat worktree di setiap repo terpilih dari `origin/<default branch>` masing-masing, dengan **nama branch yang
  sama** (`mandor/<key>-<slug>`), di satu folder run: `~/.mandor/workspaces/<run>/<nama-repo>/`.
- Agent berjalan dengan cwd folder run itu. Agent hanya boleh menulis di dalam worktree milik run; path pendamping tetap
  read-only.
- Bila satu langkah gagal, semua worktree dan branch yang sudah dibuat di-rollback (KTD-9 berlaku per repo).

### Konfigurasi Claude dari folder induk

- `CLAUDE.md` di folder induk (bila ada) disalin ke folder run.
- **MCP server dari `.mcp.json` folder induk ikut, tetapi hanya yang dicentang dev per workspace di Settings.** Bawaannya
  tidak ada yang aktif. Agent tetap dijalankan dengan `--strict-mcp-config`, ditambah `--mcp-config` yang hanya berisi
  server terpilih.
  - Alasan: MCP server bisa menulis ke GitHub atau Lark dan melewati pagar Mandor, yaitu push hanya oleh daemon (KTD-11)
    dan tiga transisi status Lark. Untuk `kelola-v2` hanya ada `graft` (graf kode lokal).
  - Tool MCP tetap melewati hook `PreToolUse`. Hook menolak tool MCP dari server yang tidak dicentang.
- `.claude/settings.local.json` di folder induk **tidak** dibawa. Pagar izin agent tetap milik Mandor (KTD-10).
- Ini mengubah KTD-10 ("tanpa server MCP") menjadi "tanpa server MCP kecuali yang diaktifkan dev per workspace".

### Perubahan dan PR

- Tab Changes dikelompokkan per repo, dan Discard berlaku per berkas per repo.
- **Push & create PR**: commit dan push di setiap repo yang berubah, lalu satu PR per repo yang berubah. Repo tanpa
  perubahan dilewati. Pagar file sensitif (KTD-19) diperiksa per repo.
- Status Lark ditulis **Waiting Review hanya setelah semua PR berhasil dibuat**. Bila sebagian gagal, run menjadi Failed
  dengan daftar repo yang gagal; Resume atau Push ulang hanya mengerjakan sisanya, karena PR yang sudah ada dipakai lagi.
- `mandor_pr` di Lark berisi semua link PR, satu per baris. Kartu dan workspace menampilkan semua PR.

### Waiting Fix

- Komentar reviewer diteruskan ke sesi agent yang sama. Setelah perbaikan, daemon push ke repo yang berubah; PR yang
  sudah ada otomatis ikut terbarui.
- Pemeriksaan PR (`check_prs`) dan klaim tetap per kartu.

### Data

- Tabel baru `workspaces (id, name, root_path, mcp_servers_json)` dan `workspace_repos (workspace_id, repo_id)`.
- Run menyimpan daftar `(repo_id, worktree, pr_url)` sebagai pengganti satu `worktree` dan satu `pr_url`. Run lama
  dimigrasi menjadi daftar satu repo.

## Di luar cakupan

- Mengubah `kelola-v2` menjadi monorepo.
- Membawa `.claude/settings.local.json`, hook, atau izin dari folder induk.
- PR gabungan lintas repo.

## Perkiraan unit

1. Domain dan store: workspace, run multi-repo, migrasi.
2. Git: worktree dan rollback per repo, publish multi-PR.
3. Agent: cwd folder run, `CLAUDE.md`, `--mcp-config` server terpilih, hook menolak MCP yang tidak dicentang.
4. Usecase: Start, publish, dan Waiting Fix multi-repo; transisi Waiting Review setelah semua PR.
5. Protokol dan UI: Settings › Workspaces, pilihan target di dialog Start, Changes per repo, banyak PR.

Setiap unit test-first sesuai `rust-engineering`, dengan gerbang `make check` hijau.
