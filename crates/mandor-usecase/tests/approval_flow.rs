//! Kartu persetujuan izin agent (U4 plan 2026-11-06-001).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::sync::Arc;

use common::run_fakes::eventually;
use common::{FakeClock, FakeLark, Station, station};
use mandor_domain::permit::TurnKind;
use mandor_usecase::approval::{ApprovalError, PermitAnswer, PermitRequest};
use mandor_usecase::run_ports::{ApprovalAnswer, ApprovalStatus, ChatItem, PermitRef};
use pretty_assertions::assert_eq;
use serde_json::json;

const RUN: &str = "run1";

fn setup() -> Station {
    let clock = FakeClock::default();
    clock.set(1_000_000);
    station(&FakeLark::default(), &clock, "ou_me")
}

fn req(st: &Station, id: &str, cmd: &str, turn: TurnKind) -> PermitRequest {
    st.permits.checked(id, "/w/api");
    PermitRequest {
        run_id: RUN.to_owned(),
        turn,
        permit: PermitRef {
            path: "/p/run.json".to_owned(),
            sha: "0".repeat(64),
        },
        tool: "Bash".to_owned(),
        input: json!({"command": cmd, "description": "langkah"}),
        tool_use_id: id.to_owned(),
    }
}

/// Menjalankan permintaan di latar; mengembalikan handle hasilnya.
fn ask(st: &Station, r: PermitRequest) -> tokio::task::JoinHandle<PermitAnswer> {
    let svc = Arc::clone(&st.svc);
    tokio::spawn(async move { svc.permit_request(r).await })
}

/// Menunggu kartu Pending ke-`n` lalu mengembalikan id-nya.
async fn pending_card(st: &Station, n: usize) -> String {
    eventually(|| st.approvals.pending(RUN).len() >= n && st.waiter.waiting() >= n).await;
    st.approvals.pending(RUN)[n - 1].id.clone()
}

fn chat(st: &Station) -> Vec<String> {
    st.runs
        .chat
        .lock()
        .unwrap()
        .iter()
        .filter_map(|(_, item)| match item {
            ChatItem::System(t) => Some(t.clone()),
            _ => None,
        })
        .collect()
}

fn denied(a: &PermitAnswer) -> bool {
    matches!(a, PermitAnswer::Deny(_))
}

#[tokio::test]
async fn ae3_izin_run_berlaku_untuk_permintaan_persis_sama() {
    let st = setup();
    let first = ask(
        &st,
        req(&st, "t1", "jq . package.json", TurnKind::Implement),
    );
    let card = pending_card(&st, 1).await;
    st.svc
        .answer_approval(RUN, &card, ApprovalAnswer::AllowRun)
        .await
        .unwrap();
    let input = json!({"command": "jq . package.json", "description": "langkah"});
    assert_eq!(first.await.unwrap(), PermitAnswer::Allow(input.clone()));

    // Deskripsi berbeda tetap permintaan yang sama.
    let mut again = req(&st, "t2", "jq . package.json", TurnKind::Implement);
    again.input["description"] = json!("lain");
    assert!(matches!(
        st.svc.permit_request(again).await,
        PermitAnswer::Allow(_)
    ));
    assert_eq!(
        st.approvals.cards.lock().unwrap().len(),
        1,
        "tanpa kartu baru"
    );

    let other = ask(
        &st,
        req(&st, "t3", "jq .name package.json", TurnKind::Implement),
    );
    let card = pending_card(&st, 1).await;
    st.svc
        .answer_approval(RUN, &card, ApprovalAnswer::AllowOnce)
        .await
        .unwrap();
    assert!(matches!(other.await.unwrap(), PermitAnswer::Allow(_)));
}

#[tokio::test]
async fn ae14_izin_giliran_implementasi_tidak_berlaku_di_plan() {
    let st = setup();
    let first = ask(&st, req(&st, "t1", "jq . a.json", TurnKind::Implement));
    let card = pending_card(&st, 1).await;
    st.svc
        .answer_approval(RUN, &card, ApprovalAnswer::AllowRun)
        .await
        .unwrap();
    first.await.unwrap();
    let plan = ask(&st, req(&st, "t2", "jq . a.json", TurnKind::Plan));
    pending_card(&st, 1).await;
    st.svc.cancel_run_approvals(RUN).await;
    assert!(denied(&plan.await.unwrap()));
    // Reviewer tidak pernah mendapat kartu.
    let review = st
        .svc
        .permit_request(req(&st, "t3", "jq . a.json", TurnKind::Review))
        .await;
    assert!(denied(&review));
}

#[tokio::test]
async fn ae4_expired_menyalakan_tolak_langsung_sampai_chat_dibuka() {
    let st = setup();
    let first = ask(&st, req(&st, "t1", "docker ps", TurnKind::Implement));
    let card = pending_card(&st, 1).await;
    st.waiter.time_out(&card);
    let answer = first.await.unwrap();
    assert!(denied(&answer), "{answer:?}");
    let stored = st.approvals.cards.lock().unwrap()[0].clone();
    assert_eq!(stored.status, ApprovalStatus::Expired);
    assert_eq!(
        *st.waiter.timeouts.lock().unwrap(),
        [std::time::Duration::from_secs(600)]
    );

    let next = st
        .svc
        .permit_request(req(&st, "t2", "jq . a.json", TurnKind::Implement))
        .await;
    assert!(denied(&next));
    assert_eq!(st.approvals.cards.lock().unwrap().len(), 1, "tanpa kartu");
    assert!(
        chat(&st).iter().any(|t| t.contains("expired")),
        "{:?}",
        chat(&st)
    );

    st.svc.approvals_seen(RUN).await.unwrap();
    assert!(chat(&st).iter().any(|t| t.contains("resumed")));
    let later = ask(&st, req(&st, "t3", "jq . a.json", TurnKind::Implement));
    pending_card(&st, 1).await;
    st.svc.cancel_run_approvals(RUN).await;
    later.await.unwrap();
}

#[tokio::test]
async fn ae13_deny_berlaku_lintas_giliran() {
    let st = setup();
    let first = ask(&st, req(&st, "t1", "docker compose up -d", TurnKind::Plan));
    let card = pending_card(&st, 1).await;
    st.svc
        .answer_approval(RUN, &card, ApprovalAnswer::Deny)
        .await
        .unwrap();
    assert!(denied(&first.await.unwrap()));
    let again = st
        .svc
        .permit_request(req(&st, "t2", "docker compose up -d", TurnKind::Implement))
        .await;
    match again {
        PermitAnswer::Deny(r) => assert!(r.contains("denied this exact request"), "{r}"),
        PermitAnswer::Allow(_) => panic!("harus ditolak"),
    }
    assert_eq!(st.approvals.cards.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn ae12_jawaban_untuk_kartu_yang_tidak_menunggu_ditolak() {
    let st = setup();
    let first = ask(&st, req(&st, "t1", "docker ps", TurnKind::Implement));
    let card = pending_card(&st, 1).await;
    st.waiter.time_out(&card);
    first.await.unwrap();
    assert_eq!(
        st.svc
            .answer_approval(RUN, &card, ApprovalAnswer::AllowOnce)
            .await,
        Err(ApprovalError::NoLongerPending)
    );
    assert_eq!(
        st.svc
            .answer_approval("run-lain", &card, ApprovalAnswer::AllowOnce)
            .await,
        Err(ApprovalError::NotFound)
    );
}

#[tokio::test]
async fn jawaban_menang_tepat_di_tenggat() {
    let st = setup();
    let first = ask(&st, req(&st, "t1", "jq . a.json", TurnKind::Implement));
    let card = pending_card(&st, 1).await;
    // Jawaban tersimpan dulu, lalu tenggat terlewati sebelum penunggu bangun.
    st.svc
        .answer_approval(RUN, &card, ApprovalAnswer::AllowRun)
        .await
        .unwrap();
    st.waiter.time_out(&card);
    assert!(matches!(first.await.unwrap(), PermitAnswer::Allow(_)));
    let stored = st.approvals.cards.lock().unwrap()[0].clone();
    assert_eq!(stored.status, ApprovalStatus::Answered);
    assert_eq!(
        st.approvals.decisions.lock().unwrap().len(),
        1,
        "izin run tertulis"
    );
    assert!(!st.approvals.deny_mode.lock().unwrap().contains(RUN));
}

#[tokio::test]
async fn ae2_larangan_keras_dan_tanpa_bukti_hook_ditolak_tanpa_kartu() {
    let st = setup();
    let push = st
        .svc
        .permit_request(req(&st, "t1", "git push origin HEAD", TurnKind::Implement))
        .await;
    assert!(denied(&push));
    let mut unchecked = req(&st, "t2", "docker ps", TurnKind::Implement);
    unchecked.tool_use_id = "tidak-dikenal".to_owned();
    assert!(denied(&st.svc.permit_request(unchecked).await));
    assert!(st.approvals.cards.lock().unwrap().is_empty());
    let notes = chat(&st);
    assert!(notes.iter().any(|t| t.contains("git push")), "{notes:?}");
}

#[tokio::test]
async fn batas_kartu_dan_ukuran_input() {
    let st = setup();
    let mut waiting = Vec::new();
    for i in 1..=5 {
        waiting.push(ask(
            &st,
            req(
                &st,
                &format!("t{i}"),
                &format!("docker ps {i}"),
                TurnKind::Implement,
            ),
        ));
        pending_card(&st, i).await;
    }
    let sixth = st
        .svc
        .permit_request(req(&st, "t6", "docker ps 6", TurnKind::Implement))
        .await;
    assert!(denied(&sixth));
    let big = "x".repeat(65 * 1024);
    let large = st
        .svc
        .permit_request(req(&st, "t7", &format!("echo {big}"), TurnKind::Implement))
        .await;
    assert!(denied(&large));
    assert_eq!(st.approvals.cards.lock().unwrap().len(), 5);

    // Kartu bersamaan urut waktu dibuat, masing-masing dengan tenggatnya.
    let cards = st.svc.list_approvals(RUN).await.unwrap();
    assert!(cards.windows(2).all(|w| w[0].created_ms <= w[1].created_ms));
    assert!(
        cards
            .iter()
            .all(|c| c.deadline_ms == c.created_ms + 600_000)
    );

    st.svc.cancel_run_approvals(RUN).await;
    for w in waiting {
        assert!(denied(&w.await.unwrap()));
    }
    assert!(
        st.approvals
            .cards
            .lock()
            .unwrap()
            .iter()
            .all(|c| c.status == ApprovalStatus::Cancelled)
    );
}

#[tokio::test]
async fn daemon_start_membatalkan_kartu_lama() {
    let st = setup();
    let first = ask(&st, req(&st, "t1", "docker ps", TurnKind::Implement));
    pending_card(&st, 1).await;
    assert_eq!(st.svc.cancel_stale_approvals().await.unwrap(), 1);
    st.svc.cancel_run_approvals(RUN).await;
    assert!(denied(&first.await.unwrap()));
    assert_eq!(
        st.approvals.cards.lock().unwrap()[0].status,
        ApprovalStatus::Cancelled
    );
}

/// U5: token `Permit` hanya untuk giliran implementasi dan plan di laptop,
/// dan hanya bisa dipakai untuk meminta izin.
mod permit_token {
    use std::sync::atomic::Ordering;

    use super::{FakeClock, FakeLark, RUN, Station, TurnKind, denied, json, setup, station};
    use crate::common::{ready_task, vm_station};
    use mandor_usecase::review::{CapabilityRole, CapabilityScope};
    use mandor_usecase::{Planning, StartTarget};
    use pretty_assertions::assert_eq;

    const DAY: i64 = 24 * 60 * 60 * 1000;

    fn board() -> (FakeLark, FakeClock) {
        let lark = FakeLark::default();
        let clock = FakeClock::default();
        clock.set(10 * DAY);
        lark.clock.store(10 * DAY, Ordering::SeqCst);
        let mut t = ready_task("rec1", &["ou_me"]);
        t.last_modified_ms = 10 * DAY;
        lark.put(t);
        (lark, clock)
    }

    async fn start(st: &Station, planning: Planning) {
        st.svc
            .start_planned(
                "rec1",
                &StartTarget::Repo("repo1".to_owned()),
                None,
                &planning,
                &[],
                |_| {},
            )
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn giliran_laptop_mendapat_token_permit_dan_vm_tidak() {
        let (lark, clock) = board();
        let st = station(&lark, &clock, "ou_me");
        start(&st, Planning::Plan { skill: None }).await;
        let req = st.agent.last().req;
        let token = req.permit.clone().expect("token permit giliran plan");
        let scope = st.svc.verify_capability(token.expose()).unwrap();
        assert_eq!(scope.role, CapabilityRole::Permit);
        let grant = st.svc.permit_grant(&scope.run_id).unwrap();
        assert_eq!(grant.turn, TurnKind::Plan);
        assert!(grant.permit.path.ends_with("permit.json"));

        let (lark, clock) = board();
        let vm = vm_station(&lark, &clock, "ou_me");
        start(&vm, Planning::Plan { skill: None }).await;
        assert!(vm.agent.last().req.permit.is_none(), "VM tanpa kartu");
    }

    #[tokio::test]
    async fn token_ledger_tidak_bisa_meminta_izin() {
        let st = setup();
        let scope = CapabilityScope {
            run_id: RUN.to_owned(),
            role: CapabilityRole::Design,
        };
        let answer = st
            .svc
            .permit_request_scoped(
                &scope,
                "Bash".to_owned(),
                json!({"command": "ls"}),
                "t1".to_owned(),
            )
            .await;
        assert!(denied(&answer));
    }
}
