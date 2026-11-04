# Progres: Pick from .env dan perbandingan tampilan dengan prototype

Plan: `docs/plans/2026-11-03-002-feat-env-picker-prototype-compare-plan.md`.

| Unit | Status | Catatan |
|---|---|---|
| U1 | selesai | `env_key_names` di `mandor-domain` (nama unik berurutan, lewati komentar, `export`, dan baris lanjutan nilai berkutip multi-baris seperti PEM). RPC `app.env_keys { repo_id \| design_source_id }` → `EnvKeysView { found, keys }`, dibaca dari `.env` folder asli dengan batas 5 detik, tanpa sesi Lark; nilai tidak pernah keluar, termasuk di pesan error. Dialog `EnvKeyPicker` di Settings → Repos (Laptop dan VM) dengan logika murni `envkeys.ts`. Test: 2 domain, 3 dispatch, 1 DTO, 4 Vitest. Dicek di `/?mock`. |
| U2 | selesai | Aturan domain `check_prototype_url` (loopback, port sama, host beda dari app) dan `port_conflict`. Sumber desain folder menyimpan setelan app di `config_json.app` (tanpa migrasi); `design_sources.add/update` memvalidasi start command + port, rentang port VM, bentrok dengan app repo dan prototype lain, dan URL (bawaan `http://127.0.0.1:<port>`). Menyimpan setelan app repo juga ditolak bila port dipakai prototype. Dialog sumber desain (laptop dan VM) punya "Runnable prototype" dan Pick from .env. Test: 2 domain, 1 integrasi daemon, 2 Vitest. |
| U3 | selesai | Port `DesignSources` mendapat `prototype`, `revision`, `changes_since`, `confirm`. Revisi folder di `mandor-git/src/revision.rs`: repo git `git:<HEAD>[+hash status]` (hanya baca, `--no-optional-locks`), folder lain `files:<sha256>` tanpa folder tersembunyi, `node_modules`, `dist`, `build`, `target` (batas 20.000 berkas, 30 detik). Revisi terkonfirmasi disimpan per sumber desain (`config_json.confirmed_rev`), bukan per run, agar run VM baru memakainya (R28); menyimpan setelan sumber mereset konfirmasi. `app_members` menambah prototype setelah anggota repo (kunci `prototype:<id>`, `cwd` folder sumber); `app_changes` memberi entri prototype dengan `confirmed`; `app_run` mengonfirmasi revisi yang ditampilkan; prototype gagal hanya menandai dirinya; restart otomatis (laptop dan VM) hanya menyalakan prototype pada revisi terkonfirmasi. RPC `design_sources.revision` dan `design_sources.confirm` untuk konfirmasi tanpa Run app (dialog "Prototype code" di daftar sumber desain, laptop dan VM). UI: "Open prototype" di kontrol Run app, nama prototype di dialog konfirmasi. Test: 3 adapter git, 1 adapter desain, 5 usecase app_run, 1 usecase VM (AE15), 1 integrasi daemon, 3 Vitest. |
| U4 | belum | |
| U5 | belum | |
| U6 | belum | |
| U7 | belum | |
| U8 | belum | |
