//! Sinkron buku AC dengan tracker (U3, KTD-27).

use std::sync::{Arc, PoisonError};

use mandor_domain::ac_ledger::{initial_rows, reconcile_rows};
use mandor_domain::group::CardState;
use mandor_domain::{AcRow, FetchedAc};

use crate::ports::{AcItem, Deps, LarkTasks};
use crate::run::{RunError, RunService};
use crate::run_ports::{GroupCardRow, LedgerStore, RunRecord, RunStore};

/// Hasil refresh buku AC.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LedgerRefresh {
    pub rows: Vec<AcRow>,
    /// Waktu sinkron terakhir yang berhasil (ms epoch); `None` bila belum pernah.
    pub synced_at_ms: Option<i64>,
    /// Alasan refresh gagal; buku AC dikembalikan apa adanya.
    pub error: Option<String>,
    /// Nama sumber desain run; kosong = tanpa sumber desain ("No design source").
    pub design_sources: Vec<String>,
}

fn design_names(run: &crate::run_ports::RunRecord) -> Vec<String> {
    run.design.iter().map(|d| d.name.clone()).collect()
}

fn fetched(items: &[AcItem]) -> Vec<FetchedAc> {
    items
        .iter()
        .map(|a| FetchedAc {
            ac_id: a.record_id.clone(),
            label: a.label.clone(),
            text: a.text.clone(),
            dropped: a.dropped,
        })
        .collect()
}

impl<D: Deps> RunService<D> {
    /// Buku AC run seperti tersimpan.
    ///
    /// # Errors
    /// Penyimpanan gagal.
    pub async fn ledger(&self, run_id: &str) -> Result<Vec<AcRow>, RunError> {
        Ok(self.reviews.ac_rows(run_id).await?)
    }

    /// Buku AC awal saat Start dari AC yang sudah diambil. Kegagalan hanya
    /// dicatat: refresh berikutnya membangunnya ulang.
    pub(crate) async fn init_ledger(&self, run_id: &str, acs: &[AcItem]) {
        let rows = initial_rows(&fetched(acs));
        match self.reviews.save_ac_rows(run_id, &rows).await {
            Ok(()) => self.mark_synced(run_id),
            Err(e) => tracing::warn!(run_id, error = %e, "buku AC awal tidak tersimpan"),
        }
    }

    /// Membaca ulang task (untuk tautan AC terbaru) lalu AC-nya, menyelaraskan
    /// buku AC, dan menyimpannya. Kegagalan Lark dilaporkan di hasil tanpa
    /// mengubah buku AC.
    ///
    /// # Errors
    /// Run tidak ada atau penyimpanan gagal.
    pub async fn refresh_ledger(&self, run_id: &str) -> Result<LedgerRefresh, RunError> {
        let run = self
            .runs
            .get(run_id)
            .await?
            .ok_or(RunError::NotFound("run"))?;
        let existing = self.reviews.ac_rows(run_id).await?;
        let cards = self.runs.cards(run_id).await?;
        if !cards.is_empty() {
            return self.refresh_group_ledger(&run, &cards, existing).await;
        }
        let latest = async {
            let task = self
                .sync
                .lark
                .fetch(std::slice::from_ref(&run.record_id))
                .await?
                .into_iter()
                .find(|t| t.record_id == run.record_id)
                .ok_or(crate::PortError::Rejected("task not found".to_owned()))?;
            self.sync.lark.acceptance_criteria(&task.ac_ids).await
        }
        .await;
        let items = match latest {
            Ok(items) => items,
            Err(e) => {
                tracing::warn!(run_id, error = %e, "AC tidak terbaca; buku AC tidak berubah");
                return Ok(LedgerRefresh {
                    rows: existing,
                    synced_at_ms: self.synced_at(run_id),
                    error: Some(e.to_string()),
                    design_sources: design_names(&run),
                });
            }
        };
        let rows = if existing.is_empty() {
            initial_rows(&fetched(&items))
        } else {
            reconcile_rows(&existing, &fetched(&items))
        };
        if rows != existing {
            self.reviews.save_ac_rows(run_id, &rows).await?;
        }
        self.mark_synced(run_id);
        Ok(LedgerRefresh {
            rows,
            synced_at_ms: self.synced_at(run_id),
            error: None,
            design_sources: design_names(&run),
        })
    }

    /// Refresh buku AC run grup: AC semua kartu aktif, diberi label kartunya.
    /// Satu kartu gagal dibaca membatalkan seluruh refresh agar AC kartu itu
    /// tidak menjadi Removed.
    async fn refresh_group_ledger(
        &self,
        run: &RunRecord,
        cards: &[GroupCardRow],
        existing: Vec<AcRow>,
    ) -> Result<LedgerRefresh, RunError> {
        let active: Vec<String> = cards
            .iter()
            .filter(|c| !matches!(c.state, CardState::Removed(_)))
            .map(|c| c.record_id.clone())
            .collect();
        let latest = async {
            let tasks = self.sync.lark.fetch(&active).await?;
            let mut per_card = Vec::with_capacity(active.len());
            for id in &active {
                let task = tasks
                    .iter()
                    .find(|t| &t.record_id == id)
                    .ok_or_else(|| crate::PortError::Rejected(format!("task {id} not found")))?;
                per_card.push((
                    id.clone(),
                    self.sync.lark.acceptance_criteria(&task.ac_ids).await?,
                ));
            }
            Ok::<_, crate::PortError>(per_card)
        }
        .await;
        let per_card = match latest {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!(run_id = %run.id, error = %e, "AC grup tidak terbaca; buku AC tidak berubah");
                return Ok(LedgerRefresh {
                    rows: existing,
                    synced_at_ms: self.synced_at(&run.id),
                    error: Some(e.to_string()),
                    design_sources: design_names(run),
                });
            }
        };
        let all: Vec<AcItem> = per_card
            .iter()
            .flat_map(|(_, v)| v.iter().cloned())
            .collect();
        let mut rows = if existing.is_empty() {
            initial_rows(&fetched(&all))
        } else {
            reconcile_rows(&existing, &fetched(&all))
        };
        for row in &mut rows {
            if let Some((card, _)) = per_card
                .iter()
                .find(|(_, items)| items.iter().any(|a| a.record_id == row.ac_id))
            {
                row.card = Some(card.clone());
            }
        }
        if rows != existing {
            self.reviews.save_ac_rows(&run.id, &rows).await?;
        }
        self.mark_synced(&run.id);
        Ok(LedgerRefresh {
            rows,
            synced_at_ms: self.synced_at(&run.id),
            error: None,
            design_sources: design_names(run),
        })
    }

    /// Refresh buku AC semua run aktif (rekonsiliasi penuh berkala). Kegagalan
    /// per run hanya dicatat. Mengembalikan jumlah run yang berhasil.
    pub async fn refresh_active_ledgers(self: &Arc<Self>) -> usize {
        let runs = match self.runs.list_active().await {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!(error = %e, "run aktif tidak terbaca; refresh AC dilewati");
                return 0;
            }
        };
        let mut done = 0;
        for run in runs {
            match self.refresh_ledger(&run.id).await {
                Ok(r) if r.error.is_none() => done += 1,
                Ok(_) => {}
                Err(e) => tracing::warn!(run_id = %run.id, error = %e, "refresh AC gagal"),
            }
        }
        done
    }

    pub(crate) fn mark_synced(&self, run_id: &str) {
        let now = self.now();
        self.ledger_synced
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(run_id.to_owned(), now);
    }

    pub(crate) fn synced_at(&self, run_id: &str) -> Option<i64> {
        self.ledger_synced
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(run_id)
            .copied()
    }
}
