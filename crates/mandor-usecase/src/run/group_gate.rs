//! Gerbang Push & PR run grup (U6 plan run grup): pemeriksaan ulang kartu,
//! keputusan revert atau tetap, label kartu di gerbang AC, dan PR grup.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::sync::Arc;

use mandor_domain::ac_ledger::mark_stale;
use mandor_domain::group::{self, CardDecision, CardState, RemovedReason};
use mandor_domain::{AcRow, AppField, RunState, Task, TaskStatus, WriteOp};

use super::group::{group_cards_domain, key_of};
use super::members::{GroupPrCard, PrHeading};
use super::{RunError, RunService};
use crate::ports::{Clock, Deps, LarkTasks};
use crate::run_ports::{ChatItem, GitPort, GroupCardRow, LedgerStore, RunRecord, RunStore};

/// Hasil keputusan dev atas kartu yang berubah di Lark (R13).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CardOutcome {
    /// Kartu tetap disertakan; alasan dicatat di PR.
    Kept,
    /// Commit kartu di-revert dan kartu keluar dari grup.
    Reverted,
    /// Revert konflik dan dibatalkan; gerbang tetap tertahan.
    RevertConflict,
}

/// Alasan kartu dikeluarkan, untuk deskripsi PR.
fn removed_label(state: &CardState) -> Option<&'static str> {
    match state {
        CardState::Removed(RemovedReason::ChangedInLark) => Some("changed in Lark"),
        CardState::Removed(RemovedReason::ByDev) => Some("removed by the developer"),
        CardState::Removed(RemovedReason::Reverted) => Some("reverted"),
        CardState::Removed(RemovedReason::FollowUp) => {
            Some("fixed in a follow-up pull request after this one merged")
        }
        _ => None,
    }
}

/// Baris AC dengan kunci kartu di depan labelnya, untuk pesan gerbang grup.
pub(super) fn labelled_rows(rows: &[AcRow], keys: &HashMap<String, String>) -> Vec<AcRow> {
    rows.iter()
        .map(|r| {
            let mut r = r.clone();
            if let Some(key) = r.card.as_ref().and_then(|c| keys.get(c)) {
                r.label = Some(format!("{key} {}", r.display_label()));
            }
            r
        })
        .collect()
}

/// "Do not merge" selama ada kartu grup di Waiting Fix (R19).
fn fix_warning(active: &[&Task]) -> Option<String> {
    let keys: Vec<&str> = active
        .iter()
        .filter(|t| t.status == TaskStatus::WaitingFix)
        .map(|t| key_of(t))
        .collect();
    match keys.as_slice() {
        [] => None,
        [one] => Some(format!("Do not merge: {one} is in Waiting Fix.")),
        many => Some(format!(
            "Do not merge: {} are in Waiting Fix.",
            many.join(", ")
        )),
    }
}

impl<D: Deps> RunService<D> {
    /// Pemeriksaan gerbang run grup sebelum review AC (R12, R13). `None` untuk
    /// run tunggal. Kartu `Done` yang tidak lagi In Progress milik dev ditandai
    /// berubah. Mengembalikan kunci kartu yang menahan gerbang dan peta kunci
    /// kartu untuk label AC.
    ///
    /// # Errors
    /// Masih ada kartu yang belum selesai, atau Lark tidak terbaca.
    pub(super) async fn group_gate(
        &self,
        run: &mut RunRecord,
    ) -> Result<Option<(Vec<String>, HashMap<String, String>)>, RunError> {
        let mut cards = self.runs.cards(&run.id).await?;
        if cards.is_empty() {
            return Ok(None);
        }
        if !group::all_done(&group_cards_domain(&cards)) {
            return Err(RunError::InvalidState("finish every card first"));
        }
        let ids: Vec<String> = cards
            .iter()
            .filter(|c| c.state == CardState::Done)
            .map(|c| c.record_id.clone())
            .collect();
        let tasks = self.sync.lark.fetch(&ids).await?;
        let mut changed = false;
        for c in cards
            .iter_mut()
            .filter(|c| c.state == CardState::Done && !c.changed_in_lark)
        {
            let task = tasks.iter().find(|t| t.record_id == c.record_id);
            if !group::still_ours(task, self.sync.me()) {
                c.changed_in_lark = true;
                changed = true;
            }
        }
        if changed {
            self.save_group(run, &cards).await?;
        }
        let keys: HashMap<String, String> = tasks
            .iter()
            .map(|t| (t.record_id.clone(), key_of(t).to_owned()))
            .collect();
        let holds = group::gate_holds(&group_cards_domain(&cards))
            .into_iter()
            .map(|id| keys.get(id).cloned().unwrap_or_else(|| id.to_owned()))
            .collect();
        Ok(Some((holds, keys)))
    }

    /// Keputusan dev atas kartu yang berubah di Lark (R13): tetap dengan
    /// alasan, atau revert commit kartu. Revert konflik dibatalkan dan gerbang
    /// tetap tertahan dengan catatan "revert conflict".
    ///
    /// # Errors
    /// Bukan run grup, run tidak menunggu dev, kartu tidak menunggu keputusan,
    /// alasan kosong, atau git gagal.
    pub async fn gate_card_decision(
        self: &Arc<Self>,
        run_id: &str,
        record_id: &str,
        decision: &CardDecision,
    ) -> Result<CardOutcome, RunError> {
        let _work = self.enter_work()?;
        let mut run = self.load(run_id).await?;
        if run.state != RunState::AwaitingDev {
            return Err(RunError::InvalidState(run.state.as_str()));
        }
        let mut cards = self.group_of(run_id).await?;
        let idx = cards
            .iter()
            .position(|c| c.record_id == record_id)
            .ok_or(RunError::NotFound("card"))?;
        let card = &cards[idx];
        if card.state != CardState::Done || !card.changed_in_lark || card.kept_reason.is_some() {
            return Err(RunError::InvalidState(
                "the card is not waiting for a decision",
            ));
        }
        match decision {
            CardDecision::Keep { reason } => {
                let reason = reason.trim();
                if reason.is_empty() {
                    return Err(RunError::InvalidState("a reason is required"));
                }
                cards[idx].kept_reason = Some(reason.to_owned());
                self.save_group(&mut run, &cards).await?;
                Ok(CardOutcome::Kept)
            }
            CardDecision::Revert => self.revert_card(&mut run, &mut cards, idx).await,
        }
    }

    /// Revert commit kartu (terbaru dulu). Konflik pada commit mana pun
    /// menghentikan revert; commit yang sudah di-revert di repo lain tetap.
    async fn revert_card(
        &self,
        run: &mut RunRecord,
        cards: &mut [GroupCardRow],
        idx: usize,
    ) -> Result<CardOutcome, RunError> {
        let key = self.key_or_id(&cards[idx].record_id).await;
        let members = self.members(run).await?;
        for commit in cards[idx].commits.iter().rev() {
            let m = members.iter().find(|m| m.repo.id == commit.repo_id).ok_or(
                RunError::InvalidState("the card's repo is no longer in the run"),
            )?;
            if !self.git.revert_commit(&m.worktree, &commit.sha).await? {
                run.note = Some(format!(
                    "{key}: revert conflict. Resolve it in the branch, or keep the card with a reason."
                ));
                self.save_group(run, cards).await?;
                return Ok(CardOutcome::RevertConflict);
            }
        }
        cards[idx].state = CardState::Removed(RemovedReason::Reverted);
        self.release_quietly(&cards[idx].record_id).await;
        // Kode berubah: verdict yang ada tidak berlaku lagi.
        let mut rows = self.reviews.ac_rows(&run.id).await?;
        mark_stale(&mut rows);
        self.reviews.save_ac_rows(&run.id, &rows).await?;
        run.note = Some(format!(
            "{key} was reverted and removed from the group; review the AC again."
        ));
        self.save_group(run, cards).await?;
        self.runs
            .append(
                &run.id,
                &ChatItem::System(format!("Reverted {key}")),
                self.now(),
            )
            .await?;
        Ok(CardOutcome::Reverted)
    }

    /// Task kartu grup yang masih disertakan untuk brief review (R12); kosong
    /// untuk run tunggal.
    ///
    /// # Errors
    /// Penyimpanan atau Lark gagal.
    pub(crate) async fn review_cards(&self, run: &RunRecord) -> Result<Vec<Task>, RunError> {
        let ids: Vec<String> = self
            .runs
            .cards(&run.id)
            .await?
            .into_iter()
            .filter(GroupCardRow::is_open)
            .map(|c| c.record_id)
            .collect();
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let tasks = self.sync.lark.fetch(&ids).await?;
        Ok(ids
            .iter()
            .filter_map(|id| tasks.iter().find(|t| &t.record_id == id).cloned())
            .collect())
    }

    /// Awalan commit perbaikan run grup: kunci kartu yang komentarnya dikirim
    /// pada siklus Waiting Fix ini (KTD-G3). `None` untuk run tunggal atau bila
    /// tidak ada kartu yang sedang diperbaiki.
    pub(super) async fn fix_commit_prefix(&self, run: &RunRecord) -> Option<String> {
        let cards = match self.runs.cards(&run.id).await {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!(run_id = %run.id, error = %e, "kartu grup tidak terbaca; commit tanpa awalan kunci");
                return None;
            }
        };
        let mut keys = Vec::new();
        for c in cards
            .iter()
            .filter(|c| c.is_open() && !c.fix_replies.is_empty())
        {
            keys.push(self.key_or_id(&c.record_id).await);
        }
        (!keys.is_empty()).then(|| keys.join(", "))
    }

    /// Judul dan bahan deskripsi PR grup (KTD-G7, R16).
    pub(super) async fn group_heading(
        &self,
        cards: &[GroupCardRow],
    ) -> Result<PrHeading, RunError> {
        let ids: Vec<String> = cards.iter().map(|c| c.record_id.clone()).collect();
        let tasks = self.sync.lark.fetch(&ids).await?;
        let task_of = |id: &str| tasks.iter().find(|t| t.record_id == id);
        let active: Vec<&Task> = cards
            .iter()
            .filter(|c| c.is_open())
            .filter_map(|c| task_of(&c.record_id))
            .collect();
        let first = active
            .first()
            .ok_or(RunError::InvalidState("no card left in the group"))?;
        let keys: Vec<&str> = active.iter().map(|t| key_of(t)).collect();
        let epic = self.epic_name(first).await;
        let mut description = String::new();
        let mut acs = Vec::new();
        for t in &active {
            let _ = write!(
                description,
                "{}: {}\n\n{}\n\n",
                key_of(t),
                t.title,
                t.description
            );
            for ac in self.ac_texts(t).await.unwrap_or_default() {
                acs.push(format!("{} {ac}", key_of(t)));
            }
        }
        let pr_cards = cards
            .iter()
            .map(|c| {
                let task = task_of(&c.record_id);
                GroupPrCard {
                    record_id: c.record_id.clone(),
                    key: task.map_or(c.record_id.clone(), |t| key_of(t).to_owned()),
                    title: task.map(|t| t.title.clone()).unwrap_or_default(),
                    removed: removed_label(&c.state),
                    kept_reason: c.kept_reason.clone(),
                }
            })
            .collect();
        Ok(PrHeading {
            title: group::pr_title(&keys, epic.as_deref(), &first.title),
            header: format!("Lark tasks: {}", keys.join(", ")),
            description: description.trim_end().to_owned(),
            acs,
            cards: Some(pr_cards),
            warning: fix_warning(&active),
        })
    }

    /// Nama epic kartu; kegagalan hanya dicatat (judul jatuh ke judul kartu).
    async fn epic_name(&self, task: &Task) -> Option<String> {
        if task.epic_ids.is_empty() {
            return None;
        }
        match self.sync.lark.epics(&task.epic_ids).await {
            Ok(epics) => epics.into_iter().next().map(|e| e.name),
            Err(e) => {
                tracing::warn!(record_id = %task.record_id, error = %e, "epic tidak terbaca; judul PR memakai judul kartu");
                None
            }
        }
    }

    /// Link PR ke semua kartu grup yang masih disertakan; Waiting Review hanya
    /// untuk kartu yang tidak berubah di Lark (R14, KTD-G7). Kartu yang masih
    /// Waiting Fix tidak disentuh; kartu lain keluar dari loop perbaikan (R17).
    pub(super) async fn mark_group_cards(
        &self,
        run_id: &str,
        url: &str,
        cursor_now_too: bool,
    ) -> Result<(), RunError> {
        let mut cards = self.runs.cards(run_id).await?;
        let domain = group_cards_domain(&cards);
        let moves: Vec<String> = group::publish_moves(&domain)
            .into_iter()
            .map(str::to_owned)
            .collect();
        for c in cards.iter_mut().filter(|c| c.is_open()) {
            let task = self.task_of(&c.record_id).await?;
            // Kartu Waiting Fix yang komentarnya belum dikirim menunggu giliran
            // perbaikannya sendiri. Kartu yang sudah diperbaiki di siklus ini
            // bisa masih terbaca Waiting Fix selama transisinya di outbox.
            if task.status == TaskStatus::WaitingFix && c.fix_replies.is_empty() {
                continue;
            }
            if moves.contains(&c.record_id) {
                self.mark_task(&task, url, cursor_now_too).await?;
            } else {
                let op = WriteOp::Fields(vec![AppField::PrUrl(url.to_owned())]);
                self.sync.request_write(&c.record_id, &op).await?;
            }
            c.in_fix = false;
            c.quiet_until_ms = None;
            c.fix_replies.clear();
        }
        self.runs.save_cards(run_id, &cards, None).await?;
        Ok(())
    }

    /// Baris buku AC untuk evaluasi gerbang; run grup memberi kunci kartu di
    /// depan label agar pesan menyebut kartunya.
    ///
    /// # Errors
    /// Penyimpanan atau Lark gagal.
    pub(crate) async fn gate_rows(&self, run: &RunRecord) -> Result<Vec<AcRow>, RunError> {
        let rows = self.reviews.ac_rows(&run.id).await?;
        let ids: Vec<String> = self
            .runs
            .cards(&run.id)
            .await?
            .into_iter()
            .map(|c| c.record_id)
            .collect();
        if ids.is_empty() {
            return Ok(rows);
        }
        let keys: HashMap<String, String> = self
            .sync
            .lark
            .fetch(&ids)
            .await?
            .iter()
            .map(|t| (t.record_id.clone(), key_of(t).to_owned()))
            .collect();
        Ok(labelled_rows(&rows, &keys))
    }

    /// Link PR dan Waiting Review untuk satu kartu. Bila status sudah diubah
    /// manusia, revalidasi outbox membatalkan transisi dan menandai kartu (KTD-7).
    pub(super) async fn mark_card(
        &self,
        record_id: &str,
        url: &str,
        cursor_now_too: bool,
    ) -> Result<(), RunError> {
        let task = self.task_of(record_id).await?;
        self.mark_task(&task, url, cursor_now_too).await
    }

    async fn mark_task(
        &self,
        task: &Task,
        url: &str,
        cursor_now_too: bool,
    ) -> Result<(), RunError> {
        let record_id = task.record_id.as_str();
        let mut fields = vec![AppField::PrUrl(url.to_owned())];
        if task.status == TaskStatus::WaitingReview {
            if task.pr_url.as_deref() != Some(url) {
                self.sync
                    .request_write(record_id, &WriteOp::Fields(fields))
                    .await?;
            }
            return Ok(());
        }
        if cursor_now_too {
            fields.push(AppField::CommentCursor(super::publish::cursor_now(
                self.sync.clock.now_ms(),
            )));
        }
        let op = WriteOp::Transition {
            from: TaskStatus::InProgress,
            to: TaskStatus::WaitingReview,
            fields,
        };
        self.sync.request_write(record_id, &op).await?;
        Ok(())
    }
}
