//! Poll record milik pengguna (KTD-6).

use std::collections::HashSet;

use mandor_domain::task::needs_comment_poll;
use mandor_domain::{AppField, CardFlag, FlagKind, TaskStatus, WriteOp};

use super::{SyncError, SyncService};
use crate::ports::Clock;
use crate::ports::{Deps, LarkTasks, Outbox, TaskCache};

/// Jendela poll inkremental: record yang berubah sejak kemarin.
const INCREMENTAL_WINDOW_MS: i64 = 24 * 60 * 60 * 1000;

/// Hasil satu putaran poll.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PollReport {
    /// Record yang baru atau berubah sejak cache.
    pub changed: Vec<String>,
    /// Record yang ditandai dihapus atau dialihkan (hanya pada rekonsiliasi penuh).
    pub flagged: Vec<String>,
    /// `true` bila komentar perlu diambil pada putaran ini.
    pub comments_needed: bool,
}

impl<D: Deps> SyncService<D> {
    /// Satu putaran poll. `full` menjalankan rekonsiliasi penuh (tiap 5 menit):
    /// tanpa batas waktu, dan memeriksa record cache yang tidak muncul lagi.
    /// `in_fix_loop` adalah record yang state alur kerjanya sedang dalam loop
    /// perbaikan.
    ///
    /// # Errors
    /// [`SyncError::Port`] bila Lark atau cache gagal.
    pub async fn poll(&self, full: bool, in_fix_loop: &[String]) -> Result<PollReport, SyncError> {
        let now = self.clock.now_ms();
        let since = (!full).then_some(now - INCREMENTAL_WINDOW_MS);
        let fresh = self.lark.search_assigned(&self.me, since).await?;
        let mut report = PollReport::default();
        let mut seen = HashSet::new();

        for task in &fresh {
            seen.insert(task.record_id.clone());
            let prev = self.cache.get(&task.record_id).await?;
            let missing_before = prev
                .as_ref()
                .and_then(|p| p.flag.as_ref())
                .is_some_and(|f| matches!(f.kind, FlagKind::Deleted | FlagKind::Unassigned));
            let unchanged = prev
                .as_ref()
                .is_some_and(|p| p.task.last_modified_ms == task.last_modified_ms);
            if unchanged && !missing_before {
                continue;
            }
            // Nilai Lark selalu menang untuk field milik manusia.
            self.cache.upsert(task).await?;
            if missing_before {
                self.cache.set_flag(&task.record_id, None).await?;
            }
            report.changed.push(task.record_id.clone());

            let reverted_to_todo = prev
                .as_ref()
                .is_some_and(|p| p.task.status != TaskStatus::ToDo)
                && task.status == TaskStatus::ToDo;
            if task.held_here(&self.identity) {
                let mut fields = Vec::new();
                if reverted_to_todo {
                    fields.push(AppField::Claim(None));
                }
                if let Some(col) = task.mirror_update() {
                    fields.push(AppField::KanbanMirror(col.to_owned()));
                }
                if !fields.is_empty() {
                    self.outbox
                        .enqueue(&task.record_id, &WriteOp::Fields(fields), now)
                        .await?;
                }
            }
        }

        if full {
            self.reconcile_missing(&seen, &mut report).await?;
        }

        let cached: Vec<_> = self
            .cache
            .list()
            .await?
            .into_iter()
            .map(|c| c.task)
            .collect();
        report.comments_needed = needs_comment_poll(&cached, in_fix_loop, &self.me);
        Ok(report)
    }

    /// Record di cache yang tidak lagi muncul di pencarian: dihapus atau dialihkan.
    async fn reconcile_missing(
        &self,
        seen: &HashSet<String>,
        report: &mut PollReport,
    ) -> Result<(), SyncError> {
        let candidates: Vec<String> = self
            .cache
            .list()
            .await?
            .into_iter()
            .filter(|c| !seen.contains(&c.task.record_id))
            .filter(|c| {
                !c.flag
                    .as_ref()
                    .is_some_and(|f| matches!(f.kind, FlagKind::Deleted | FlagKind::Unassigned))
            })
            .map(|c| c.task.record_id)
            .collect();
        if candidates.is_empty() {
            return Ok(());
        }
        let found = self.lark.fetch(&candidates).await?;
        for id in candidates {
            let flag = match found.iter().find(|t| t.record_id == id) {
                None => CardFlag {
                    kind: FlagKind::Deleted,
                    detail: "record was deleted in Lark".to_owned(),
                },
                Some(t) if !t.assigned_to(&self.me) => {
                    self.cache.upsert(t).await?;
                    CardFlag {
                        kind: FlagKind::Unassigned,
                        detail: "record is no longer assigned to you".to_owned(),
                    }
                }
                // Masih milik pengguna tetapi di luar filter (mis. belum terindeks).
                Some(t) => {
                    self.cache.upsert(t).await?;
                    continue;
                }
            };
            self.cache.set_flag(&id, Some(&flag)).await?;
            report.flagged.push(id);
        }
        Ok(())
    }
}
