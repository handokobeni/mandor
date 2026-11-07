//! Pertanyaan agent yang bisa diklik (U3 plan 2026-11-07-001).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::sync::atomic::Ordering;

use common::run_fakes::eventually;
use common::{FakeClock, FakeLark, Station, ready_task, station, vm_station};
use mandor_domain::RunState;
use mandor_domain::question::{QuestionDraft, QuestionOption, QuestionStatus};
use mandor_usecase::review::{CapabilityRole, CapabilityScope};
use mandor_usecase::run_ports::RunPhase;
use mandor_usecase::{AskError, Planning, StartTarget};
use pretty_assertions::assert_eq;

const DAY: i64 = 24 * 60 * 60 * 1000;
const PLAN: &str = "## Plan\n1. Tambah tombol";

fn setup() -> (FakeLark, FakeClock) {
    let lark = FakeLark::default();
    let clock = FakeClock::default();
    clock.set(10 * DAY);
    lark.clock.store(10 * DAY, Ordering::SeqCst);
    let mut t = ready_task("rec1", &["ou_me"]);
    t.key = Some("PGS-7".to_owned());
    "Tab Data".clone_into(&mut t.title);
    t.last_modified_ms = 10 * DAY;
    lark.put(t);
    (lark, clock)
}

async fn start(st: &Station, planning: Planning) -> String {
    st.svc
        .start_planned(
            "rec1",
            &StartTarget::Repo("repo1".to_owned()),
            None,
            &planning,
            &[],
            &|_| {},
        )
        .await
        .unwrap();
    st.runs.only().id
}

fn brainstorm() -> Planning {
    Planning::Brainstorm {
        brainstorm: None,
        plan: None,
    }
}

fn plan() -> Planning {
    Planning::Plan { skill: None }
}

/// Lingkup token giliran terakhir.
fn scope(st: &Station) -> CapabilityScope {
    let token = st.agent.last().req.ledger.unwrap().token;
    st.svc.verify_capability(token.expose()).unwrap()
}

fn draft() -> QuestionDraft {
    QuestionDraft {
        question: "Keep the old result?".to_owned(),
        options: vec![
            QuestionOption {
                label: "Keep both".to_owned(),
                description: "Show both results".to_owned(),
                recommended: true,
            },
            QuestionOption {
                label: "Replace".to_owned(),
                description: "Only the new one".to_owned(),
                recommended: false,
            },
        ],
    }
}

/// Agent bertanya lalu mengakhiri giliran dengan `result`.
async fn ask_and_finish(st: &Station, run: &str, result: &str) -> String {
    let reply = st.svc.ask_question(&scope(st), &draft()).await.unwrap();
    st.agent.last().finish(result);
    eventually(|| st.runs.only().state == RunState::AwaitingDev).await;
    let q = st.questions.all(run).last().unwrap().clone();
    assert!(reply.starts_with(&q.id), "{reply}");
    q.id
}

fn last_notice(st: &Station) -> String {
    st.notifier
        .sent
        .lock()
        .unwrap()
        .last()
        .unwrap()
        .body
        .clone()
}

#[tokio::test]
async fn ae2_satu_pertanyaan_per_giliran() {
    let (lark, clock) = setup();
    let st = station(&lark, &clock, "ou_me");
    let run = start(&st, brainstorm()).await;
    st.svc.ask_question(&scope(&st), &draft()).await.unwrap();
    assert_eq!(
        st.svc.ask_question(&scope(&st), &draft()).await,
        Err(AskError::AlreadyAsked)
    );
    assert_eq!(st.questions.all(&run).len(), 1);
}

#[tokio::test]
async fn ae1_pertanyaan_tidak_valid_ditolak_tanpa_kartu() {
    let (lark, clock) = setup();
    let st = station(&lark, &clock, "ou_me");
    let run = start(&st, brainstorm()).await;
    let mut d = draft();
    d.options.truncate(1);
    let err = st.svc.ask_question(&scope(&st), &d).await.unwrap_err();
    assert!(err.to_string().contains("2 to 4 options"), "{err}");
    assert!(st.questions.all(&run).is_empty());
}

#[tokio::test]
async fn hanya_peran_ask_atau_design_yang_saat_bekerja_bisa_bertanya() {
    let (lark, clock) = setup();
    let st = station(&lark, &clock, "ou_me");
    let run = start(&st, brainstorm()).await;
    let verdict = CapabilityScope {
        run_id: run.clone(),
        role: CapabilityRole::Verdict,
    };
    assert_eq!(
        st.svc.ask_question(&verdict, &draft()).await,
        Err(AskError::NotAllowed)
    );
    // Giliran sudah selesai: tidak sedang bekerja.
    let s = scope(&st);
    st.agent.last().finish("Ringkasan");
    eventually(|| st.runs.only().state == RunState::AwaitingDev).await;
    assert_eq!(
        st.svc.ask_question(&s, &draft()).await,
        Err(AskError::NotAsking)
    );
    assert!(st.questions.all(&run).is_empty());
}

#[tokio::test]
async fn fase_implementasi_tidak_bisa_bertanya() {
    let (lark, clock) = setup();
    let st = station(&lark, &clock, "ou_me");
    let run = start(&st, Planning::Skip).await;
    assert!(st.agent.last().req.ledger.is_none());
    assert!(!st.agent.last().req.ask);
    let s = CapabilityScope {
        run_id: run,
        role: CapabilityRole::Ask,
    };
    assert_eq!(
        st.svc.ask_question(&s, &draft()).await,
        Err(AskError::NotAsking)
    );
}

#[tokio::test]
async fn ae7_vm_tidak_mendapat_alat_dan_menolak() {
    let (lark, clock) = setup();
    let st = vm_station(&lark, &clock, "ou_me");
    let run = start(&st, plan()).await;
    let req = st.agent.last().req;
    assert!(req.ledger.is_none(), "VM tanpa sumber desain: tanpa token");
    assert!(!req.ask);
    let s = CapabilityScope {
        run_id: run,
        role: CapabilityRole::Ask,
    };
    assert_eq!(
        st.svc.ask_question(&s, &draft()).await,
        Err(AskError::NotAllowed)
    );
}

#[tokio::test]
async fn ae7_laptop_brainstorm_tanpa_desain_mendapat_token_ask() {
    let (lark, clock) = setup();
    let st = station(&lark, &clock, "ou_me");
    start(&st, brainstorm()).await;
    let req = st.agent.last().req;
    assert!(req.ask);
    assert_eq!(req.ledger.map(|l| l.role), Some(CapabilityRole::Ask));
}

#[tokio::test]
async fn ae6_giliran_plan_yang_bertanya_bukan_plan() {
    let (lark, clock) = setup();
    let st = station(&lark, &clock, "ou_me");
    let run = start(&st, plan()).await;
    ask_and_finish(&st, &run, "Waiting for your answer").await;
    let r = st.runs.only();
    assert_eq!(r.plan, None, "teks penutup tidak menjadi plan");
    assert!(
        last_notice(&st).contains("has a question"),
        "{}",
        last_notice(&st)
    );
    assert!(st.svc.approve_plan(&run, &[]).await.is_err());
}

#[tokio::test]
async fn plan_lama_tidak_bisa_diapprove_selama_ada_pertanyaan() {
    let (lark, clock) = setup();
    let st = station(&lark, &clock, "ou_me");
    let run = start(&st, plan()).await;
    st.agent.last().finish(PLAN);
    eventually(|| st.runs.only().state == RunState::AwaitingDev).await;
    assert!(last_notice(&st).contains("Plan ready"));
    st.svc.send_message(&run, "revisi").await.unwrap();
    ask_and_finish(&st, &run, "Pertanyaan").await;
    assert_eq!(
        st.runs.only().plan.as_deref(),
        Some(PLAN),
        "plan lama tetap"
    );
    let err = st.svc.approve_plan(&run, &[]).await.unwrap_err();
    assert!(err.to_string().contains("asked a question"), "{err}");
    let err = st.svc.prepare_for_vm(&run, &[]).await.unwrap_err();
    assert!(err.to_string().contains("asked a question"), "{err}");
}

#[tokio::test]
async fn ae3_klik_pilihan_mengirim_label_sekali() {
    let (lark, clock) = setup();
    let st = station(&lark, &clock, "ou_me");
    let run = start(&st, brainstorm()).await;
    let q = ask_and_finish(&st, &run, "").await;
    st.svc.answer_question(&q, 0).await.unwrap();
    assert_eq!(st.agent.count(), 2);
    assert_eq!(st.agent.last().req.message, "Keep both");
    assert_eq!(st.runs.dev_messages().last().unwrap(), "Keep both");
    let rec = st.questions.all(&run)[0].clone();
    assert_eq!(rec.status, QuestionStatus::Answered);
    assert_eq!(rec.answer.as_deref(), Some("Keep both"));
    // Klik kedua: tidak ada giliran kedua.
    st.agent.last().finish("ok");
    eventually(|| st.runs.only().state == RunState::AwaitingDev).await;
    assert!(st.svc.answer_question(&q, 1).await.is_err());
    assert_eq!(st.agent.count(), 2);
}

#[tokio::test]
async fn jawaban_ditolak_saat_bekerja_atau_pilihan_tidak_dikenal() {
    let (lark, clock) = setup();
    let st = station(&lark, &clock, "ou_me");
    let run = start(&st, brainstorm()).await;
    st.svc.ask_question(&scope(&st), &draft()).await.unwrap();
    let q = st.questions.all(&run)[0].id.clone();
    assert!(
        st.svc.answer_question(&q, 0).await.is_err(),
        "masih bekerja"
    );
    st.agent.last().finish("");
    eventually(|| st.runs.only().state == RunState::AwaitingDev).await;
    assert!(st.svc.answer_question(&q, 5).await.is_err());
    assert_eq!(st.questions.all(&run)[0].status, QuestionStatus::Pending);
}

#[tokio::test]
async fn spawn_gagal_meninggalkan_kartu_pending() {
    let (lark, clock) = setup();
    let st = station(&lark, &clock, "ou_me");
    let run = start(&st, brainstorm()).await;
    let q = ask_and_finish(&st, &run, "").await;
    *st.agent.fail_spawn.lock().unwrap() = true;
    assert!(st.svc.answer_question(&q, 0).await.is_err());
    assert!(st.svc.send_message(&run, "jawab").await.is_err());
    assert_eq!(st.questions.all(&run)[0].status, QuestionStatus::Pending);
    *st.agent.fail_spawn.lock().unwrap() = false;
    st.svc.answer_question(&q, 1).await.unwrap();
    assert_eq!(st.questions.all(&run)[0].status, QuestionStatus::Answered);
}

#[tokio::test]
async fn kartu_di_run_gagal_tetap_bisa_dijawab() {
    let (lark, clock) = setup();
    let st = station(&lark, &clock, "ou_me");
    let run = start(&st, brainstorm()).await;
    st.svc.ask_question(&scope(&st), &draft()).await.unwrap();
    st.agent.last().crash("putus");
    eventually(|| st.runs.only().state == RunState::Failed).await;
    let q = st.questions.all(&run)[0].id.clone();
    st.svc.answer_question(&q, 0).await.unwrap();
    assert_eq!(st.questions.all(&run)[0].status, QuestionStatus::Answered);
}

#[tokio::test]
async fn ae4_pesan_chat_menjawab_kartu() {
    let (lark, clock) = setup();
    let st = station(&lark, &clock, "ou_me");
    let run = start(&st, brainstorm()).await;
    ask_and_finish(&st, &run, "").await;
    st.svc.send_message(&run, "pakai dua-duanya").await.unwrap();
    let rec = st.questions.all(&run)[0].clone();
    assert_eq!(rec.status, QuestionStatus::AnsweredInChat);
    assert_eq!(rec.answer.as_deref(), Some("pakai dua-duanya"));
    assert_eq!(st.agent.last().req.message, "pakai dua-duanya");
}

#[tokio::test]
async fn ae9_antrean_ditahan_lalu_dikirim_atau_dibuang() {
    let (lark, clock) = setup();
    let st = station(&lark, &clock, "ou_me");
    let run = start(&st, brainstorm()).await;
    st.svc.send_message(&run, "catatan 1").await.unwrap();
    st.svc.send_message(&run, "catatan 2").await.unwrap();
    ask_and_finish(&st, &run, "").await;
    assert_eq!(st.agent.count(), 1, "antrean tidak dikirim otomatis");
    assert_eq!(st.runs.only().queue, ["catatan 1", "catatan 2"]);

    st.svc.discard_queued(&run).await.unwrap();
    assert!(st.runs.only().queue.is_empty());
    assert_eq!(st.questions.all(&run)[0].status, QuestionStatus::Pending);
    assert!(st.svc.send_queued(&run).await.is_err(), "antrean kosong");
}

#[tokio::test]
async fn ae9_kirim_antrean_menjawab_lewat_chat() {
    let (lark, clock) = setup();
    let st = station(&lark, &clock, "ou_me");
    let run = start(&st, brainstorm()).await;
    st.svc.send_message(&run, "catatan 1").await.unwrap();
    st.svc.send_message(&run, "catatan 2").await.unwrap();
    ask_and_finish(&st, &run, "").await;
    st.svc.send_queued(&run).await.unwrap();
    assert_eq!(st.agent.count(), 2);
    assert_eq!(st.agent.last().req.message, "catatan 1\n\ncatatan 2");
    assert!(st.runs.only().queue.is_empty());
    assert_eq!(
        st.questions.all(&run)[0].status,
        QuestionStatus::AnsweredInChat
    );
}

#[tokio::test]
async fn klik_saat_antrean_ditahan_lalu_antrean_terkuras_normal() {
    let (lark, clock) = setup();
    let st = station(&lark, &clock, "ou_me");
    let run = start(&st, brainstorm()).await;
    st.svc.send_message(&run, "catatan").await.unwrap();
    let q = ask_and_finish(&st, &run, "").await;
    st.svc.answer_question(&q, 0).await.unwrap();
    assert_eq!(st.agent.last().req.message, "Keep both");
    st.agent.last().finish("lanjut");
    eventually(|| st.agent.count() == 3).await;
    assert_eq!(st.agent.last().req.message, "catatan");
}

#[tokio::test]
async fn ae10_continue_to_plan_melewati_kartu() {
    let (lark, clock) = setup();
    let st = station(&lark, &clock, "ou_me");
    let run = start(&st, brainstorm()).await;
    ask_and_finish(&st, &run, "").await;
    st.svc.continue_to_plan(&run).await.unwrap();
    assert_eq!(st.runs.only().phase, RunPhase::Plan);
    assert_eq!(st.questions.all(&run)[0].status, QuestionStatus::Skipped);
}

#[tokio::test]
async fn giliran_tanpa_pertanyaan_tetap_seperti_dulu() {
    let (lark, clock) = setup();
    let st = station(&lark, &clock, "ou_me");
    let run = start(&st, plan()).await;
    st.svc.send_message(&run, "antre").await.unwrap();
    st.agent.last().finish(PLAN);
    eventually(|| st.agent.count() == 2).await;
    assert_eq!(st.runs.only().plan.as_deref(), Some(PLAN));
    assert_eq!(st.agent.last().req.message, "antre");
}

#[tokio::test]
async fn pertanyaan_lama_dilewati_saat_giliran_baru_bertanya() {
    let (lark, clock) = setup();
    let st = station(&lark, &clock, "ou_me");
    let run = start(&st, brainstorm()).await;
    st.svc.ask_question(&scope(&st), &draft()).await.unwrap();
    // Giliran dihentikan dev tanpa menjawab, lalu giliran baru dari pesan
    // lain yang tidak melewati jalur chat (mis. Stop lalu Send now).
    st.svc.stop(&run).await.unwrap();
    eventually(|| !st.runs.only().state.is_busy()).await;
    assert_eq!(st.questions.all(&run)[0].status, QuestionStatus::Pending);
    st.svc.send_message(&run, "lanjut").await.unwrap();
    st.svc.ask_question(&scope(&st), &draft()).await.unwrap();
    let all = st.questions.all(&run);
    assert_eq!(all.len(), 2);
    assert_eq!(all[0].status, QuestionStatus::AnsweredInChat);
    assert_eq!(all[1].status, QuestionStatus::Pending);
}

#[tokio::test]
async fn send_now_tidak_mencabut_token_giliran_baru() {
    let (lark, clock) = setup();
    let st = station(&lark, &clock, "ou_me");
    let run = start(&st, brainstorm()).await;
    let old = scope(&st);
    st.svc.send_now(&run, "ganti arah").await.unwrap();
    assert_eq!(st.agent.count(), 2);
    let token = st.agent.last().req.ledger.unwrap().token;
    // Giliran lama sudah berhenti dan selesai diproses.
    eventually(|| st.agent.run(0).is_stopped()).await;
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    let new = st
        .svc
        .verify_capability(token.expose())
        .expect("token baru berlaku");
    assert_eq!(new.run_id, old.run_id);
    st.svc.ask_question(&new, &draft()).await.unwrap();
    assert_eq!(st.questions.all(&run).len(), 1);
}

#[tokio::test]
async fn token_giliran_yang_dihentikan_dicabut_bila_tidak_ada_giliran_baru() {
    let (lark, clock) = setup();
    let st = station(&lark, &clock, "ou_me");
    let run = start(&st, brainstorm()).await;
    let token = st.agent.last().req.ledger.unwrap().token;
    st.svc.stop(&run).await.unwrap();
    eventually(|| st.agent.run(0).is_stopped()).await;
    eventually(|| st.svc.verify_capability(token.expose()).is_none()).await;
}
