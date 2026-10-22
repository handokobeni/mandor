//! Penyimpanan Rilis 2 sebagai port usecase.

use mandor_domain::AcRow;
use mandor_usecase::PortError;
use mandor_usecase::run_ports::LedgerStore;

use crate::Store;

/// Buku AC, review, sumber desain, dan setelan untuk usecase.
#[derive(Debug, Clone)]
pub struct ReviewsStore {
    store: Store,
}

impl ReviewsStore {
    #[must_use]
    pub fn new(store: Store) -> Self {
        Self { store }
    }
}

impl LedgerStore for ReviewsStore {
    async fn ac_rows(&self, run_id: &str) -> Result<Vec<AcRow>, PortError> {
        Ok(self.store.ac_rows(run_id).await?)
    }

    async fn save_ac_rows(&self, run_id: &str, rows: &[AcRow]) -> Result<(), PortError> {
        Ok(self.store.save_ac_rows(run_id, rows).await?)
    }
}
