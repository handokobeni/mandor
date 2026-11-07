# Progres: pertanyaan agent yang bisa diklik

Plan: `docs/plans/2026-11-07-001-feat-clickable-agent-questions-plan.md`. Branch `feat/clickable-agent-questions`.

## Unit

| Unit | Status | Catatan |
|---|---|---|
| U1 | selesai | `mandor-domain/src/question.rs`: validasi (1 pertanyaan ≤ 500, 2–4 pilihan, label unik ≤ 80, penjelasan wajib ≤ 300, ≤ 1 rekomendasi) dan status kartu. |
| U2 | selesai | Migrasi `0013_questions.sql` (UNIQUE `(run_id, generation)`), `QuestionsStore`, `pending_question_runs`. |
| U3 | selesai | `run/questions.rs`: `ask_question`, `answer_question`, `send_queued`, `discard_queued`; `finish_turn` tidak menyimpan plan dan menahan antrean bila giliran bertanya; Approve dan Approve and queue for VM ditolak selama ada pertanyaan; Continue to plan melewati; pesan chat menjawab; kartu ditutup setelah giliran mulai. Token Ask untuk brainstorm/plan laptop tanpa sumber desain. Perbaikan risiko token: `interrupt` mencabut token, `finish_turn` giliran lama hanya mencabut bila tidak ada giliran baru berjalan. |
| U4 | selesai | Alat `ask_developer` di `mcp.rs` (peran ask, dan design dengan flag `ask`), allowlist dan berkas izin plan, `ASK_TOOL_MESSAGE` di prompt brainstorm/plan laptop. |
| U5 | selesai | Rute kapabilitas `Ask` di `scoped_route` (peran ask tidak bisa membaca buku AC), `questions.list`, `questions.answer`, `runs.queued.send`, `runs.queued.discard`, event `questions.updated`, `RunView.pending_question`. |
| U6 | selesai | `QuestionCard.svelte` inline di Chat, `stores/questions.svelte.ts`, `questions.ts`; composer memberi petunjuk dan Send/Discard antrean yang ditahan; Approve disembunyikan, label "Question for you", tab tidak pindah ke Plan. Dicek di `/?mock` (run PGS-1350): kartu, klik, status Answered. |
| U7 | selesai | Uji langsung di laptop: lihat tabel di bawah. Uji UI di app Windows (installer branch ini) menunggu pemilik produk. |

## Uji langsung (laptop)

Kartu uji di Base salinan, repo `mandor-smoke`, mode Brainstorm, then plan: PGS-1580 (untuk uji UI di app Windows) dan kartu kedua "TEST Mandor pertanyaan agent lewat RPC" (alur daemon dengan Claude asli, digerakkan lewat `mandord connect`).

| AE | Skenario | Hasil |
|---|---|---|
| AE1 | Panggilan tidak valid ditolak tanpa kartu | Lulus langsung 2026-11-07 (kartu RPC): agent diminta memanggil `ask_developer` dengan satu pilihan → hasil alat error "give 2 to 4 options (got 1)", tanpa kartu. Juga test `ae1_pertanyaan_tidak_valid_ditolak_tanpa_kartu`, `pilihan_yang_salah_menyebut_nomornya`. |
| AE2 | Satu pertanyaan per giliran | Lulus langsung 2026-11-07: dalam satu giliran, panggilan valid pertama diterima (`q-b786…`), panggilan berikutnya ditolak "you already asked a question in this turn…"; hanya satu kartu. Juga test `ae2_satu_pertanyaan_per_giliran`, `satu_pertanyaan_per_generasi_giliran`. |
| AE3 | Klik pilihan mengirim label; kartu Answered | Lulus 2026-11-07 (kartu kedua, RPC): `questions.answer` opsi 0 → pesan dev "Bahasa Indonesia", kartu `answered`, giliran baru; klik kedua ditolak "the question was already answered". UI: `/?mock` menampilkan Answered dengan pilihan ditandai. |
| AE4 | Pesan chat menjawab kartu | Lulus 2026-11-07: pesan chat "Pakai gaya ramah-formal…" saat kartu Pending → kartu `answered_in_chat` dengan teks itu, giliran baru dengan pesan itu. |
| AE5 | Kartu bertahan setelah daemon restart | Lulus 2026-11-07: `systemctl --user restart mandord`, run tetap `awaiting_dev`, `pending_question` dan `questions.list` tetap Pending. |
| AE6 | Giliran plan yang bertanya tanpa Approve | Lulus 2026-11-07: revisi plan yang bertanya → `pending_question` terisi, plan lama tidak berubah (hash sama), `runs.approve_plan` dan `runs.prepare_for_vm` ditolak "the agent asked a question; answer it first". UI: Approve tersembunyi, label Question for you (`/?mock`). |
| AE7 | Brainstorm tanpa sumber desain mendapat alat | Lulus 2026-11-07 (PGS-1580, Start lewat RPC): agent memuat alat lewat ToolSearch lalu memanggil `mcp__mandor__ask_developer` dengan 3 pilihan (satu Recommended); balasan `q-… recorded…`; giliran selesai dan run `awaiting_dev`. |
| AE8 | Pertanyaan teks biasa tetap tampil | Lulus langsung 2026-11-07: agent diminta bertanya lewat teks biasa → tidak ada kartu baru, `pending_question` kosong, pertanyaan tampil sebagai pesan assistant; composer tetap bisa menjawab. Juga test di `chat.test.ts`. |
| AE9 | Antrean ditahan, Send/Discard | Lulus 2026-11-07: pesan dikirim saat giliran pertama berjalan (`queued`); giliran berakhir dengan pertanyaan → antrean tetap, tidak ada giliran baru; `runs.queued.send` → satu giliran, antrean kosong, kartu `answered_in_chat` dengan teks antrean. Discard: test `ae9_antrean_ditahan_lalu_dikirim_atau_dibuang`. |
| AE10 | Continue to plan melewati kartu | Lulus 2026-11-07: kartu Pending lalu `runs.continue_to_plan` → kartu `skipped`, fase plan, plan ditulis. |

Semua AE1–AE10 lulus uji langsung dengan Claude asli di daemon laptop; perilaku UI (kartu inline, klik, status akhir, Approve tersembunyi, label) dicek di `/?mock` dan Vitest. Uji di app Windows dari installer branch ini dilakukan pemilik produk sebelum merge.
