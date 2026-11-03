//! Menutup run kartu yang sudah selesai dan mendaftar ukuran worktree run
//! (plan `2026-11-03-001`, R1–R9, KTD-C1–KTD-C4).

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::Ordering;

use mandor_domain::cleanup::{self, CloseDecision, SkipReason};
use mandor_domain::group::CardState;
use mandor_domain::{RunState, TaskStatus};

use super::{RunError, RunService};
use crate::ports::{Clock, Deps};
use crate::run_ports::{
    CleanupSettings, GitPort, Notice, NoticeKind, Notifier, RunRecord, RunStore,
};

/// Jarak minimum antar pemeriksaan otomatis (KTD-C4).
const AUTO_INTERVAL_MS: i64 = 10 * 60 * 1000;

/// Satu run yang kartunya sudah selesai.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FinishedRun {
    pub run_id: String,
    /// Kunci kartu yang masih disertakan (satu untuk run tunggal).
    pub keys: Vec<String>,
    pub title: String,
    /// Ukuran worktree; `None` bila gagal diukur.
    pub bytes: Option<u64>,
}

/// Run selesai yang tidak ditutup beserta alasannya.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkippedRun {
    pub run: FinishedRun,
    pub reason: SkipReason,
}

/// Hasil pemeriksaan run selesai.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FinishedRuns {
    pub closable: Vec<FinishedRun>,
    pub skipped: Vec<SkippedRun>,
}

/// Hasil penutupan run selesai.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CloseFinishedReport {
    pub closed: Vec<String>,
    pub skipped: Vec<SkippedRun>,
    pub freed_bytes: u64,
}

/// Satu run terbuka beserta ukuran worktree-nya (R9).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunStorage {
    pub run_id: String,
    pub keys: Vec<String>,
    pub title: String,
    pub state: RunState,
    /// Status Lark kartu yang masih disertakan.
    pub card_status: Vec<TaskStatus>,
    pub bytes: Option<u64>,
    pub updated_at_ms: i64,
}

/// Kartu run yang masih disertakan: kunci, judul pertama, dan statusnya.
struct RunCards {
    keys: Vec<String>,
    title: String,
    statuses: Vec<TaskStatus>,
}

/// Penilaian satu run selesai.
enum Verdict {
    NotFinished,
    Decided(FinishedRun, CloseDecision),
}

impl<D: Deps> RunService<D> {
    /// Run terbuka yang kartunya selesai: yang bisa ditutup dan yang dilewati
    /// (R1, R2).
    ///
    /// # Errors
    /// Penyimpanan gagal.
    pub async fn finished_runs(&self) -> Result<FinishedRuns, RunError> {
        let mut out = FinishedRuns::default();
        for run in self.runs.list_active().await? {
            match self.verdict(&run).await? {
                Verdict::NotFinished => {}
                Verdict::Decided(r, CloseDecision::Close) => out.closable.push(r),
                Verdict::Decided(r, CloseDecision::Skip(reason)) => {
                    out.skipped.push(SkippedRun { run: r, reason });
                }
            }
        }
        Ok(out)
    }

    /// Menutup run `run_ids` yang masih selesai dan bersih saat diperiksa ulang
    /// (KTD-C2); run lain dilewati atau diabaikan (R3).
    ///
    /// # Errors
    /// Penyimpanan gagal.
    pub async fn close_finished(
        self: &Arc<Self>,
        run_ids: &[String],
    ) -> Result<CloseFinishedReport, RunError> {
        let mut report = CloseFinishedReport::default();
        for id in run_ids {
            let Some(run) = self.runs.get(id).await? else {
                continue;
            };
            match self.verdict(&run).await? {
                Verdict::NotFinished => {}
                Verdict::Decided(r, CloseDecision::Skip(reason)) => {
                    report.skipped.push(SkippedRun { run: r, reason });
                }
                Verdict::Decided(r, CloseDecision::Close) => {
                    self.close_run(id).await?;
                    tracing::info!(run_id = %id, keys = ?r.keys, "run kartu selesai ditutup");
                    report.freed_bytes += r.bytes.unwrap_or_default();
                    report.closed.push(id.clone());
                }
            }
        }
        Ok(report)
    }

    /// Penutupan otomatis (R6): hanya bila dinyalakan dan paling sering sekali
    /// per 10 menit. `None` bila tidak berjalan.
    ///
    /// # Errors
    /// Penyimpanan gagal.
    pub async fn auto_close_finished(
        self: &Arc<Self>,
    ) -> Result<Option<CloseFinishedReport>, RunError> {
        if !self.runs.cleanup_settings().await.auto_close {
            return Ok(None);
        }
        let now = self.sync.clock.now_ms();
        let last = self.cleanup_checked_ms.load(Ordering::Relaxed);
        if last != 0 && now - last < AUTO_INTERVAL_MS {
            return Ok(None);
        }
        self.cleanup_checked_ms.store(now, Ordering::Relaxed);
        let ids: Vec<String> = self
            .finished_runs()
            .await?
            .closable
            .into_iter()
            .map(|r| r.run_id)
            .collect();
        let report = self.close_finished(&ids).await?;
        if !report.closed.is_empty() {
            self.notifier
                .notify(Notice {
                    kind: NoticeKind::RunsClosed,
                    record_id: String::new(),
                    run_id: None,
                    title: "Finished runs closed".to_owned(),
                    body: closed_text(report.closed.len(), report.freed_bytes),
                })
                .await;
        }
        Ok(Some(report))
    }

    /// Semua run terbuka beserta ukuran worktree, terbesar dulu (R9).
    ///
    /// # Errors
    /// Penyimpanan gagal.
    pub async fn run_storage(&self) -> Result<Vec<RunStorage>, RunError> {
        let mut out = Vec::new();
        for run in self.runs.list_active().await? {
            let cards = self.run_cards(&run).await?;
            out.push(RunStorage {
                run_id: run.id.clone(),
                keys: cards.keys,
                title: cards.title,
                state: run.state,
                card_status: cards.statuses,
                bytes: self.run_bytes(&run).await,
                updated_at_ms: run.updated_at_ms,
            });
        }
        out.sort_by(|a, b| {
            b.bytes
                .unwrap_or_default()
                .cmp(&a.bytes.unwrap_or_default())
        });
        Ok(out)
    }

    /// Setelan penutupan run selesai.
    pub async fn cleanup_settings(&self) -> CleanupSettings {
        self.runs.cleanup_settings().await
    }

    /// Menyimpan setelan penutupan run selesai.
    ///
    /// # Errors
    /// Penyimpanan gagal.
    pub async fn set_cleanup_settings(&self, s: &CleanupSettings) -> Result<(), RunError> {
        Ok(self.runs.set_cleanup_settings(s).await?)
    }

    async fn verdict(&self, run: &RunRecord) -> Result<Verdict, RunError> {
        let cards = self.run_cards(run).await?;
        if !cleanup::finished(&cards.statuses) {
            return Ok(Verdict::NotFinished);
        }
        let clean = self.worktrees_clean(run).await;
        let found = FinishedRun {
            run_id: run.id.clone(),
            keys: cards.keys,
            title: cards.title,
            bytes: self.run_bytes(run).await,
        };
        Ok(Verdict::Decided(
            found,
            cleanup::close_decision(run.state, clean),
        ))
    }

    /// Kartu run yang masih disertakan; kartu yang tidak ada di cache
    /// dianggap belum selesai.
    async fn run_cards(&self, run: &RunRecord) -> Result<RunCards, RunError> {
        let grouped = self.runs.cards(&run.id).await?;
        let ids: Vec<String> = if grouped.is_empty() {
            vec![run.record_id.clone()]
        } else {
            grouped
                .into_iter()
                .filter(|c| !matches!(c.state, CardState::Removed(_)))
                .map(|c| c.record_id)
                .collect()
        };
        let mut out = RunCards {
            keys: Vec::new(),
            title: String::new(),
            statuses: Vec::new(),
        };
        for id in &ids {
            if let Ok(t) = self.task_of(id).await {
                if out.title.is_empty() {
                    out.title.clone_from(&t.title);
                }
                out.keys.push(t.key.clone().unwrap_or_else(|| id.clone()));
                out.statuses.push(t.status.clone());
            } else {
                out.keys.push(id.clone());
                out.statuses.push(TaskStatus::Unknown("missing".to_owned()));
            }
        }
        Ok(out)
    }

    /// Semua worktree anggota bersih dan tanpa commit belum di-push; gagal
    /// membaca dianggap tidak bersih (R2).
    async fn worktrees_clean(&self, run: &RunRecord) -> bool {
        let Some(branch) = &run.branch else {
            return true;
        };
        let Ok(members) = self.members(run).await else {
            return run.worktree.is_none();
        };
        for m in members {
            let clean = self
                .git
                .changes(&m.repo, &m.worktree, branch)
                .await
                .is_ok_and(|c| c.is_empty());
            if !clean {
                return false;
            }
        }
        true
    }

    /// Ukuran folder run: folder workspace, atau worktree repo tunggal.
    async fn run_bytes(&self, run: &RunRecord) -> Option<u64> {
        let path: PathBuf = match (&run.workspace, &run.worktree) {
            (Some(w), _) => w.root.clone(),
            (None, Some(wt)) => wt.clone(),
            (None, None) => return Some(0),
        };
        match self.git.dir_size(&path).await {
            Ok(b) => Some(b),
            Err(e) => {
                tracing::warn!(run_id = %run.id, error = %e, "ukuran worktree tidak terbaca");
                None
            }
        }
    }
}

/// Ringkasan untuk notifikasi dan UI.
#[must_use]
pub fn closed_text(count: usize, freed_bytes: u64) -> String {
    let runs = if count == 1 { "run" } else { "runs" };
    format!(
        "Closed {count} finished {runs}, freed {}",
        human_bytes(freed_bytes)
    )
}

/// Ukuran yang mudah dibaca (desimal, satu angka di belakang koma).
#[must_use]
pub fn human_bytes(b: u64) -> String {
    const UNITS: [&str; 4] = ["B", "KB", "MB", "GB"];
    let mut value = u128::from(b) * 10;
    let mut unit = 0;
    while value >= 10_000 && unit < UNITS.len() - 1 {
        value /= 1000;
        unit += 1;
    }
    if unit == 0 {
        format!("{b} B")
    } else {
        format!("{}.{} {}", value / 10, value % 10, UNITS[unit])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ringkasan_dan_ukuran() {
        assert_eq!(human_bytes(512), "512 B");
        assert_eq!(human_bytes(2_000_000), "2.0 MB");
        assert_eq!(human_bytes(1_050_000_000), "1.0 GB");
        assert_eq!(closed_text(1, 3_000), "Closed 1 finished run, freed 3.0 KB");
        assert_eq!(closed_text(2, 0), "Closed 2 finished runs, freed 0 B");
    }
}
