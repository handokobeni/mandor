//! Task dari Sprint Tasks, tulisan milik app, dan validasi ulang outbox (KTD-7).

use crate::claim::Claim;
use crate::gate::transition_allowed;
use crate::kanban::{Column, TaskStatus, column_of};

/// Record Sprint Tasks dalam bentuk domain. Field milik manusia hanya dibaca.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Task {
    pub record_id: String,
    /// `PGS-xxxx`.
    pub key: Option<String>,
    pub title: String,
    pub description: String,
    pub status: TaskStatus,
    pub readiness: Option<String>,
    pub assignees: Vec<String>,
    pub reporter: Option<String>,
    pub ac_ids: Vec<String>,
    pub sprint_ids: Vec<String>,
    pub branch: Option<String>,
    pub pr_url: Option<String>,
    pub claim: Option<Claim>,
    pub comment_cursor: Option<String>,
    pub kanban_mirror: Option<String>,
    pub fix_notes: Option<String>,
    /// Repo lokal tempat task dikerjakan (state tampilan milik app).
    pub repo_id: Option<String>,
    pub last_modified_ms: i64,
    pub last_modified_by: Option<String>,
}

impl Task {
    /// Record kosong dengan ID tertentu (berguna untuk test dan fake).
    #[must_use]
    pub fn new(record_id: &str, status: TaskStatus) -> Self {
        Self {
            record_id: record_id.to_owned(),
            key: None,
            title: String::new(),
            description: String::new(),
            status,
            readiness: None,
            assignees: Vec::new(),
            reporter: None,
            ac_ids: Vec::new(),
            sprint_ids: Vec::new(),
            branch: None,
            pr_url: None,
            claim: None,
            comment_cursor: None,
            kanban_mirror: None,
            fix_notes: None,
            repo_id: None,
            last_modified_ms: 0,
            last_modified_by: None,
        }
    }

    #[must_use]
    pub fn column(&self) -> Column {
        column_of(&self.status)
    }

    #[must_use]
    pub fn assigned_to(&self, open_id: &str) -> bool {
        self.assignees.iter().any(|a| a == open_id)
    }

    /// `true` bila klaim di record tercatat atas nama `open_id` (aktif atau tidak).
    #[must_use]
    pub fn claimed_by(&self, open_id: &str) -> bool {
        self.claim.as_ref().is_some_and(|c| c.open_id == open_id)
    }

    /// Nilai cermin `kanban_column` yang perlu ditulis, bila berbeda dari yang ada.
    #[must_use]
    pub fn mirror_update(&self) -> Option<&'static str> {
        let want = self.column().mirror()?;
        (self.kanban_mirror.as_deref() != Some(want)).then_some(want)
    }

    /// Menerapkan tulisan app ke salinan record. Respons update Bitable hanya memuat
    /// field yang ditulis, jadi field lain diambil dari record yang sudah ada.
    pub fn apply_write(&mut self, status: Option<&TaskStatus>, fields: &[AppField]) {
        if let Some(s) = status {
            self.status = s.clone();
        }
        for f in fields {
            match f {
                AppField::Branch(b) => self.branch = Some(b.clone()),
                AppField::PrUrl(u) => self.pr_url = Some(u.clone()),
                AppField::Claim(c) => self.claim.clone_from(c),
                AppField::CommentCursor(c) => self.comment_cursor = Some(c.clone()),
                AppField::KanbanMirror(m) => self.kanban_mirror = Some(m.clone()),
                AppField::RepoId(r) => self.repo_id = Some(r.clone()),
                // Hanya untuk tim di tracker; tidak dibaca kembali oleh app.
                AppField::DesignGaps(_) => {}
            }
        }
    }
}

/// Satu field milik app yang ditulis ke Lark.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppField {
    Branch(String),
    PrUrl(String),
    /// `None` mengosongkan klaim.
    Claim(Option<Claim>),
    CommentCursor(String),
    KanbanMirror(String),
    RepoId(String),
    /// Selisih desain per AC (`mandor_design_gaps`, KTD-34); kosong = tidak ada.
    DesignGaps(String),
}

/// Operasi tulis di outbox.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WriteOp {
    /// Transisi status beserta field milik app yang ikut ditulis dalam satu PUT.
    Transition {
        from: TaskStatus,
        to: TaskStatus,
        fields: Vec<AppField>,
    },
    /// Hanya field milik app.
    Fields(Vec<AppField>),
}

/// Alasan tulisan dibatalkan saat validasi ulang.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum WriteRejected {
    #[error("record was deleted in Lark")]
    Deleted,
    #[error("record is no longer assigned to you")]
    Unassigned,
    #[error("the owner claim is no longer yours")]
    NotOwner,
    #[error("status in Lark is now \"{actual}\", not \"{expected}\"")]
    StatusChanged { expected: String, actual: String },
    #[error("transition {from} → {to} is not allowed for the app")]
    NotAllowed { from: String, to: String },
}

/// Memeriksa transisi saat diantrekan; transisi di luar allowlist ditolak di tempat.
///
/// # Errors
/// [`WriteRejected::NotAllowed`].
pub fn check_enqueue(op: &WriteOp) -> Result<(), WriteRejected> {
    match op {
        WriteOp::Transition { from, to, .. } if !transition_allowed(from, to) => {
            Err(WriteRejected::NotAllowed {
                from: from.as_str().to_owned(),
                to: to.as_str().to_owned(),
            })
        }
        _ => Ok(()),
    }
}

/// Validasi ulang sebelum PUT, terhadap baca segar record.
///
/// # Errors
/// [`WriteRejected`] bila record, assignee, pemilik, atau status sumber sudah
/// tidak sesuai.
pub fn revalidate(fresh: Option<&Task>, op: &WriteOp, me: &str) -> Result<(), WriteRejected> {
    check_enqueue(op)?;
    let task = fresh.ok_or(WriteRejected::Deleted)?;
    if !task.assigned_to(me) {
        return Err(WriteRejected::Unassigned);
    }
    if !task.claimed_by(me) {
        return Err(WriteRejected::NotOwner);
    }
    if let WriteOp::Transition { from, .. } = op
        && task.status != *from
    {
        return Err(WriteRejected::StatusChanged {
            expected: from.as_str().to_owned(),
            actual: task.status.as_str().to_owned(),
        });
    }
    Ok(())
}

/// Jenis penanda kartu (R6, R6a).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlagKind {
    /// Tulisan app dibatalkan karena nilai di Lark berubah; Lark menang.
    Overwritten,
    /// Tulis atau baca gagal berulang.
    SyncFailed,
    /// Record dihapus di Lark.
    Deleted,
    /// Record tidak lagi di-assign ke pengguna.
    Unassigned,
}

impl FlagKind {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Overwritten => "overwritten",
            Self::SyncFailed => "sync_failed",
            Self::Deleted => "deleted",
            Self::Unassigned => "unassigned",
        }
    }

    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "overwritten" => Some(Self::Overwritten),
            "sync_failed" => Some(Self::SyncFailed),
            "deleted" => Some(Self::Deleted),
            "unassigned" => Some(Self::Unassigned),
            _ => None,
        }
    }
}

/// Penanda yang ditampilkan di kartu beserta penyebabnya.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CardFlag {
    pub kind: FlagKind,
    pub detail: String,
}

impl From<&WriteRejected> for CardFlag {
    fn from(r: &WriteRejected) -> Self {
        let kind = match r {
            WriteRejected::Deleted => FlagKind::Deleted,
            WriteRejected::Unassigned => FlagKind::Unassigned,
            WriteRejected::NotOwner
            | WriteRejected::StatusChanged { .. }
            | WriteRejected::NotAllowed { .. } => FlagKind::Overwritten,
        };
        Self {
            kind,
            detail: r.to_string(),
        }
    }
}

/// Komentar perlu diambil bila ada record milik pengguna berstatus Waiting Fix
/// atau yang sedang dalam loop perbaikan lokal (KTD-6).
#[must_use]
pub fn needs_comment_poll<'a>(
    tasks: impl IntoIterator<Item = &'a Task>,
    in_fix_loop: &[String],
    me: &str,
) -> bool {
    !in_fix_loop.is_empty()
        || tasks
            .into_iter()
            .any(|t| t.status == TaskStatus::WaitingFix && t.assigned_to(me))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mine(status: TaskStatus) -> Task {
        let mut t = Task::new("rec1", status);
        t.assignees = vec!["ou_me".to_owned()];
        t.claim = Some(Claim {
            open_id: "ou_me".to_owned(),
            ts_ms: 0,
            nonce: "n".to_owned(),
        });
        t
    }

    #[test]
    fn tulisan_app_menimpa_field_yang_ditulis_dan_mempertahankan_sisanya() {
        let mut t = mine(TaskStatus::WaitingFix);
        t.key = Some("PGS-1510".to_owned());
        t.title = "TEST Mandor".to_owned();
        t.ac_ids = vec!["ac1".to_owned()];
        t.branch = Some("lama".to_owned());

        t.apply_write(
            Some(&TaskStatus::InProgress),
            &[
                AppField::Branch("mandor/pgs-1510".to_owned()),
                AppField::PrUrl("https://github.com/o/r/pull/1".to_owned()),
                AppField::Claim(None),
                AppField::CommentCursor("1:2".to_owned()),
                AppField::KanbanMirror("in_progress".to_owned()),
                AppField::RepoId("mandor-smoke".to_owned()),
            ],
        );

        assert_eq!(t.status, TaskStatus::InProgress);
        assert_eq!(t.branch.as_deref(), Some("mandor/pgs-1510"));
        assert_eq!(t.pr_url.as_deref(), Some("https://github.com/o/r/pull/1"));
        assert_eq!(t.claim, None);
        assert_eq!(t.comment_cursor.as_deref(), Some("1:2"));
        assert_eq!(t.kanban_mirror.as_deref(), Some("in_progress"));
        assert_eq!(t.repo_id.as_deref(), Some("mandor-smoke"));
        // Field yang tidak ditulis tetap seperti semula.
        assert_eq!(t.key.as_deref(), Some("PGS-1510"));
        assert_eq!(t.title, "TEST Mandor");
        assert_eq!(t.assignees, ["ou_me"]);
        assert_eq!(t.ac_ids, ["ac1"]);
    }

    #[test]
    fn tulisan_tanpa_status_tidak_mengubah_status() {
        let mut t = mine(TaskStatus::WaitingReview);
        t.apply_write(None, &[AppField::CommentCursor("9:9".to_owned())]);
        assert_eq!(t.status, TaskStatus::WaitingReview);
        assert_eq!(t.comment_cursor.as_deref(), Some("9:9"));
    }

    fn start_op() -> WriteOp {
        WriteOp::Transition {
            from: TaskStatus::ToDo,
            to: TaskStatus::InProgress,
            fields: vec![AppField::Branch("mandor/pgs-1".to_owned())],
        }
    }

    #[test]
    fn revalidasi_lolos_bila_semua_masih_sesuai() {
        assert_eq!(
            revalidate(Some(&mine(TaskStatus::ToDo)), &start_op(), "ou_me"),
            Ok(())
        );
    }

    #[test]
    fn revalidasi_membatalkan_bila_status_bukan_lagi_to_do() {
        let err = revalidate(Some(&mine(TaskStatus::InReview)), &start_op(), "ou_me").unwrap_err();
        assert!(matches!(err, WriteRejected::StatusChanged { .. }));
        assert_eq!(CardFlag::from(&err).kind, FlagKind::Overwritten);
    }

    #[test]
    fn revalidasi_mendeteksi_hapus_pindah_assignee_dan_pemilik() {
        assert_eq!(
            revalidate(None, &start_op(), "ou_me"),
            Err(WriteRejected::Deleted)
        );
        let mut t = mine(TaskStatus::ToDo);
        t.assignees = vec!["ou_other".to_owned()];
        assert_eq!(
            revalidate(Some(&t), &start_op(), "ou_me"),
            Err(WriteRejected::Unassigned)
        );
        let mut t = mine(TaskStatus::ToDo);
        t.claim = None;
        assert_eq!(
            revalidate(Some(&t), &start_op(), "ou_me"),
            Err(WriteRejected::NotOwner)
        );
    }

    #[test]
    fn transisi_terlarang_ditolak_saat_diantrekan() {
        let op = WriteOp::Transition {
            from: TaskStatus::WaitingReview,
            to: TaskStatus::InReview,
            fields: vec![],
        };
        assert!(matches!(
            check_enqueue(&op),
            Err(WriteRejected::NotAllowed { .. })
        ));
        assert_eq!(check_enqueue(&WriteOp::Fields(vec![])), Ok(()));
    }

    #[test]
    fn cermin_hanya_ditulis_bila_berubah() {
        let mut t = mine(TaskStatus::WaitingFix);
        assert_eq!(t.mirror_update(), Some("in_progress"));
        t.kanban_mirror = Some("in_progress".to_owned());
        assert_eq!(t.mirror_update(), None);
        t.status = TaskStatus::Unknown("Blocked".to_owned());
        assert_eq!(t.mirror_update(), None);
    }

    #[test]
    fn komentar_hanya_dipoll_saat_ada_waiting_fix_atau_loop_perbaikan() {
        let idle = [
            mine(TaskStatus::InProgress),
            mine(TaskStatus::WaitingReview),
        ];
        assert!(!needs_comment_poll(&idle, &[], "ou_me"));
        assert!(needs_comment_poll(&idle, &["rec1".to_owned()], "ou_me"));
        let wf = [mine(TaskStatus::WaitingFix)];
        assert!(needs_comment_poll(&wf, &[], "ou_me"));
        assert!(!needs_comment_poll(&wf, &[], "ou_other"));
    }

    #[test]
    fn flag_kind_bolak_balik() {
        for k in [
            FlagKind::Overwritten,
            FlagKind::SyncFailed,
            FlagKind::Deleted,
            FlagKind::Unassigned,
        ] {
            assert_eq!(FlagKind::parse(k.as_str()), Some(k));
        }
        assert_eq!(FlagKind::parse("x"), None);
    }
}
