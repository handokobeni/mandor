//! Agent pengusul perlengkapan lab (plan 2026-11-10-001, U8, KTD-L14).
//!
//! Giliran `LabSetup` membaca worktree utama repo tanpa shell, browser, berkas
//! env, maupun kredensial, lalu mengirim usulan lewat tool `lab_propose`.
//! Usulan yang lolos validasi disimpan sebagai draf di memori; developer
//! memeriksa, mengisi password, menyimpan, lalu menyetujuinya lewat `lab.*`.

use std::sync::{Arc, PoisonError};
use std::time::Duration;

use mandor_domain::AgentEvent;
use mandor_domain::lab::{LabEquipment, validate_equipment};

use super::capability::{CapabilityRole, CapabilityScope, LedgerAccess, LedgerToolError};
use crate::ports::{Deps, Ids};
use crate::run::{RunError, RunService};
use crate::run_ports::{
    AgentPort, AgentRequest, AgentRunHandle, LabTurn, Prompts, Repos, ReviewHistory, ReviewRequest,
    SessionRef, Spawner,
};

/// Batas biaya giliran pengusul, dalam sen.
pub const LAB_SETUP_BUDGET_CENTS: u32 = 100;
/// Batas waktu giliran pengusul.
pub const LAB_SETUP_TIMEOUT: Duration = Duration::from_secs(600);
/// Usulan tidak valid yang diterima tool: kiriman pertama dan satu kiriman
/// ulang.
const MAX_INVALID: u8 = 2;
/// Awalan kunci lingkup token dan penghenti giliran pengusul.
const KEY_PREFIX: &str = "lab-setup/";

const NO_PROPOSAL: &str = "The agent finished without a valid proposal";

/// Status usulan perlengkapan satu repo untuk layar pengaturan.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LabSetupStatus {
    /// Agent pengusul sedang berjalan.
    pub running: bool,
    /// Draf terakhir yang lolos validasi; belum disimpan maupun disetujui.
    pub proposal: Option<LabEquipment>,
    /// Alasan giliran terakhir tidak menghasilkan usulan.
    pub error: Option<String>,
}

/// Progres giliran pengusul satu repo.
#[derive(Debug, Default)]
pub(crate) struct SetupProgress {
    status: LabSetupStatus,
    invalid: u8,
}

fn setup_key(repo_id: &str) -> String {
    format!("{KEY_PREFIX}{repo_id}")
}

impl<D: Deps> RunService<D> {
    /// Memulai giliran pengusul perlengkapan lab di worktree utama repo
    /// (RPC `lab.propose`). Hasilnya dibaca lewat [`Self::lab_setup_status`].
    ///
    /// # Errors
    /// Repo tidak ada, URL app belum diisi, giliran pengusul repo ini masih
    /// berjalan, daemon sedang drain, atau agent gagal dimulai.
    pub async fn propose_lab(self: &Arc<Self>, repo_id: &str) -> Result<(), RunError> {
        let repo = self
            .repos
            .repo(repo_id)
            .await?
            .ok_or(RunError::NotFound("repo"))?;
        let Some(app_url) = repo.app.canonical_url() else {
            return Err(RunError::InvalidState("set the app URL first"));
        };
        let key = setup_key(repo_id);
        {
            let mut setups = self
                .lab_setups
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            let entry = setups.entry(key.clone()).or_default();
            if entry.status.running {
                return Err(RunError::InvalidState("a proposal is already running"));
            }
            *entry = SetupProgress {
                status: LabSetupStatus {
                    running: true,
                    ..LabSetupStatus::default()
                },
                invalid: 0,
            };
        }
        let review_id = format!("labsetup-{}", self.sync.ids.nonce());
        let model = self.reviews.lab_settings().await.model;
        let req = AgentRequest {
            worktree: repo.path.clone(),
            branch: repo.default_branch.clone(),
            session: SessionRef::New(self.sync.ids.nonce()),
            message: self.prompts.lab_setup_message(&app_url),
            workspace: None,
            plan_mode: false,
            ledger: Some(LedgerAccess {
                role: CapabilityRole::LabSetup,
                token: self.issue_capability(&key, CapabilityRole::LabSetup),
            }),
            design: Vec::new(),
            review: Some(ReviewRequest {
                review_id: review_id.clone(),
                origins: Vec::new(),
                browser: false,
                model,
                budget_cents: LAB_SETUP_BUDGET_CENTS,
                timeout: LAB_SETUP_TIMEOUT,
                proxy: None,
                sessions: Vec::new(),
                lab: Some(LabTurn {
                    fixtures: None,
                    setup: true,
                }),
            }),
            permit: None,
            ask: false,
            repo,
        };
        let handle = match self.agent.spawn(&req).await {
            Ok(h) => h,
            Err(e) => {
                self.end_setup(&key, &review_id, Some(e.to_string())).await;
                return Err(e.into());
            }
        };
        self.set_stopper(&key, 0, handle.stopper());
        tracing::info!(repo = %repo_id, "pengusul perlengkapan lab dimulai");
        let this = Arc::clone(self);
        self.spawner
            .spawn(async move { this.drive_setup(key, review_id, handle).await });
        Ok(())
    }

    /// Status usulan perlengkapan repo.
    #[must_use]
    pub fn lab_setup_status(&self, repo_id: &str) -> LabSetupStatus {
        self.lab_setups
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(&setup_key(repo_id))
            .map(|p| p.status.clone())
            .unwrap_or_default()
    }

    /// Tool `lab_propose` (peran `LabSetup`): usulan yang lolos validasi
    /// menjadi draf; usulan tidak valid ditolak dengan alasannya, dan agent
    /// boleh mengirim ulang sekali.
    ///
    /// # Errors
    /// Peran salah, giliran sudah berakhir, usulan sudah diterima, kiriman
    /// ulang habis, atau usulan tidak valid.
    pub async fn ledger_lab_propose(
        &self,
        scope: &CapabilityScope,
        proposal: LabEquipment,
    ) -> Result<(), LedgerToolError> {
        if scope.role != CapabilityRole::LabSetup {
            return Err(LedgerToolError::WrongRole);
        }
        let Some(repo_id) = scope.run_id.strip_prefix(KEY_PREFIX) else {
            return Err(LedgerToolError::WrongRole);
        };
        let origins: Vec<String> = self
            .repos
            .repo(repo_id)
            .await?
            .and_then(|r| r.app.canonical_url())
            .into_iter()
            .collect();
        let checked = validate_equipment(&proposal, &origins);
        let mut setups = self
            .lab_setups
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let Some(entry) = setups.get_mut(&scope.run_id).filter(|e| e.status.running) else {
            return Err(LedgerToolError::NotAllowed("the proposal turn has ended"));
        };
        if entry.status.proposal.is_some() {
            return Err(LedgerToolError::NotAllowed("a proposal was already saved"));
        }
        if entry.invalid >= MAX_INVALID {
            return Err(LedgerToolError::NotAllowed("no more proposal attempts"));
        }
        match checked {
            Ok(()) => {
                entry.status.proposal = Some(proposal);
                tracing::info!(repo = %repo_id, "usulan perlengkapan lab diterima sebagai draf");
                Ok(())
            }
            Err(e) => {
                entry.invalid += 1;
                let retry = if entry.invalid < MAX_INVALID {
                    "; fix it and call lab_propose once more"
                } else {
                    "; no attempts left"
                };
                Err(LedgerToolError::Invalid(format!(
                    "invalid proposal: {e}{retry}"
                )))
            }
        }
    }

    async fn drive_setup(
        self: Arc<Self>,
        key: String,
        review_id: String,
        mut handle: <D::Agent as AgentPort>::Run,
    ) {
        let mut error = None;
        while let Some(event) = handle.next_event().await {
            match event {
                AgentEvent::Failed { message } => error = Some(message),
                AgentEvent::Done {
                    limit: Some(limit), ..
                } => error = Some(format!("The agent stopped at its {limit} limit")),
                _ => {}
            }
        }
        self.clear_stopper(&key, 0);
        self.end_setup(&key, &review_id, error).await;
    }

    /// Akhir giliran pengusul: token dicabut, berkas sesi dibersihkan, dan
    /// status berhenti berjalan dengan alasan bila tidak ada usulan.
    async fn end_setup(&self, key: &str, review_id: &str, error: Option<String>) {
        self.revoke_capabilities(key);
        self.agent.end_review(review_id).await;
        let mut setups = self
            .lab_setups
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if let Some(entry) = setups.get_mut(key) {
            entry.status.running = false;
            if entry.status.proposal.is_none() {
                entry.status.error = Some(error.unwrap_or_else(|| NO_PROPOSAL.to_owned()));
            }
        }
    }
}
