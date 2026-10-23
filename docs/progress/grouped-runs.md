# Progres: run grup kartu satu flow

Plan: `docs/plans/2026-10-23-004-feat-grouped-card-runs-plan.md`. Branch `feat/grouped-card-runs` dari `main`.

| Unit | Status | Catatan |
|---|---|---|
| U2 | selesai | `Task.epic_ids` dari field `Epic` (cache lama tetap terbaca lewat default serde); port `LarkTasks::epics` membaca tabel epic tertaut (`Epic Name`, `ID`); `PrRef.merged` dari `PrState` gh yang sudah ada. |
| U1 | selesai | `mandor-domain/src/group.rs`: state kartu, kandidat (epic, PIC, To Do, gerbang Start, run/VM lewat fungsi `busy`), urutan nomor kartu, kartu berikutnya, `still_ours`, aksi kehilangan klaim, kartu publish dan kartu yang menahan gerbang. `ac_ledger::restore_decisions` memulihkan Push anyway / konfirmasi manual bila verdict tidak berubah. |
| U3 | selesai | Migrasi `0006_run_cards.sql`: tabel `run_cards` (state, `data_json` berisi commit per repo, plan, data Waiting Fix kartu, `open`) dengan indeks unik parsial kartu terbuka; kolom `ac_rows.card`. `RunStore`: `create_group` atomik, `cards`, `save_cards` (dengan pindah kartu utama); `create` run tunggal dan `create_group` saling menolak kartu yang sama; `active_for` mencari lewat kartu; `close_run` menutup kartu. `AcRow.card`. |
| U4 | selesai | `RunService::start_group` (`run/group.rs`): tolak < 2 kartu dan kartu yang punya run, klaim berurutan dengan pelepasan bila gagal, branch `mandor/<awal>-<nomor akhir>-<slug epic>` (fallback judul kartu pertama), satu worktree (per anggota untuk workspace), `create_group`, buku AC berlabel kartu, giliran pertama kartu pertama dengan konteks flow dan AC kartu itu saja, lalu semua kartu To Do → In Progress. `group_candidates` dari cache. Refresh buku AC grup mengambil AC semua kartu aktif; satu kartu gagal dibaca membatalkan refresh. |
| U5 | selesai | `next_card` (AwaitingDev + Implement): commit lokal per repo dengan pesan `<KEY>: …` (tanpa push), kartu `Done` dengan commit dan plan-nya, lalu `advance`: periksa ulang kartu berikutnya di Lark (R9; berubah → `Removed`, klaim dilepas, status tidak diubah, grup berhenti dengan catatan) atau giliran kartu berikutnya di sesi yang sama (plan lagi bila grup memakai plan). Semua selesai → catatan "Push & PR". `continue_group`, `remove_card` (hanya `pending`), `stop_group` (push commit tanpa PR, `close_run` melepas klaim semua kartu terbuka). Heartbeat per kartu: klaim semua kartu terbuka diperbarui; kehilangan klaim kartu aktif menghentikan run, kartu `Done` ditandai `changed_in_lark`. `GitPort::commit_all` mengembalikan hash, `revert_commit` baru (git sungguhan teruji, konflik dibatalkan). `DesignGaps` saat approve plan ditulis ke kartu aktif. |

### Penyimpangan dari plan

- KTD-G6 menyebut kolom `decided_verdict`. Tidak perlu: Skipped selalu berasal dari Fail dan Checked manually dari Can't verify, jadi keputusan lama cukup disimpan sebagai baris sebelum review dan dipulihkan dengan `restore_decisions`.
