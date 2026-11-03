//! Satu plan untuk seluruh grup (plan `2026-10-23-004`, R25, KTD-G19,
//! KTD-G20): satu giliran plan berisi AC semua kartu, satu persetujuan, lalu
//! kartu dikerjakan satu per satu tanpa giliran plan. Plan disetujui ulang
//! bila AC kartu yang belum selesai berubah.

use std::sync::Arc;

use mandor_domain::group::{self, AcSnap, CardState};
use mandor_domain::{HoldReason, Task};

use super::group::{GroupInput, ac_lines, key_of};
use super::start::Plan;
use super::{RunError, RunService};
use crate::ports::{AcItem, Deps};
use crate::review::DesignRoot;
use crate::run_ports::{GroupCardRow, Prompts, RunPhase, RunRecord, RunStore};
use crate::{Planning, StartStep, StartTarget};

/// Catatan untuk giliran plan grup: daftar kartu dan bentuk plan.
fn group_note(tasks: &[Task], dev_note: Option<&str>) -> String {
    let list: Vec<String> = tasks
        .iter()
        .map(|t| format!("{} {}", key_of(t), t.title))
        .collect();
    let mut note = format!(
        "These {} cards form one flow on the same branch: {}. Write one plan that covers the \
         acceptance criteria of all cards, with one section per card in this order. Later turns \
         implement one card at a time from this plan.",
        tasks.len(),
        list.join("; "),
    );
    if let Some(n) = dev_note.map(str::trim).filter(|n| !n.is_empty()) {
        note.push_str("\n\n");
        note.push_str(n);
    }
    note
}

/// AC semua kartu dengan kunci kartunya.
fn labelled_acs(input: &GroupInput, only: Option<&[String]>) -> Vec<String> {
    input
        .tasks
        .iter()
        .zip(&input.per_card)
        .filter(|(_, (id, _))| only.is_none_or(|o| o.contains(id)))
        .flat_map(|(t, (_, items))| {
            let key = key_of(t).to_owned();
            ac_lines(items)
                .into_iter()
                .map(move |l| format!("{key} {l}"))
        })
        .collect()
}

/// Batas giliran implementasi pada grup satu plan.
pub(super) fn scope_note(key: &str) -> String {
    format!(
        "Follow the approved group plan: implement only the section for {key} now; the next \
         cards follow in later turns."
    )
}

/// Cuplikan AC kartu `remaining` (KTD-G20).
pub(super) fn snaps_of(per_card: &[(String, Vec<AcItem>)], remaining: &[&str]) -> Vec<AcSnap> {
    per_card
        .iter()
        .filter(|(card, _)| remaining.contains(&card.as_str()))
        .flat_map(|(card, items)| {
            items.iter().filter(|a| !a.dropped).map(move |a| AcSnap {
                card: card.clone(),
                ac_id: a.record_id.clone(),
                text: a.text.clone(),
            })
        })
        .collect()
}

/// Kartu yang belum selesai (aktif atau menunggu).
pub(super) fn remaining_cards(cards: &[GroupCardRow]) -> Vec<String> {
    cards
        .iter()
        .filter(|c| matches!(c.state, CardState::Pending | CardState::Active))
        .map(|c| c.record_id.clone())
        .collect()
}

impl<D: Deps> RunService<D> {
    /// "Start together" dengan "One plan for all cards" (R25): seperti
    /// [`RunService::start_group`], tetapi giliran pertama membuat satu plan
    /// untuk seluruh grup.
    ///
    /// # Errors
    /// Kurang dari dua kartu, kartu punya run, gerbang, klaim, git, atau agent gagal.
    pub async fn start_one_plan_group(
        self: &Arc<Self>,
        record_ids: &[String],
        target: &StartTarget,
        dev_note: Option<&str>,
        skill: Option<String>,
        design: &[DesignRoot],
        progress: impl Fn(StartStep) + Send + Sync,
    ) -> Result<RunRecord, RunError> {
        if record_ids.len() < 2 {
            return Err(RunError::InvalidState("a group needs at least two cards"));
        }
        let _work = self.enter_work()?;
        for id in record_ids {
            if self.runs.active_for(id).await?.is_some() {
                return Err(RunError::InvalidState("a card already has an active run"));
            }
        }
        let mut plan: Plan = self.resolve(target).await?;
        plan.design = design.to_vec();
        plan.whole_group = true;
        self.start_group_plan(
            record_ids,
            &plan,
            dev_note,
            &Planning::Plan { skill },
            progress,
        )
        .await
    }

    /// Pesan giliran plan grup; menyetel fase plan dan mode satu plan.
    pub(super) fn one_plan_message(
        &self,
        run: &mut RunRecord,
        input: &GroupInput,
        dev_note: Option<&str>,
        skill: Option<&str>,
    ) -> String {
        run.phase = RunPhase::Plan;
        run.group_plan = false;
        run.one_plan = Some(Vec::new());
        let note = group_note(&input.tasks, dev_note);
        let acs = labelled_acs(input, None);
        let msg = self
            .prompts
            .plan_message(&input.tasks[0], &acs, Some(&note), skill);
        self.with_design(run, msg)
    }

    /// Cuplikan AC kartu yang belum selesai, dibaca ulang dari Lark, untuk
    /// disimpan saat plan grup disetujui.
    pub(super) async fn one_plan_snapshot(&self, run_id: &str) -> Result<Vec<AcSnap>, RunError> {
        let cards = self.group_of(run_id).await?;
        let remaining = remaining_cards(&cards);
        let input = self.load_group(&remaining).await?;
        let refs: Vec<&str> = remaining.iter().map(String::as_str).collect();
        Ok(snaps_of(&input.per_card, &refs))
    }

    /// Kartu yang AC-nya berubah sejak plan grup disetujui; kosong bila plan
    /// masih berlaku atau run bukan grup satu plan.
    pub(super) fn outdated_cards(
        run: &RunRecord,
        cards: &[GroupCardRow],
        input: &GroupInput,
    ) -> Vec<String> {
        let Some(approved) = &run.one_plan else {
            return Vec::new();
        };
        let remaining = remaining_cards(cards);
        let refs: Vec<&str> = remaining.iter().map(String::as_str).collect();
        let current = snaps_of(&input.per_card, &refs);
        group::plan_outdated(approved, &current, &refs)
    }

    /// Plan grup usang sebelum kartu `next` dimulai (R25, KTD-G20): di
    /// laptop agent merevisi plan dan dev menyetujui ulang; di VM grup
    /// diparkir.
    pub(super) async fn plan_outdated_turn(
        self: &Arc<Self>,
        run: &mut RunRecord,
        cards: &mut [GroupCardRow],
        next: usize,
        input: &GroupInput,
        changed: &[String],
    ) -> Result<(), RunError> {
        let keys: Vec<String> = changed
            .iter()
            .map(|id| {
                input
                    .tasks
                    .iter()
                    .find(|t| &t.record_id == id)
                    .map_or_else(|| id.clone(), |t| key_of(t).to_owned())
            })
            .collect();
        if self.is_vm() {
            cards[next].state = CardState::Pending;
            self.save_group(run, cards).await?;
            let note = format!(
                "The acceptance criteria of {} changed after the group plan was approved. Take the \
                 group over to the laptop to approve a revised plan.",
                keys.join(", ")
            );
            return self.hold(run, HoldReason::PlanOutdated, Some(note)).await;
        }
        let remaining = remaining_cards(cards);
        let acs = labelled_acs(input, Some(&remaining));
        let message = format!(
            "The acceptance criteria of {} changed after the group plan was approved. Revise the \
             group plan for the cards not finished yet, one section per card, using the current \
             acceptance criteria below, then wait for approval. Do not change code.\n\n{}",
            keys.join(", "),
            acs.join("\n")
        );
        run.phase = RunPhase::Plan;
        run.note = None;
        self.save_group(run, cards).await?;
        let message = self.with_design(run, message);
        let req = self.resume_request(run, message).await?;
        self.spawn_turn(
            run,
            req,
            Some(format!("Group plan outdated: {}", keys.join(", "))),
        )
        .await
    }

    /// Kunci kartu aktif grup, untuk batas giliran setelah plan disetujui.
    pub(super) async fn active_key(&self, run_id: &str) -> Result<Option<String>, RunError> {
        let active = self
            .runs
            .cards(run_id)
            .await?
            .into_iter()
            .find(|c| c.state == CardState::Active);
        Ok(match active {
            Some(c) => Some(self.key_or_id(&c.record_id).await),
            None => None,
        })
    }
}
