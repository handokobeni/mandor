//! Kartu Planned yang dipulihkan di sesi baru (plan 2026-10-30-001, KTD-3,
//! KTD-12): buku AC dari Lark ditimpa kolom Design paket, dan pesan pertama
//! membawa plan yang disetujui.

use mandor_domain::planned::{DesignMark, PlannedWork};
use mandor_domain::{Design, Task};

use super::start::ac_texts;
use super::{RunError, RunService};
use crate::ports::{Deps, LarkTasks};
use crate::review::DesignRoot;
use crate::run_ports::{LedgerStore, Prompts, RunRecord};

/// Paket plan untuk Take over ke laptop: sumber desain laptop yang masih ada
/// sudah di-resolve oleh adapter; yang hilang hanya dicatat namanya.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TakeoverPlan {
    pub work: PlannedWork,
    pub design: Vec<DesignRoot>,
    pub missing_design: Vec<String>,
}

impl<D: Deps> RunService<D> {
    /// Mengisi buku AC run dari Lark lalu menimpa kolom Design dengan hasil
    /// tahap plan. Baris paket untuk AC yang sudah tidak ada diabaikan.
    pub(super) async fn seed_planned_ledger(
        &self,
        run_id: &str,
        task: &Task,
        marks: &[DesignMark],
    ) -> Result<(), RunError> {
        let acs = self.sync.lark.acceptance_criteria(&task.ac_ids).await?;
        self.init_ledger(run_id, &acs).await;
        let mut rows = self.reviews.ac_rows(run_id).await?;
        for row in &mut rows {
            if let Some(mark) = marks.iter().find(|m| m.ac_id == row.ac_id) {
                row.design = mark.design;
                row.design_note.clone_from(&mark.note);
            }
        }
        self.reviews.save_ac_rows(run_id, &rows).await?;
        Ok(())
    }

    /// Pesan pertama sesi baru untuk plan yang disetujui di sesi lain: tugas,
    /// AC, plan, AC Follow design, dan sumber desain run bila ada.
    pub(super) async fn approved_plan_turn(
        &self,
        run: &RunRecord,
        plan: &str,
    ) -> Result<String, RunError> {
        let task = self.task_of(&run.record_id).await?;
        let acs = ac_texts(&self.sync.lark.acceptance_criteria(&task.ac_ids).await?);
        let followed: Vec<String> = self
            .reviews
            .ac_rows(&run.id)
            .await?
            .iter()
            .filter(|r| r.design == Design::FollowDesign)
            .map(|r| format!("{}: {}", r.display_label(), r.text))
            .collect();
        let message = self
            .prompts
            .approved_plan_message(&task, &acs, plan, &followed);
        Ok(self.with_design(run, message))
    }
}
