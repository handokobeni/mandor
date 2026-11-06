# Progres: Pick from .env dan perbandingan tampilan dengan prototype

Plan: `docs/plans/2026-11-03-002-feat-env-picker-prototype-compare-plan.md`.

| Unit | Status | Catatan |
|---|---|---|
| U1 | selesai | `env_key_names` di `mandor-domain` (nama unik berurutan, lewati komentar, `export`, dan baris lanjutan nilai berkutip multi-baris seperti PEM). RPC `app.env_keys { repo_id \| design_source_id }` → `EnvKeysView { found, keys }`, dibaca dari `.env` folder asli dengan batas 5 detik, tanpa sesi Lark; nilai tidak pernah keluar, termasuk di pesan error. Dialog `EnvKeyPicker` di Settings → Repos (Laptop dan VM) dengan logika murni `envkeys.ts`. Test: 2 domain, 3 dispatch, 1 DTO, 4 Vitest. Dicek di `/?mock`. |
| U2 | belum | |
| U3 | belum | |
| U4 | belum | |
| U5 | belum | |
| U6 | belum | |
| U7 | belum | |
| U8 | belum | |
