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

## Bukti otomatis AE1–AE7

| AE | Test otomatis | Yang belum (uji langsung) |
|---|---|---|
| AE1 | `review_lab_data.rs` (snapshot dengan app berhenti, `lab_sql`, restore, sidik cocok); `review_flow.rs::covers_ae1_lab_menguji_calon_setelah_review_biasa` (alur lab setelah review biasa) | Batas atas KB di landlord kelola, sidik sungguhan |
| AE2 | `review_lab_data.rs` (env database ke host staging → lab data tidak jalan); `lab_data.rs` (`is_local_db`, `app_env_block`) | Database non-lokal sungguhan |
| AE3 | `review_lab_data.rs` (restore gagal → `restore_failed`, review dan giliran ditahan, retry dan konfirmasi) | Banner dan penahanan di app sungguhan |
| AE4 | `port.rs::giliran_lab_satu_server_per_role_tanpa_shell` (satu browser per role); `review_flow.rs::covers_ae1_…` (server `playwright_admin`, `ROLES:admin`, prepared menyebut akun); `review_flow.rs::password_akun_uji_disamarkan_di_bukti_dan_temuan`; `login_role_gagal_disebut_di_pesan_lab` | Manager tanpa `admin.profileData` di kelola |
| AE5 | `lab.rs::calon_lab_hanya_cant_verify_dengan_lab_need`, `calon_data_dikecualikan_bila_lab_data_belum_siap`, `run_grup_dikelompokkan_per_kartu_urut_kemunculan`, `baris_di_luar_gerbang_tidak_jadi_calon`; `review_flow.rs::covers_ae1_…` (baris di luar calon tertutup bagi sesi lab); `batas_lab_per_run_membuat_kartu_sisa_tetap_cant_verify` | Run grup 113 AC di kelola |
| AE6 | `review_flow.rs::covers_ae6_keputusan_rekomendasi_tampilan`, `rekomendasi_tampilan_hanya_untuk_baris_berbeda` | — |
| AE7 | `review_flow.rs::covers_ae7_tanpa_perlengkapan_lab_tidak_jalan_dan_ada_catatan`; `mcp.rs` | — |

## Uji langsung U10 (kelola PGS-1477–1483)

Daemon WSL dipasang ulang dari branch PR #28 pada 2026-11-10 (`mandord install` → `updated`). Installer Windows dibuat dari branch yang sama (Actions run 42791753932) untuk UI Review lab.

Pembanding: `../integro-kelola/kelola-v2/review-fixtures/keputusan-AC-PGS-1477-1483.md` (cek manual 2026-11-09). AC yang diharapkan diputuskan lab fase 1:

| Kebutuhan | AC | Keputusan manual |
|---|---|---|
| Simulasi request | 8612, 8614, 8615, 8633, 8634, 8636, 8637, 8645, 8648, 8660, 8439, 8442, 8664 | Lulus (respons diperlambat, kosong, 403/422/500, koneksi putus) |
| Role (Manager tanpa admin.profileData) | 8616, 8635, 8643, 9007, 8435, 8656 | Lulus (halaman ditolak, endpoint 403) |
| Berkas uji | 8436, 8437, 8661, 8662, 8665, 9035 | Lulus (pilih/tolak berkas, batas ukuran, nama berkas) |

Di luar fase 1 (mengubah data, fase 2): 8630, 8631, 8941, 9371, 9372, 8409, 8410, 8418, 8420, 8647, 8421–8441 yang mengunggah nilai, 9424–9426.

Hasil: menunggu.

Rencana uji fase 2 (bersama fase 1, setelah daemon dipasang ulang dengan biner fase 2):

1. Buat role `mandor_lab` berhak minimum di database lokal kelola-v2 (tenant dan landlord) dan skrip snapshot/restore/sidik/test change di `review-fixtures/lab` repo kelola; setujui perlengkapan data (tenant, landlord, folder unggahan) dengan uji bolak-balik.
2. Run lab di PGS-1477–1483: AC data di atas dan isolasi antar-corporate (Pentest 2026), termasuk batas atas KB di landlord (AE1).
3. Sesudah setiap lab: sidik semua penyimpanan sama dengan sebelum lab, folder snapshot terhapus, tidak ada KB uji, nilai uji, atau data corporate kedua yang tersisa.
4. AE2: arahkan sementara ke database non-lokal, harap "lab only runs against local databases".
5. AE3: paksa restore gagal (skrip restore keluar non-nol), periksa banner, penahanan Start/review/giliran agent, lalu Retry restore.

Status 2026-11-10: **uji langsung ditunda** atas keputusan pemilik produk. Temuan saat persiapan:

- Run PGS-1477–1483 sudah ditutup dan branch-nya sudah di-merge, jadi uji butuh run terbuka baru untuk kartu kelola.
- Skrip lab dicocokkan dengan salinan di worktree run, jadi `review-fixtures/lab` harus ada di branch run kelola-web (ter-commit di `talentlytica/kelola-web` atau di branch run). Tempatnya diputuskan saat uji.
- Perlengkapan data ada di kelola-web (app yang diuji di browser), sedangkan database dipakai kelola-api; app run multi-repo dihentikan dan dinyalakan bersama, sehingga snapshot dan restore mencakup keduanya.
- Penyimpanan: Postgres lokal `postgres-vector` (127.0.0.1:57432, Docker) dengan DB `landlord`, `stagingintegro_db`, dan `pentest2026_db`, plus folder unggahan.
- Role `mandor_lab` sudah dibuat (bukan superuser, tanpa CREATEDB/CREATEROLE/BYPASSRLS, tanpa password). Hak: `SELECT, INSERT, UPDATE, DELETE` pada tabel biasa, `SELECT` pada view, `USAGE, SELECT` pada sequence di skema `public` ketiga DB. Tidak ada hak pada 15 foreign table `postgres_fdw` di DB tenant, supaya SQL lab di tenant tidak menulis ke landlord lewat FDW. Password diset developer (`ALTER ROLE mandor_lab PASSWORD …`) lalu diisi di Settings.
- Penyimpangan KTD-L16 / test scenario U11 (keputusan pemilik produk 2026-11-10): `role_block` tidak lagi menolak `dblink`, `postgres_fdw`, atau `file_fdw` yang sekadar terpasang, melainkan hanya bila role lab bisa memakainya: `dblink` lewat fungsi yang bisa dijalankan atau wrapper-nya, `postgres_fdw`/`file_fdw` lewat USAGE pada wrapper atau server atau hak pada foreign table. Alasannya, kelola memakai `postgres_fdw` (tenant → landlord) dan `dblink` (landlord) sebagai bagian app; dengan aturan lama perlengkapan data kelola tidak pernah bisa disetujui. Fungsi handler/validator FDW dan pengelola koneksi `postgres_fdw` memang bisa dijalankan PUBLIC, tetapi tidak membuka koneksi tanpa server dan user mapping. Diuji di domain dan di PostgreSQL 18 sungguhan.
- Review keamanan otomatis atas perubahan itu: jalur tidak langsung kini ikut dinilai. Hak per kolom pada foreign table (`has_any_column_privilege`), view bertingkat di atas foreign table (rekursif lewat `pg_rewrite`, karena view berjalan dengan hak pemiliknya), dan fungsi SECURITY DEFINER bukan milik extension yang bisa dijalankan role lab (dihitung untuk setiap extension, gagal tertutup). Putaran berikutnya menambah hak TRUNCATE (didukung `postgres_fdw` sejak PG 14) dan tabel induk partisi/pewarisan dari foreign table (akses lewat induk hanya memeriksa hak induk). Ketiganya diuji di PostgreSQL 18 sungguhan; DB kelola tidak punya jalur itu.
- `landlord` lokal: EXECUTE semua fungsi `dblink` dicabut dari PUBLIC (kelola-api memakai `postgres` superuser, tidak terpengaruh). Ketiga DB kelola kini lolos `role_block` untuk `mandor_lab`.

Perbaikan test flaky (ditemukan saat `make check`):

- `lab_proxy::tests::berhenti_melepas_port` (fase 1, PR #28): bind ulang ke port yang baru dilepas bisa kalah dari test paralel. Kini diperiksa bahwa proxy tidak lagi menjawab di port itu.
- `lab_exec::tests::seed_dengan_role_lain_ditolak` (fase 2): exec salinan skrip gagal `ETXTBSY` (text file busy) karena fork paralel. Balapan yang sama mungkin terjadi di produksi (persetujuan menyalin lalu langsung menjalankan skrip), jadi `lab_exec` kini mencoba ulang exec maks. 5 kali, hanya untuk `ETXTBSY`.

## Fase 2

Keputusan pemilik produk 2026-11-10: fase 1 dan fase 2 diuji langsung bersama setelah fase 2 selesai. Fase 2 dikerjakan di branch `feat/review-lab-phase2` yang bercabang dari `feat/review-lab-phase1`; PR fase 2 berbasis PR #28.

| Unit | Status | Catatan |
|---|---|---|
| U11 | selesai | `mandor-domain::lab_data`: `is_local_db`, `parse_dsn` (host di query string atau beberapa host ditolak), `check_statement`, `role_block` (atribut, keanggotaan, CREATE, TRIGGER, kepemilikan, extension, akses tabel), `other_clients` (gagal tertutup), `scripts_match`, mesin status sesi `next_phase`, `restore_outcome`, `lock_keys`, `check_command`/`validate_data`; `LabEquipment.data` ikut validasi, digest, codec store, dan DTO `LabDataDto` |
| U12 | selesai | Migrasi `0015_lab_sessions.sql`; port `LabSessions`, `LabDatabase`, `DbCredentials`; `LabSessionsStore` (transisi `UPDATE … WHERE phase = ?`, `pending_recovery`, `holding`); `PgLabDatabase` (sqlx fitur `postgres` tanpa TLS, kueri runtime, koneksi tunggal dengan opsi eksplisit, batas waktu koneksi/katalog/pernyataan, keluaran ≤ 64 KiB). Diuji dengan PostgreSQL 18 sungguhan di kontainer sementara (`MANDOR_E2E=1`) |
| U13 | selesai | `mandord::lab_exec`: salinan skrip baca-saja di `<lab_root>/<repo>/private/scripts/<digest>/` (symlink ditolak, digest atas path, bit eksekusi, dan isi); perintah sebagai argv tanpa shell dari salinan, env bersih + `PG*` dari daemon (pemeliharaan untuk snapshot/restore/sidik, role lab + `MANDOR_WORKTREE` untuk seed), batas 300 detik dengan process group, keluaran ≤ 64 KiB disamarkan; folder snapshot 0700, mode berkas dikencangkan ke 0600. Uji bolak-balik persetujuan (database lokal, `pg_read_all_stats`, `role_block`, sidik berubah lalu kembali, seed hanya role lab) dipasang di `lab.approve`; kredensial database per penyimpanan di berkas secret; digest persetujuan menyertakan digest skrip (`repos.lab_scripts_digest`) |
| U14 | selesai | Orkestrasi `review/lab_data.rs`: sebelum kartu pertama dengan calon `data` → persiapan (skrip sama dengan salinan, database lokal, role, akses tabel, env app lokal), kunci, klien lain, sesi `snapshotting` → app berhenti → snapshot + sidik → app menyala → `running` dan proxy boleh menulis; sidik antar kartu dan restore bila berbeda; akhir lab → restore + sidik → `done`/`restore_waiting`(coba ulang di latar)/`restore_failed`. `restore_failed` menahan review, giliran agent, dan Start/Stop app repo itu (Stop diizinkan saat `restore_waiting`). Pemulihan saat start (`recover_lab_data`, kunci diambil ulang, penanda yatim dihapus). `lab_sql`/`lab_seed` (verdict kartu bertanda SQL otomatis), RPC `review.lab_start`, `lab.retry_restore`, `lab.data_restored`, `LabView.data_session`, peringatan di laporan pagi. Daemon: `lab_lock` (berkas kunci `O_NOFOLLOW` 0666, guard dipegang thread, penanda `.dirty`), `lab_data_port` (operasi lewat `lab_exec`, `PgLabDatabase`, secret), `MANDOR_LAB_LOCK_DIR`. Daftar tabel ikut sidik untuk deteksi perubahan skema |
| U15 | selesai | Settings → Repos → Review lab: bagian "Lab with test data" (folder skrip, penyimpanan database/folder dengan role lab dan pemeliharaan, seed), persetujuan dengan batas 30 menit untuk uji bolak-balik. Tab AC review: tombol **Run lab (N)** untuk calon data yang ditunda, dengan dialog konfirmasi. `LabBanner` di workspace (sesi berjalan, restore menunggu, restore gagal dengan Retry restore dan Data restored). Peringatan lab data di laporan pagi. Mock `?labdata=`; Vitest 282 lulus, `bun check` hijau, tampilan diperiksa di `/?mock` |
| U16 | sebagian | Runbook `review-lab.md` bagian 5 (skrip, role, persetujuan, sesi, restore gagal), risiko dan masalah umum fase 2; `ac-review.md`; rencana aktif di `CLAUDE.md`; role `mandor_lab` disiapkan. Uji langsung bersama U10 ditunda pemilik produk |

Catatan fase 2:

- `ctutils` dinaikkan ke 0.4.8 (RUSTSEC-2026-0385, lewat `sqlx-postgres` → `hkdf`/`digest`); butuh Rust 1.87, toolchain proyek 1.95.
- KTD-L15 diubah (keputusan pemilik produk 2026-11-10): server boleh melihat alamatnya sebagai alamat privat (jaringan kontainer Docker yang portnya dipublikasikan ke loopback, seperti `postgres-18` kelola-v2), selama host koneksi tetap loopback.
- Review keamanan otomatis atas U11: pengurai pernyataan SQL bisa dilewati dengan `E''`, `U&''`/`U&""`+`UESCAPE`, dan dollar quote; DSN dengan opsi selain `sslmode`/`application_name`/`connect_timeout` atau percent-encoding. Semuanya kini ditolak.
- Umask 077 untuk perintah lab (KTD-L19) tidak dipasang karena butuh `unsafe` (`pre_exec`); gantinya folder snapshot 0700 dan mode berkas diperiksa serta dikencangkan setelah snapshot.
- Test lab exec memakai skrip yang menimpa berkas, bukan menghapus (classifier menolak `rm -rf` dengan target variabel).
- U14, penyederhanaan KTD-L23: di laptop calon `data` selalu ditunda dengan alasan "Lab with test data deferred"; tombol Run lab (`review.lab_start`) memulai review baru dengan lab data diizinkan, sehingga tidak ada jeda 10 menit di Push & PR dan pemeriksaan HEAD tidak diperlukan (review berjalan di kode terbaru).
- U14: `lab_sql`/`lab_seed` ditawarkan ke token reviewer lab (peran Verdict) hanya selama sesi data berjalan (flag `data` di info ledger), bukan peran kapabilitas terpisah.
- U14: di laptop, lab data hanya berjalan bila review memakai browser; review hanya memakai browser bila app milik run ini menyala, sehingga syarat KTD-L20 "app dari run ini" terpenuhi lewat kesiapan review.
