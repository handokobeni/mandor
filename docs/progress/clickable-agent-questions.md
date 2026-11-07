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
| U7 | berjalan | Uji langsung di laptop: lihat tabel di bawah. |

## Uji langsung (laptop)

Kartu uji PGS-1580 di Base salinan, repo `mandor-smoke`, mode Brainstorm, then plan.

| AE | Skenario | Hasil |
|---|---|---|
| AE1 | Panggilan tidak valid ditolak tanpa kartu | Test: `ae1_pertanyaan_tidak_valid_ditolak_tanpa_kartu` (usecase), `pilihan_yang_salah_menyebut_nomornya` (domain), `panggilan_pertanyaan_diteruskan_dengan_balasan_teks_polos` (mcp). |
| AE2 | Satu pertanyaan per giliran | Test: `ae2_satu_pertanyaan_per_giliran` (usecase), `satu_pertanyaan_per_generasi_giliran` (store, UNIQUE index). |
| AE3 | Klik pilihan mengirim label; kartu Answered | |
| AE4 | Pesan chat menjawab kartu | |
| AE5 | Kartu bertahan setelah daemon restart | Lulus 2026-11-07: `systemctl --user restart mandord`, run tetap `awaiting_dev`, `pending_question` dan `questions.list` tetap Pending. |
| AE6 | Giliran plan yang bertanya tanpa Approve | |
| AE7 | Brainstorm tanpa sumber desain mendapat alat | Lulus 2026-11-07 (PGS-1580, Start lewat RPC): agent memuat alat lewat ToolSearch lalu memanggil `mcp__mandor__ask_developer` dengan 3 pilihan (satu Recommended); balasan `q-… recorded…`; giliran selesai dan run `awaiting_dev`. |
| AE8 | Pertanyaan teks biasa tetap tampil | Test: `pertanyaan teks biasa tetap blok assistant` (chat.test.ts). |
| AE9 | Antrean ditahan, Send/Discard | |
| AE10 | Continue to plan melewati kartu | |

AE1, AE2, AE7, AE8 juga terbukti oleh test otomatis (`questions_flow.rs`, `mcp.rs`, `port.rs`, `chat.test.ts`).
