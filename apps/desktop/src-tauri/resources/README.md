`mandord` di direktori ini adalah biner daemon Linux yang dibawa installer
Windows dan paket Linux (diunduh workflow `release`). Berkas ini tidak
di-commit. Untuk build dev di Windows dan Linux (termasuk `bun run tauri dev`),
isi dengan placeholder:

    printf 'placeholder\n' > apps/desktop/src-tauri/resources/mandord

App build dev tidak pernah memasang placeholder (hash kosong). Build release
(`bun run tauri build`) tanpa biner asli gagal; untuk paket lokal, salin biner
`mandord` hasil `cargo build --release` ke sini lebih dulu.
