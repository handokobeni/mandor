//! Snapshot kode run dan penanda hasil review usang (U4, R16, KTD-28).

use std::collections::BTreeMap;

use mandor_domain::ac_ledger::mark_stale;

use crate::ports::Deps;
use crate::run::{RunError, RunService};
use crate::run_ports::{GitPort, LedgerStore, RunRecord};

impl<D: Deps> RunService<D> {
    /// Tree id setiap repo anggota run.
    ///
    /// # Errors
    /// Worktree hilang atau git gagal.
    pub(crate) async fn code_snapshot(
        &self,
        run: &RunRecord,
    ) -> Result<BTreeMap<String, String>, RunError> {
        let mut out = BTreeMap::new();
        for m in self.members(run).await? {
            let tree = self.git.snapshot(&m.worktree).await?;
            out.insert(m.repo.id, tree);
        }
        Ok(out)
    }

    /// Menandai verdict usang bila kode berubah sejak review terakhir dimulai.
    /// Snapshot yang gagal dihitung dianggap berubah. Kegagalan penyimpanan
    /// hanya dicatat; review berikutnya tetap dibutuhkan oleh gerbang.
    pub(crate) async fn mark_stale_if_changed(&self, run: &RunRecord) {
        if run.review_snapshot.is_empty() {
            return;
        }
        let changed = match self.code_snapshot(run).await {
            Ok(now) => now != run.review_snapshot,
            Err(e) => {
                tracing::warn!(run_id = %run.id, error = %e, "snapshot gagal; verdict dianggap usang");
                true
            }
        };
        if !changed {
            return;
        }
        let result = async {
            let mut rows = self.reviews.ac_rows(&run.id).await?;
            let before = rows.clone();
            mark_stale(&mut rows);
            if rows != before {
                self.reviews.save_ac_rows(&run.id, &rows).await?;
            }
            Ok::<(), crate::PortError>(())
        }
        .await;
        if let Err(e) = result {
            tracing::warn!(run_id = %run.id, error = %e, "tanda usang tidak tersimpan");
        }
    }
}
