//! Pemetaan tipe domain/usecase ke DTO protokol.

use mandor_domain::gate::check_start;
use mandor_domain::{ClaimIdentity, Holder, Task, TaskStatus};
use mandor_protocol::{
    AcView, CardView, FlagView, GroupCardView, NoticeView, RunRepoView, RunView, SprintView,
    TaskDetailsView,
};
use mandor_usecase::run_ports::{GroupCardRow, RunRecord};
use mandor_usecase::{CachedTask, SprintItem, TaskDetails};

#[must_use]
pub fn run_view(run: &RunRecord, task: Option<&Task>) -> RunView {
    let mut repos = vec![RunRepoView {
        repo_id: run.repo_id.clone(),
        pr_url: run.pr_url.clone(),
    }];
    repos.extend(run.extra.iter().map(|m| RunRepoView {
        repo_id: m.repo_id.clone(),
        pr_url: m.pr_url.clone(),
    }));
    let urls: Vec<&str> = repos.iter().filter_map(|r| r.pr_url.as_deref()).collect();
    RunView {
        id: run.id.clone(),
        record_id: run.record_id.clone(),
        repo_id: run.repo_id.clone(),
        key: task.and_then(|t| t.key.clone()),
        title: task.map(|t| t.title.clone()).unwrap_or_default(),
        state: run.state.as_str().to_owned(),
        branch: run.branch.clone(),
        pr_url: (!urls.is_empty()).then(|| urls.join("\n")),
        workspace_id: run.workspace.as_ref().map(|w| w.id.clone()),
        repos,
        phase: run.phase.as_str().to_owned(),
        plan: run.plan.clone(),
        last_error: run.last_error.clone(),
        note: run.note.clone(),
        draft: run.draft_pending.is_some(),
        queued: run.queue.clone(),
        automatic: run.automatic,
        quiet_until_ms: run.quiet_until_ms,
        started_at_ms: run.created_at_ms,
        updated_at_ms: run.updated_at_ms,
        ac: None,
        cards: Vec::new(),
        one_plan: run.one_plan.is_some(),
    }
}

/// Kartu run grup untuk UI; kunci dan judul dari task bila ada di cache.
#[must_use]
pub fn group_card_view(c: &GroupCardRow, task: Option<&Task>) -> GroupCardView {
    GroupCardView {
        record_id: c.record_id.clone(),
        key: task.and_then(|t| t.key.clone()),
        title: task.map(|t| t.title.clone()).unwrap_or_default(),
        state: c.state.as_str().to_owned(),
        changed_in_lark: c.changed_in_lark,
        kept_reason: c.kept_reason.clone(),
        in_fix: c.in_fix,
    }
}

/// Kartu papan yang menjadi bagian run grup: Start nonaktif dengan alasan
/// grupnya dan run membawa daftar kartu (U8 plan run grup).
pub fn apply_group(card: &mut CardView, cards: &[GroupCardView]) {
    if cards.is_empty() {
        return;
    }
    let keys: Vec<&str> = cards
        .iter()
        .filter(|c| !c.state.starts_with("removed"))
        .map(|c| c.key.as_deref().unwrap_or(&c.record_id))
        .collect();
    let label = match keys.as_slice() {
        [] => String::new(),
        [one] => (*one).to_owned(),
        [first, .., last] => format!("{first}–{last}"),
    };
    card.start_blocked = Some(format!("In group {label}"));
    if let Some(run) = card.run.as_mut() {
        run.cards = cards.to_vec();
    }
}

/// Ringkasan buku AC; `None` bila tidak ada baris aktif.
#[must_use]
pub fn ac_summary(rows: &[mandor_domain::AcRow]) -> Option<mandor_protocol::AcSummaryView> {
    use mandor_domain::{Design, RowStatus, Verdict};
    let active: Vec<_> = rows
        .iter()
        .filter(|r| r.status != RowStatus::Removed)
        .collect();
    if active.is_empty() {
        return None;
    }
    let count = |f: &dyn Fn(&mandor_domain::AcRow) -> bool| {
        u32::try_from(active.iter().filter(|r| f(r)).count()).unwrap_or(u32::MAX)
    };
    Some(mandor_protocol::AcSummaryView {
        total: count(&|_| true),
        ok: count(&|r| {
            r.design == Design::FollowDesign
                || matches!(
                    r.verdict,
                    Verdict::Pass | Verdict::CheckedManually | Verdict::Skipped
                )
        }),
        failed: count(&|r| r.in_gate() && r.verdict == Verdict::Fail),
        unverified: count(&|r| r.in_gate() && r.verdict == Verdict::CantVerify),
        not_reviewed: count(&|r| {
            r.in_gate() && matches!(r.verdict, Verdict::NotReviewed | Verdict::FixInProgress)
        }),
        stale: active.iter().any(|r| r.stale),
        design_gaps: count(&|r| r.design.is_gap()),
    })
}

/// Alasan Start dinonaktifkan untuk kartu ini, bila ada.
fn start_blocked(task: &Task, run: Option<&RunRecord>, me: &str, now_ms: i64) -> Option<String> {
    if run.is_some() {
        return Some("Task already has an active run".to_owned());
    }
    if let Some(c) = &task.claim {
        if c.open_id != me && !c.is_stale(now_ms) {
            return Some(format!("Being worked on by {}", c.open_id));
        }
    }
    check_start(&task.status, task.readiness.as_deref())
        .err()
        .map(|e| e.to_string())
}

#[must_use]
pub fn card_view(c: &CachedTask, run: Option<&RunRecord>, me: &str, now_ms: i64) -> CardView {
    let t = &c.task;
    let owner = t.claim.as_ref().map(|cl| cl.open_id.clone());
    let owner_active = t.claim.as_ref().is_some_and(|cl| !cl.is_stale(now_ms));
    let mine = t.claimed_by(me);
    CardView {
        record_id: t.record_id.clone(),
        key: t.key.clone(),
        title: t.title.clone(),
        status: t.status.as_str().to_owned(),
        column: match t.column().mirror() {
            Some(m) => m.to_owned(),
            None => "unknown".to_owned(),
        },
        readiness: t.readiness.clone(),
        start_blocked: start_blocked(t, run, me, now_ms),
        sprint_ids: t.sprint_ids.clone(),
        assignees: t.assignees.clone(),
        team: false,
        owner,
        owner_active,
        mine,
        can_takeover: t.claim.is_some() && !mine && !owner_active && t.status != TaskStatus::ToDo,
        flag: c.flag.as_ref().map(|f| FlagView {
            kind: f.kind.as_str().to_owned(),
            detail: f.detail.clone(),
        }),
        run: run.map(|r| run_view(r, Some(t))),
        holder: None,
        pr_url: t.pr_url.clone(),
        branch: t.branch.clone(),
        repo_id: t.repo_id.clone(),
        last_modified_ms: t.last_modified_ms,
    }
}

/// Pemegang klaim kartu dari sudut daemon ini (KTD-56 plan VM). Kartu yang
/// dipegang daemon lain milik dev yang sama hanya-baca: Start dan ambil alih
/// nonaktif (R16).
pub fn apply_holder(card: &mut CardView, task: &Task, me: &ClaimIdentity) {
    let Some(holder) = task.holder(me) else {
        return;
    };
    card.holder = Some(holder.as_str().to_owned());
    if holder == Holder::OwnOtherDaemon {
        let daemon = task
            .claim
            .as_ref()
            .and_then(|c| c.daemon_id())
            .unwrap_or("another daemon");
        card.start_blocked = Some(format!("Held by your other Mandor daemon ({daemon})"));
        card.can_takeover = false;
    }
}

/// Kartu rekan di papan Team: hanya baca.
#[must_use]
pub fn team_card_view(t: &Task, me: &str, now_ms: i64) -> CardView {
    let cached = CachedTask {
        task: t.clone(),
        flag: None,
    };
    let mut v = card_view(&cached, None, me, now_ms);
    v.team = true;
    v.can_takeover = false;
    v.start_blocked = Some("Assigned to another developer".to_owned());
    v
}

#[must_use]
pub fn sprint_view(s: &SprintItem) -> SprintView {
    SprintView {
        id: s.record_id.clone(),
        name: s.name.clone(),
        start_ms: s.start_ms,
        end_ms: s.end_ms,
        status: s.status.clone(),
    }
}

/// Notifikasi dari riwayat; `None` untuk payload yang tidak bisa dibaca.
#[must_use]
pub fn notice_view(n: &mandor_store::Notification) -> Option<NoticeView> {
    let mut v: NoticeView = serde_json::from_value(n.payload.clone()).ok()?;
    v.id = n.id;
    v.read = n.read;
    Some(v)
}

#[must_use]
pub fn task_details_view(d: &TaskDetails) -> TaskDetailsView {
    let t = &d.task;
    TaskDetailsView {
        record_id: t.record_id.clone(),
        key: t.key.clone(),
        title: t.title.clone(),
        description: t.description.clone(),
        status: t.status.as_str().to_owned(),
        readiness: t.readiness.clone(),
        assignees: t.assignees.clone(),
        reporter: t.reporter.clone(),
        sprint_ids: t.sprint_ids.clone(),
        acs: d
            .acs
            .iter()
            .map(|ac| AcView {
                id: ac.record_id.clone(),
                label: ac.label.clone(),
                text: ac.text.clone(),
                dropped: ac.dropped,
            })
            .collect(),
    }
}

/// Baris buku AC untuk UI.
pub fn ac_row_view(r: &mandor_domain::AcRow) -> mandor_protocol::AcRowView {
    mandor_protocol::AcRowView {
        ac_id: r.ac_id.clone(),
        label: r.label.clone(),
        text: r.text.clone(),
        design: r.design.as_str().to_owned(),
        design_note: r.design_note.clone(),
        verdict: r.verdict.as_str().to_owned(),
        evidence: r.evidence.clone(),
        stale: r.stale,
        status: r.status.as_str().to_owned(),
        reason: r.reason.clone(),
        decided_by: r.decided_by.clone(),
        in_gate: r.in_gate(),
        selectable: r.selectable(),
        card: r.card.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mandor_domain::Claim;
    use mandor_domain::claim::STALE_MS;
    use mandor_usecase::AcItem;

    fn cached(t: Task) -> CachedTask {
        CachedTask {
            task: t,
            flag: None,
        }
    }

    #[test]
    fn kartu_milik_dev_lain_read_only_dan_ambil_alih_saat_stale() {
        let mut t = Task::new("rec1", TaskStatus::InProgress);
        t.claim = Some(Claim {
            open_id: "ou_b".to_owned(),
            ts_ms: 0,
            nonce: "n".to_owned(),
        });
        t.assignees = vec!["ou_a".to_owned(), "ou_b".to_owned()];
        let active = card_view(&cached(t.clone()), None, "ou_a", 1);
        assert!(!active.mine && active.owner_active && !active.can_takeover);
        assert_eq!(
            active.assignees,
            ["ou_a", "ou_b"],
            "filter Mine butuh assignee"
        );
        assert!(active.start_blocked.unwrap().contains("ou_b"));
        let stale = card_view(&cached(t), None, "ou_a", STALE_MS + 1);
        assert!(stale.can_takeover);
    }

    #[test]
    fn kartu_grup_menampilkan_run_grup_dan_alasan_start() {
        let mut t = Task::new("c2", TaskStatus::InProgress);
        t.key = Some("PGS-1402".to_owned());
        let mut r = RunRecord::new("g".to_owned(), "repo1".to_owned(), "c1".to_owned());
        r.state = mandor_domain::RunState::Running;
        let mut card = card_view(&cached(t.clone()), Some(&r), "ou_me", 0);
        let rows: Vec<GroupCardRow> = ["c1", "c2", "c3"]
            .iter()
            .map(|id| GroupCardRow::pending(id))
            .collect();
        let views: Vec<GroupCardView> = rows
            .iter()
            .enumerate()
            .map(|(i, c)| {
                let mut task = Task::new(&c.record_id, TaskStatus::InProgress);
                task.key = Some(format!("PGS-140{}", i + 1));
                group_card_view(c, Some(&task))
            })
            .collect();
        apply_group(&mut card, &views);
        assert_eq!(
            card.start_blocked.as_deref(),
            Some("In group PGS-1401–PGS-1403")
        );
        let run = card.run.unwrap();
        assert_eq!(run.cards.len(), 3);
        assert_eq!(run.cards[1].key.as_deref(), Some("PGS-1402"));
        assert_eq!(run.cards[1].state, "pending");

        let mut single = card_view(&cached(t), Some(&r), "ou_me", 0);
        apply_group(&mut single, &[]);
        assert_eq!(
            single.start_blocked.as_deref(),
            Some("Task already has an active run"),
            "run tunggal tidak berubah"
        );
    }

    #[test]
    fn kartu_yang_dipegang_daemon_vm_hanya_baca() {
        use mandor_domain::Claim;
        let laptop = ClaimIdentity::new("ou_me", "laptop", true);
        let mut t = Task::new("rec1", TaskStatus::WaitingFix);
        t.claim = Some(Claim {
            open_id: "ou_me".to_owned(),
            ts_ms: 0,
            nonce: "r~vm".to_owned(),
        });
        let mut card = card_view(&cached(t.clone()), None, "ou_me", 0);
        apply_holder(&mut card, &t, &laptop);
        assert_eq!(card.holder.as_deref(), Some("own_other_daemon"));
        assert_eq!(
            card.start_blocked.as_deref(),
            Some("Held by your other Mandor daemon (vm)")
        );
        assert!(!card.can_takeover);

        t.claim = Some(Claim {
            open_id: "ou_me".to_owned(),
            ts_ms: 0,
            nonce: "r~laptop".to_owned(),
        });
        let mut own = card_view(&cached(t.clone()), None, "ou_me", 0);
        apply_holder(&mut own, &t, &laptop);
        assert_eq!(own.holder.as_deref(), Some("self"));
        t.claim = None;
        let mut free = card_view(&cached(t.clone()), None, "ou_me", 0);
        apply_holder(&mut free, &t, &laptop);
        assert_eq!(free.holder, None);
    }

    #[test]
    fn kartu_tim_hanya_baca() {
        let mut t = Task::new("recB", TaskStatus::ToDo);
        t.assignees = vec!["ou_b".to_owned()];
        let v = team_card_view(&t, "ou_a", 1);
        assert!(v.team && !v.mine && !v.can_takeover);
        assert_eq!(
            v.start_blocked.as_deref(),
            Some("Assigned to another developer")
        );
        assert!(!card_view(&cached(t), None, "ou_a", 1).team);
    }

    #[test]
    fn notifikasi_dari_riwayat_membawa_id_dan_status_baca() {
        let payload = serde_json::json!({
            "kind": "failed", "record_id": "rec1", "run_id": null,
            "title": "PGS-1 failed", "body": "x", "created_at_ms": 5
        });
        let n = mandor_store::Notification {
            id: 7,
            kind: "failed".to_owned(),
            payload,
            created_at_ms: 5,
            read: true,
        };
        let v = notice_view(&n).unwrap();
        assert_eq!((v.id, v.read, v.title.as_str()), (7, true, "PGS-1 failed"));
        let rusak = mandor_store::Notification {
            payload: serde_json::json!({}),
            ..n
        };
        assert!(notice_view(&rusak).is_none());
    }

    #[test]
    fn sprint_membawa_nama_tanggal_dan_status() {
        let v = sprint_view(&SprintItem {
            record_id: "spr1".to_owned(),
            name: "Sprint 42".to_owned(),
            start_ms: Some(1),
            end_ms: None,
            status: Some("In Progress".to_owned()),
        });
        assert_eq!(v.id, "spr1");
        assert_eq!(v.name, "Sprint 42");
        assert_eq!((v.start_ms, v.end_ms), (Some(1), None));
        assert_eq!(v.status.as_deref(), Some("In Progress"));
    }

    #[test]
    fn run_workspace_membawa_semua_repo_dan_link_pr() {
        use mandor_usecase::run_ports::{RunMember, RunWorkspace};
        let mut r = RunRecord::new("a".to_owned(), "api".to_owned(), "rec1".to_owned());
        r.workspace = Some(RunWorkspace {
            id: "kelola-v2".to_owned(),
            root: "/wt/k".into(),
        });
        r.extra = vec![RunMember {
            repo_id: "web".to_owned(),
            worktree: "/wt/k/web".into(),
            pr_url: Some("https://x/web/pull/2".to_owned()),
        }];
        r.phase = mandor_usecase::run_ports::RunPhase::Plan;
        r.plan = Some("## Plan".to_owned());
        let v = run_view(&r, None);
        assert_eq!(
            (v.phase.as_str(), v.plan.as_deref()),
            ("plan", Some("## Plan"))
        );
        assert_eq!(v.workspace_id.as_deref(), Some("kelola-v2"));
        assert_eq!(v.repos.len(), 2);
        assert_eq!(v.repos[0].pr_url, None);
        assert_eq!(v.pr_url.as_deref(), Some("https://x/web/pull/2"));
        r.pr_url = Some("https://x/api/pull/1".to_owned());
        assert_eq!(
            run_view(&r, None).pr_url.as_deref(),
            Some("https://x/api/pull/1\nhttps://x/web/pull/2")
        );
    }

    #[test]
    fn run_membawa_waktu_mulai_dan_perubahan_terakhir() {
        let mut r = RunRecord::new("a".to_owned(), "repo1".to_owned(), "rec1".to_owned());
        (r.created_at_ms, r.updated_at_ms) = (10, 20);
        r.quiet_until_ms = Some(30);
        let v = run_view(&r, None);
        assert_eq!((v.started_at_ms, v.updated_at_ms), (10, 20));
        assert_eq!(v.quiet_until_ms, Some(30));
        assert!(!v.draft);
        r.draft_pending = Some(40);
        assert!(run_view(&r, None).draft, "PR draft menunggu Send to review");
    }

    #[test]
    fn detail_task_membawa_deskripsi_orang_dan_ac_yang_di_drop() {
        let mut t = Task::new("rec1", TaskStatus::InProgress);
        t.key = Some("PGS-1".to_owned());
        "Deskripsi".clone_into(&mut t.description);
        t.assignees = vec!["ou_a".to_owned()];
        t.reporter = Some("ou_r".to_owned());
        let v = task_details_view(&TaskDetails {
            task: t,
            acs: vec![AcItem {
                record_id: "ac1".to_owned(),
                label: Some("AC-1".to_owned()),
                text: "Nama tampil".to_owned(),
                dropped: true,
                ..Default::default()
            }],
        });
        assert_eq!(v.key.as_deref(), Some("PGS-1"));
        assert_eq!(v.description, "Deskripsi");
        assert_eq!(v.status, "In Progress");
        assert_eq!(v.assignees, ["ou_a"]);
        assert_eq!(v.reporter.as_deref(), Some("ou_r"));
        assert_eq!(v.acs[0].label.as_deref(), Some("AC-1"));
        assert!(v.acs[0].dropped);
    }

    #[test]
    fn start_nonaktif_dengan_alasan_readiness_dan_kolom_tak_dikenal() {
        let mut t = Task::new("rec1", TaskStatus::ToDo);
        t.readiness = Some("Review AC By PM".to_owned());
        let v = card_view(&cached(t.clone()), None, "ou_a", 1);
        assert!(v.start_blocked.unwrap().contains("Review AC By PM"));
        t.readiness = Some("Ready To Dev".to_owned());
        assert_eq!(
            card_view(&cached(t.clone()), None, "ou_a", 1).start_blocked,
            None
        );
        t.status = TaskStatus::Unknown("Blocked".to_owned());
        assert_eq!(card_view(&cached(t), None, "ou_a", 1).column, "unknown");
    }

    #[test]
    fn ringkasan_ac_menghitung_baris_aktif_dan_mengabaikan_removed() {
        use mandor_domain::{AcRow, Design, RowStatus, Verdict};
        assert_eq!(ac_summary(&[]), None);
        let row = |id: &str, v: Verdict| {
            let mut r = AcRow::new(id, None, "t", RowStatus::Active);
            r.verdict = v;
            r
        };
        let mut follow = row("5", Verdict::NotReviewed);
        follow.design = Design::FollowDesign;
        let mut gap = row("6", Verdict::FixInProgress);
        gap.design = Design::Missing;
        gap.stale = true;
        let mut removed = row("7", Verdict::Fail);
        removed.status = RowStatus::Removed;
        let rows = [
            row("1", Verdict::Pass),
            row("2", Verdict::Fail),
            row("3", Verdict::CantVerify),
            row("4", Verdict::Skipped),
            follow,
            gap,
            removed,
        ];
        let s = ac_summary(&rows).unwrap();
        assert_eq!(
            (s.total, s.ok, s.failed, s.unverified, s.not_reviewed),
            (6, 3, 1, 1, 1)
        );
        assert!(s.stale);
        assert_eq!(s.design_gaps, 1);
        let only_removed = [rows[6].clone()];
        assert_eq!(ac_summary(&only_removed), None);
    }
}
