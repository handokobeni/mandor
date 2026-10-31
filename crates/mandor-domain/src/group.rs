//! Aturan run grup: kartu satu flow dikerjakan berurutan di satu branch
//! (plan `docs/plans/2026-10-23-004-feat-grouped-card-runs-plan.md`).
//!
//! Semua keputusan murni: kandidat grup, kartu berikutnya, kapan grup
//! berhenti, kartu yang berubah di Lark, dan kartu yang dipindah status saat
//! publish. Penyimpanan dan efek samping ada di usecase.

use crate::gate::{GateRejection, check_start};
use crate::kanban::TaskStatus;
use crate::run::HoldReason;
use crate::task::Task;

/// Keadaan satu kartu di run grup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CardState {
    /// Belum dikerjakan.
    Pending,
    /// Sedang dikerjakan (plan atau implementasi).
    Active,
    /// Selesai dan sudah di-commit lokal (atau tanpa perubahan kode).
    Done,
    /// Dikeluarkan dari grup.
    Removed(RemovedReason),
}

impl CardState {
    /// Nama stabil untuk penyimpanan dan protokol.
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Active => "active",
            Self::Done => "done",
            Self::Removed(RemovedReason::ChangedInLark) => "removed_changed",
            Self::Removed(RemovedReason::ByDev) => "removed_by_dev",
            Self::Removed(RemovedReason::Reverted) => "removed_reverted",
            Self::Removed(RemovedReason::FollowUp) => "removed_follow_up",
        }
    }

    /// Kebalikan [`CardState::as_str`]; nilai tak dikenal menjadi `None`.
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "pending" => Self::Pending,
            "active" => Self::Active,
            "done" => Self::Done,
            "removed_changed" => Self::Removed(RemovedReason::ChangedInLark),
            "removed_by_dev" => Self::Removed(RemovedReason::ByDev),
            "removed_reverted" => Self::Removed(RemovedReason::Reverted),
            "removed_follow_up" => Self::Removed(RemovedReason::FollowUp),
            _ => return None,
        })
    }
}

/// Alasan kartu dikeluarkan dari grup.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemovedReason {
    /// Status atau PIC berubah di Lark sebelum giliran kartu (R9).
    ChangedInLark,
    /// Dev mengeluarkan kartu yang belum dimulai, atau menghentikan grup (R10).
    ByDev,
    /// Commit kartu di-revert di gerbang (R13).
    Reverted,
    /// PR grup sudah merged saat kartu masuk Waiting Fix; perbaikan pindah ke
    /// run lanjutan dari branch default (R19).
    FollowUp,
}

/// Satu kartu di run grup, urut sesuai posisi.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupCard {
    pub record_id: String,
    pub state: CardState,
    /// Kartu `Done` yang berubah status atau PIC di Lark (R13); diputuskan di
    /// gerbang.
    pub changed_in_lark: bool,
    /// Alasan dev tetap menyertakan kartu yang berubah di Lark (R13).
    pub kept_reason: Option<String>,
}

impl GroupCard {
    /// Kartu baru yang belum dikerjakan.
    #[must_use]
    pub fn pending(record_id: &str) -> Self {
        Self {
            record_id: record_id.to_owned(),
            state: CardState::Pending,
            changed_in_lark: false,
            kept_reason: None,
        }
    }

    /// Kartu masih bagian grup (belum dikeluarkan).
    #[must_use]
    pub fn is_active(&self) -> bool {
        !matches!(self.state, CardState::Removed(_))
    }
}

/// Alasan kartu tidak bisa dipilih untuk grup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Unselectable {
    /// Gerbang Start menolak (status bukan To Do atau readiness belum siap).
    Gate(GateRejection),
    /// Kartu sudah punya run terbuka.
    HasRun,
    /// Kartu ada di antrean VM atau dipegang daemon VM.
    OnVm,
}

impl std::fmt::Display for Unselectable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Gate(g) => write!(f, "{g}"),
            Self::HasRun => f.write_str("Task already has an active run"),
            Self::OnVm => f.write_str("Task is queued for or held by the VM"),
        }
    }
}

/// Satu kandidat grup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    pub record_id: String,
    pub key: Option<String>,
    pub title: String,
    /// `None` bila bisa dipilih.
    pub blocked: Option<Unselectable>,
}

/// Kandidat "Start together" untuk kartu `anchor` (R1, R2): kartu dengan epic
/// yang sama dan PIC `me`, diurutkan menurut nomor kartu. Kartu milik orang
/// lain tidak ditampilkan. `busy` memberi alasan run/VM per kartu.
#[must_use]
pub fn candidates(
    anchor: &Task,
    tasks: &[Task],
    me: &str,
    busy: &dyn Fn(&str) -> Option<Unselectable>,
) -> Vec<Candidate> {
    let mut out: Vec<Candidate> = tasks
        .iter()
        .filter(|t| t.assignees.iter().any(|a| a == me))
        .filter(|t| {
            t.record_id == anchor.record_id
                || (!anchor.epic_ids.is_empty()
                    && t.epic_ids.iter().any(|e| anchor.epic_ids.contains(e)))
        })
        .filter(|t| t.status == TaskStatus::ToDo || t.record_id == anchor.record_id)
        .map(|t| Candidate {
            record_id: t.record_id.clone(),
            key: t.key.clone(),
            title: t.title.clone(),
            blocked: busy(&t.record_id).or_else(|| {
                check_start(&t.status, t.readiness.as_deref())
                    .err()
                    .map(Unselectable::Gate)
            }),
        })
        .collect();
    out.sort_by_key(|c| card_number(c.key.as_deref()));
    out
}

/// Nomor kartu dari kunci `PGS-1401` untuk urutan default (R3); kartu tanpa
/// nomor di akhir.
#[must_use]
pub fn card_number(key: Option<&str>) -> u64 {
    key.and_then(|k| k.rsplit('-').next())
        .and_then(|n| n.parse().ok())
        .unwrap_or(u64::MAX)
}

/// Nama branch grup: `mandor/<kunci-awal>-<nomor-akhir>-<slug judul>`, mis.
/// `mandor/pgs-1401-1404-job-position`; batas panjang sama dengan run tunggal.
#[must_use]
pub fn branch_name(
    first_key: Option<&str>,
    last_key: Option<&str>,
    record_id: &str,
    title: &str,
) -> String {
    let head = match (first_key, last_key.and_then(|k| k.rsplit('-').next())) {
        (Some(first), Some(last)) => format!("{first}-{last}"),
        (Some(first), None) => first.to_owned(),
        (None, _) => record_id.to_owned(),
    };
    crate::run::branch_name(Some(&head), record_id, title)
}

/// Kartu berikutnya yang belum dikerjakan.
#[must_use]
pub fn next_pending(cards: &[GroupCard]) -> Option<&GroupCard> {
    cards.iter().find(|c| c.state == CardState::Pending)
}

/// Kartu yang sedang dikerjakan.
#[must_use]
pub fn active_card(cards: &[GroupCard]) -> Option<&GroupCard> {
    cards.iter().find(|c| c.state == CardState::Active)
}

/// Semua kartu aktif sudah selesai: tidak ada yang menunggu atau dikerjakan.
#[must_use]
pub fn all_done(cards: &[GroupCard]) -> bool {
    cards.iter().any(GroupCard::is_active)
        && cards
            .iter()
            .all(|c| matches!(c.state, CardState::Done | CardState::Removed(_)))
}

/// Kartu masih milik flow dev di Lark: In Progress dan `me` tetap PIC (R9,
/// R13). `None` bila kartu dihapus.
#[must_use]
pub fn still_ours(task: Option<&Task>, me: &str) -> bool {
    task.is_some_and(|t| t.status == TaskStatus::InProgress && t.assignees.iter().any(|a| a == me))
}

/// Tindakan atas kehilangan klaim sebuah kartu grup (KTD-G5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LossAction {
    /// Kartu yang sedang dikerjakan: run berhenti seperti run tunggal.
    StopRun,
    /// Kartu lain: hanya ditandai; R9 atau R13 yang menangani.
    MarkCard,
}

/// Kehilangan klaim pada kartu dengan keadaan `state`.
#[must_use]
pub fn on_claim_loss(state: &CardState) -> LossAction {
    if *state == CardState::Active {
        LossAction::StopRun
    } else {
        LossAction::MarkCard
    }
}

/// Kartu yang dipindah ke Waiting Review saat publish (R14): kartu aktif yang
/// tidak berubah di Lark.
#[must_use]
pub fn publish_moves(cards: &[GroupCard]) -> Vec<&str> {
    cards
        .iter()
        .filter(|c| c.state == CardState::Done && !c.changed_in_lark)
        .map(|c| c.record_id.as_str())
        .collect()
}

/// Kartu `Done` yang berubah di Lark dan belum diputuskan dev; menahan
/// gerbang (R13).
#[must_use]
pub fn gate_holds(cards: &[GroupCard]) -> Vec<&str> {
    cards
        .iter()
        .filter(|c| c.state == CardState::Done && c.changed_in_lark && c.kept_reason.is_none())
        .map(|c| c.record_id.as_str())
        .collect()
}

/// Keputusan dev atas kartu yang berubah di Lark (R13).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CardDecision {
    /// Revert commit kartu; kartu keluar dari grup.
    Revert,
    /// Tetap disertakan; alasan dicatat di PR.
    Keep { reason: String },
}

/// Judul PR grup (KTD-G7): `<KEY-awal>–<KEY-akhir>: <epic>`, atau judul kartu
/// pertama bila nama epic tidak ada. Satu kartu tersisa memakai satu kunci.
#[must_use]
pub fn pr_title(keys: &[&str], epic: Option<&str>, first_title: &str) -> String {
    let name = epic.filter(|e| !e.trim().is_empty()).unwrap_or(first_title);
    match keys {
        [] => name.to_owned(),
        [one] => format!("{one}: {name}"),
        [first, .., last] => format!("{first}–{last}: {name}"),
    }
}

/// Keadaan daemon VM saat satu kartu grup selesai.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PauseInput {
    /// Jam lembur sedang berjalan.
    pub in_window: bool,
    /// Grup dimulai dengan "Start now".
    pub start_now: bool,
    /// Ada Waiting Fix kartu VM lain yang siap dikerjakan.
    pub fix_ready: bool,
}

/// Grup VM berjeda sebelum kartu berikutnya (R20, R21, KTD-G12)? Di luar jam,
/// grup biasa menunggu siklus berikutnya; grup Start now tetap jalan kecuali
/// ada Waiting Fix yang harus didahulukan.
#[must_use]
pub fn vm_pause(s: &PauseInput) -> Option<HoldReason> {
    if !s.in_window && !s.start_now {
        Some(HoldReason::WindowEnded)
    } else if s.fix_ready {
        Some(HoldReason::YieldToFix)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jeda_grup_vm_sebelum_kartu_berikutnya() {
        let at = |in_window, start_now, fix_ready| {
            vm_pause(&PauseInput {
                in_window,
                start_now,
                fix_ready,
            })
        };
        assert_eq!(at(true, false, false), None, "lanjut");
        assert_eq!(at(false, false, false), Some(HoldReason::WindowEnded));
        assert_eq!(
            at(false, true, false),
            None,
            "Start now tidak berhenti karena jam"
        );
        assert_eq!(at(true, false, true), Some(HoldReason::YieldToFix));
        assert_eq!(at(false, true, true), Some(HoldReason::YieldToFix));
        assert_eq!(
            at(false, false, true),
            Some(HoldReason::WindowEnded),
            "di luar jam grup menunggu siklus berikutnya"
        );
    }

    fn task(id: &str, key: &str, status: TaskStatus, pic: &str, epic: &str) -> Task {
        let mut t = Task::new(id, status);
        t.key = Some(key.to_owned());
        t.title = format!("Judul {key}");
        t.readiness = Some("Ready To Dev".to_owned());
        t.assignees = vec![pic.to_owned()];
        if !epic.is_empty() {
            t.epic_ids = vec![epic.to_owned()];
        }
        t
    }

    #[test]
    fn nama_branch_grup() {
        assert_eq!(
            branch_name(Some("PGS-1401"), Some("PGS-1404"), "r", "Job & Position"),
            "mandor/pgs-1401-1404-job-position"
        );
        assert_eq!(
            branch_name(None, None, "recX", "Judul"),
            "mandor/recx-judul"
        );
        assert!(branch_name(Some("PGS-1"), Some("PGS-9"), "r", &"x".repeat(200)).len() <= 60);
    }

    #[test]
    fn kandidat_epic_sama_pic_saya_dengan_alasan() {
        // Covers AE1.
        let anchor = task("r1", "PGS-1401", TaskStatus::ToDo, "ou_beni", "ep");
        let mut not_ready = task("r5", "PGS-1406", TaskStatus::ToDo, "ou_beni", "ep");
        not_ready.readiness = Some("Draft".to_owned());
        let tasks = vec![
            task("r4", "PGS-1404", TaskStatus::ToDo, "ou_beni", "ep"),
            anchor.clone(),
            task("r2", "PGS-1402", TaskStatus::ToDo, "ou_beni", "ep"),
            task("r3", "PGS-1403", TaskStatus::ToDo, "ou_beni", "ep"),
            task("rA", "PGS-1405", TaskStatus::ToDo, "ou_arsya", "ep"),
            task("rD", "PGS-1300", TaskStatus::Delivered, "ou_beni", "ep"),
            task("rX", "PGS-1500", TaskStatus::ToDo, "ou_beni", "lain"),
            not_ready,
        ];
        let busy = |id: &str| match id {
            "r2" => Some(Unselectable::HasRun),
            "r4" => Some(Unselectable::OnVm),
            _ => None,
        };
        let c = candidates(&anchor, &tasks, "ou_beni", &busy);
        let ids: Vec<&str> = c.iter().map(|c| c.record_id.as_str()).collect();
        assert_eq!(
            ids,
            ["r1", "r2", "r3", "r4", "r5"],
            "urut nomor, tanpa milik orang lain"
        );
        assert_eq!(c[0].blocked, None);
        assert_eq!(c[1].blocked, Some(Unselectable::HasRun));
        assert_eq!(c[2].blocked, None);
        assert_eq!(c[3].blocked, Some(Unselectable::OnVm));
        assert!(matches!(c[4].blocked, Some(Unselectable::Gate(_))));
    }

    #[test]
    fn kartu_tanpa_epic_hanya_dirinya() {
        let anchor = task("r1", "PGS-1", TaskStatus::ToDo, "ou_b", "");
        let other = task("r2", "PGS-2", TaskStatus::ToDo, "ou_b", "");
        let c = candidates(&anchor, &[anchor.clone(), other], "ou_b", &|_| None);
        assert_eq!(c.len(), 1);
    }

    #[test]
    fn nomor_kartu_untuk_urutan() {
        assert_eq!(card_number(Some("PGS-1401")), 1401);
        assert_eq!(card_number(Some("tanpa")), u64::MAX);
        assert_eq!(card_number(None), u64::MAX);
    }

    #[test]
    fn kartu_berikutnya_dan_selesai() {
        let mut cards = vec![
            GroupCard::pending("a"),
            GroupCard::pending("b"),
            GroupCard::pending("c"),
        ];
        assert!(!all_done(&cards));
        cards[0].state = CardState::Done;
        cards[1].state = CardState::Active;
        assert_eq!(
            next_pending(&cards).map(|c| c.record_id.as_str()),
            Some("c")
        );
        assert_eq!(active_card(&cards).map(|c| c.record_id.as_str()), Some("b"));
        cards[1].state = CardState::Done;
        cards[2].state = CardState::Removed(RemovedReason::ChangedInLark);
        assert!(all_done(&cards), "kartu dikeluarkan tidak menunggu");
        assert!(next_pending(&cards).is_none());
        let removed: Vec<GroupCard> = cards
            .iter()
            .map(|c| GroupCard {
                state: CardState::Removed(RemovedReason::ByDev),
                ..c.clone()
            })
            .collect();
        assert!(!all_done(&removed), "tanpa kartu aktif bukan selesai");
    }

    #[test]
    fn kartu_masih_milik_flow() {
        let t = task("r", "PGS-1", TaskStatus::InProgress, "ou_b", "ep");
        assert!(still_ours(Some(&t), "ou_b"));
        assert!(!still_ours(Some(&t), "ou_lain"), "PIC berubah");
        let blocked = task(
            "r",
            "PGS-1",
            TaskStatus::Unknown("Blocked".to_owned()),
            "ou_b",
            "ep",
        );
        assert!(!still_ours(Some(&blocked), "ou_b"), "status berubah");
        assert!(!still_ours(None, "ou_b"), "kartu dihapus");
    }

    #[test]
    fn kehilangan_klaim_per_kartu() {
        assert_eq!(on_claim_loss(&CardState::Active), LossAction::StopRun);
        assert_eq!(on_claim_loss(&CardState::Pending), LossAction::MarkCard);
        assert_eq!(on_claim_loss(&CardState::Done), LossAction::MarkCard);
    }

    #[test]
    fn publish_dan_gerbang_memisahkan_kartu_berubah() {
        let mut cards = vec![
            GroupCard::pending("a"),
            GroupCard::pending("b"),
            GroupCard::pending("c"),
        ];
        for c in &mut cards {
            c.state = CardState::Done;
        }
        cards[0].changed_in_lark = true;
        cards[2].state = CardState::Removed(RemovedReason::Reverted);
        assert_eq!(publish_moves(&cards), ["b"]);
        assert_eq!(gate_holds(&cards), ["a"]);
        cards[0].kept_reason = Some("QA butuh kodenya".to_owned());
        assert!(gate_holds(&cards).is_empty(), "sudah diputuskan tetap");
        assert_eq!(publish_moves(&cards), ["b"], "tetap tidak dipindah");
    }

    #[test]
    fn judul_pr_grup() {
        assert_eq!(
            pr_title(&["PGS-1", "PGS-2", "PGS-4"], Some("Job"), "Daftar"),
            "PGS-1–PGS-4: Job"
        );
        assert_eq!(pr_title(&["PGS-2"], Some("Job"), "Daftar"), "PGS-2: Job");
        assert_eq!(
            pr_title(&["PGS-1", "PGS-2"], None, "Daftar"),
            "PGS-1–PGS-2: Daftar",
            "epic tidak terbaca"
        );
        assert_eq!(pr_title(&[], Some(" "), "Daftar"), "Daftar");
    }

    #[test]
    fn nama_state_bolak_balik() {
        for s in [
            CardState::Pending,
            CardState::Active,
            CardState::Done,
            CardState::Removed(RemovedReason::ChangedInLark),
            CardState::Removed(RemovedReason::ByDev),
            CardState::Removed(RemovedReason::Reverted),
            CardState::Removed(RemovedReason::FollowUp),
        ] {
            assert_eq!(CardState::parse(s.as_str()), Some(s));
        }
        assert_eq!(CardState::parse("x"), None);
    }

    #[test]
    fn alasan_tidak_bisa_dipilih_untuk_pengguna() {
        assert_eq!(
            Unselectable::HasRun.to_string(),
            "Task already has an active run"
        );
        assert!(Unselectable::OnVm.to_string().contains("VM"));
    }
}
