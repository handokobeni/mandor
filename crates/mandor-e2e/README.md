# mandor-e2e

Smoke test Rilis 1 terhadap salinan uji Base Sprint Tasks, repo GitHub uji, dan
login Claude Code milik pengguna. Tidak berjalan di CI.

## Persiapan

1. Ikuti `docs/runbook/setup-lark-app.md` dan `docs/runbook/setup-wsl.md` (bila WSL).
2. Jalankan app desktop, isi Settings → Lark, login, dan tambahkan repo uji.
3. Siapkan satu record uji di Base uji: Task Status *To Do*, Development Readiness
   *Ready To Dev*, Assignee berisi akun Anda.
4. Build daemon: `cargo build -p mandord`.

## Menjalankan

```bash
export MANDOR_E2E=1
export MANDOR_E2E_HOME=~/.mandor          # direktori data daemon yang sudah login
export MANDOR_E2E_RECORD=recXXXXXXXX      # record_id uji
export MANDOR_E2E_REPO=nama-repo          # id repo di Settings
cargo test -p mandor-e2e --test rilis1_smoke -- --ignored --test-threads 1 f1_
```

| Test | Skenario | Langkah manual |
|---|---|---|
| `f1_start_sampai_waiting_review` | F1: Start sampai Waiting Review | tidak ada |
| `f2_waiting_fix_kembali_ke_waiting_review` | F2: loop Waiting Fix | reviewer mengubah status ke Waiting Fix dan berkomentar |
| `restart_daemon_tidak_meninggalkan_proses` | restart di tengah run | jalankan saat ada run berjalan |

Skenario manual tambahan (dicatat di laporan rilis):

- Dua akun uji pada task dengan dua PIC: tekan Start hampir bersamaan dari dua
  laptop; hanya satu yang menjalankan agent, yang lain melihat error "lost the claim".
- Windows + WSL: tutup UI saat agent berjalan, tunggu melebihi `vmIdleTimeout`,
  buka lagi; agent masih berjalan dan chat berlanjut.
- Restart daemon di tengah perbaikan Waiting Fix: komentar tidak terkirim dua kali.
