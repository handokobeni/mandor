//! Fallback Fix Notes saat API komentar tidak tersedia.

use mandor_domain::trust::is_trusted;
use mandor_domain::{Reply, Task};

use crate::ports::Deps;
use crate::run::{RunError, RunService};
use crate::run_ports::{RepoInfo, RunRecord, RunStore};

/// ID balasan sintetis untuk isi Fix Notes; berubah bila isinya berubah.
#[must_use]
pub fn fix_notes_reply_id(task: &Task) -> String {
    format!("fixnotes:{}", task.last_modified_ms)
}

impl<D: Deps> RunService<D> {
    /// Isi Fix Notes sebagai balasan sintetis bila pengubah terakhir record adalah
    /// penulis tepercaya. Selain itu kartu ditandai dan dev harus mengonfirmasi
    /// lewat "Mulai perbaikan".
    pub(crate) async fn fix_notes_reply(
        &self,
        task: &Task,
        repo: &RepoInfo,
        run: &mut RunRecord,
    ) -> Result<Option<Reply>, RunError> {
        let Some(notes) = task.fix_notes.as_deref().filter(|n| !n.trim().is_empty()) else {
            return Ok(None);
        };
        let author = task.last_modified_by.clone().unwrap_or_default();
        if !is_trusted(&author, task, &repo.trusted_reviewers) {
            let note = "Fix Notes are from an untrusted author; confirm via Start fix".to_owned();
            if run.note.as_deref() != Some(note.as_str()) {
                run.note = Some(note);
                self.runs.save(run, run.generation, self.now()).await?;
            }
            return Ok(None);
        }
        Ok(Some(Reply {
            record_id: task.record_id.clone(),
            comment_id: "fixnotes".to_owned(),
            reply_id: fix_notes_reply_id(task),
            author,
            create_time: task.last_modified_ms / 1000,
            text: notes.to_owned(),
        }))
    }
}
