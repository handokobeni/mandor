//! Pertanyaan agent yang bisa diklik (plan 2026-11-07-001): alat
//! `ask_developer`, jawaban dev, dan antrean yang ditahan.

use std::sync::Arc;

use mandor_domain::RunState;
use mandor_domain::question::{QuestionDraft, QuestionRejected, QuestionStatus};

use super::{RunError, RunService};
use crate::ports::{Deps, Ids};
use crate::review::{CapabilityRole, CapabilityScope};
use crate::run_ports::{QuestionRecord, QuestionStore, RunPhase, RunStore};

/// Alasan panggilan `ask_developer` ditolak; pesannya dikirim ke agent.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AskError {
    #[error("this session cannot ask the developer")]
    NotAllowed,
    #[error("questions are only for brainstorm and plan turns while the agent is working")]
    NotAsking,
    #[error("you already asked a question in this turn; end your turn now and wait for the answer")]
    AlreadyAsked,
    #[error(transparent)]
    Invalid(#[from] QuestionRejected),
    #[error("storage error: {0}")]
    Storage(String),
}

impl From<crate::PortError> for AskError {
    fn from(e: crate::PortError) -> Self {
        Self::Storage(e.to_string())
    }
}

impl From<RunError> for AskError {
    fn from(e: RunError) -> Self {
        Self::Storage(e.to_string())
    }
}

/// Balasan alat yang diterima, diawali id pertanyaan agar Chat bisa menautkan
/// kartu ke langkah alat (KTD-Q2). Pendek: preview hasil tool dipotong 200
/// karakter.
fn accepted_reply(id: &str) -> String {
    format!("{id} recorded. End your turn now; the developer answers in the chat.")
}

impl<D: Deps> RunService<D> {
    /// Peran token ini mendapat `ask_developer` (laptop, peran ask atau
    /// design; KTD-Q4, KTD-Q5).
    #[must_use]
    pub fn ask_available(&self, role: CapabilityRole) -> bool {
        matches!(role, CapabilityRole::Ask | CapabilityRole::Design) && !self.is_vm()
    }

    /// Alat `ask_developer` (R1–R5): hanya giliran brainstorm/plan di laptop
    /// yang sedang bekerja, satu pertanyaan per giliran.
    ///
    /// # Errors
    /// [`AskError`] dengan alasan yang bisa diperbaiki agent.
    pub async fn ask_question(
        &self,
        scope: &CapabilityScope,
        draft: &QuestionDraft,
    ) -> Result<String, AskError> {
        if !self.ask_available(scope.role) {
            return Err(AskError::NotAllowed);
        }
        let run = self.load(&scope.run_id).await?;
        let planning = matches!(run.phase, RunPhase::Brainstorm | RunPhase::Plan);
        if !planning || run.state != RunState::Running {
            return Err(AskError::NotAsking);
        }
        draft.validate()?;
        let draft = draft.trimmed();
        let now = self.now();
        // Pertanyaan lama yang belum dijawab dari giliran sebelumnya tidak
        // relevan lagi.
        if let Some(old) = self.questions.pending_question(&run.id).await? {
            if old.generation != run.generation {
                self.questions
                    .close_question(&old.id, QuestionStatus::Skipped, None, now)
                    .await?;
            }
        }
        let record = QuestionRecord {
            id: format!("q-{}", self.sync.ids.nonce()),
            run_id: run.id.clone(),
            generation: run.generation,
            question: draft.question,
            options: draft.options,
            status: QuestionStatus::Pending,
            answer: None,
            created_ms: now,
            closed_ms: None,
        };
        if !self.questions.create_question(&record).await? {
            return Err(AskError::AlreadyAsked);
        }
        Ok(accepted_reply(&record.id))
    }

    /// Pertanyaan run, urut waktu.
    ///
    /// # Errors
    /// Penyimpanan gagal.
    pub async fn questions(&self, run_id: &str) -> Result<Vec<QuestionRecord>, RunError> {
        Ok(self.questions.questions(run_id).await?)
    }

    /// Pertanyaan Pending run, bila ada.
    pub(crate) async fn pending_question(
        &self,
        run_id: &str,
    ) -> Result<Option<QuestionRecord>, RunError> {
        Ok(self.questions.pending_question(run_id).await?)
    }

    /// Giliran `generation` berakhir dengan pertanyaan Pending (KTD-Q3).
    pub(crate) async fn asked_in_turn(&self, run_id: &str, generation: i64) -> bool {
        match self.questions.pending_question(run_id).await {
            Ok(q) => q.is_some_and(|q| q.generation == generation),
            Err(e) => {
                tracing::warn!(run_id, error = %e, "pertanyaan run tidak terbaca");
                false
            }
        }
    }

    /// Run tidak bisa di-approve selama ada pertanyaan terbuka (R15).
    pub(crate) async fn ensure_no_open_question(&self, run_id: &str) -> Result<(), RunError> {
        if self.pending_question(run_id).await?.is_some() {
            return Err(RunError::InvalidState(
                "the agent asked a question; answer it first",
            ));
        }
        Ok(())
    }

    /// Menutup pertanyaan Pending run setelah giliran berhasil dimulai.
    pub(crate) async fn close_open_question(
        &self,
        run_id: &str,
        status: QuestionStatus,
        answer: Option<&str>,
    ) -> Result<(), RunError> {
        if let Some(q) = self.pending_question(run_id).await? {
            self.questions
                .close_question(&q.id, status, answer, self.now())
                .await?;
        }
        Ok(())
    }

    /// Klik pilihan (R8): label dikirim sebagai pesan dev, lalu kartu ditutup
    /// setelah giliran mulai, sehingga kegagalan bisa dicoba lagi (R9).
    ///
    /// # Errors
    /// Pertanyaan tidak ada atau sudah dijawab, pilihan tidak dikenal, run
    /// sedang bekerja, atau agent gagal dimulai.
    pub async fn answer_question(
        self: &Arc<Self>,
        question_id: &str,
        option: usize,
    ) -> Result<(), RunError> {
        let q = self
            .questions
            .question(question_id)
            .await?
            .ok_or(RunError::NotFound("question"))?;
        if q.status != QuestionStatus::Pending {
            return Err(RunError::InvalidState("the question was already answered"));
        }
        let label = q
            .options
            .get(option)
            .map(|o| o.label.clone())
            .ok_or(RunError::InvalidState("unknown option"))?;
        let run = self.load(&q.run_id).await?;
        if run.state.is_busy() {
            return Err(RunError::InvalidState(
                "the agent is still working; answer when the turn ends",
            ));
        }
        self.start_dev_turn(run, label.clone()).await?;
        self.questions
            .close_question(&q.id, QuestionStatus::Answered, Some(&label), self.now())
            .await?;
        Ok(())
    }

    /// Pesan antrean yang ditahan dikirim sebagai satu pesan dan menjawab
    /// pertanyaan lewat chat (R13).
    ///
    /// # Errors
    /// Antrean kosong, run sedang bekerja, atau agent gagal dimulai.
    pub async fn send_queued(self: &Arc<Self>, run_id: &str) -> Result<(), RunError> {
        let mut run = self.load(run_id).await?;
        if run.state.is_busy() {
            return Err(RunError::InvalidState(run.state.as_str()));
        }
        if run.queue.is_empty() {
            return Err(RunError::InvalidState("no queued messages"));
        }
        let text = run.queue.join("\n\n");
        run.queue.clear();
        self.start_dev_turn(run, text.clone()).await?;
        self.close_open_question(run_id, QuestionStatus::AnsweredInChat, Some(&text))
            .await
    }

    /// Membuang pesan antrean yang ditahan; pertanyaan tetap terbuka (R13).
    ///
    /// # Errors
    /// Run sedang bekerja atau penyimpanan gagal.
    pub async fn discard_queued(&self, run_id: &str) -> Result<(), RunError> {
        let mut run = self.load(run_id).await?;
        if run.state.is_busy() {
            return Err(RunError::InvalidState(run.state.as_str()));
        }
        run.queue.clear();
        self.runs.save(&run, run.generation, self.now()).await?;
        Ok(())
    }

    /// Giliran baru dengan pesan dev; prasyarat sama dengan pesan chat.
    async fn start_dev_turn(
        self: &Arc<Self>,
        mut run: crate::run_ports::RunRecord,
        text: String,
    ) -> Result<(), RunError> {
        let _work = self.enter_work()?;
        if run.state.is_parked() || matches!(run.state, RunState::Preparing | RunState::Publishing)
        {
            return Err(RunError::InvalidState(run.state.as_str()));
        }
        let req = self.resume_request(&mut run, text.clone()).await?;
        self.spawn_turn(&mut run, req, Some(text)).await
    }
}

#[cfg(test)]
mod tests {
    use super::accepted_reply;

    #[test]
    fn balasan_alat_pendek_dan_diawali_id() {
        let reply = accepted_reply("q-0123456789abcdef0123456789abcdef");
        assert!(reply.starts_with("q-0123"));
        assert!(reply.chars().count() < 200);
    }
}
