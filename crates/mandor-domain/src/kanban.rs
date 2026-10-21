//! Task Status Lark dan derivasi kolom kanban (KTD-4).

/// Nilai Task Status di Sprint Tasks.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum TaskStatus {
    ToDo,
    InProgress,
    WaitingReview,
    InReview,
    WaitingFix,
    WaitingDeploy,
    Delivered,
    /// Nilai yang tidak dikenal; app tidak pernah menulis ke record seperti ini.
    Unknown(String),
}

impl TaskStatus {
    /// Mengurai nama opsi Lark. String kosong atau field kosong menjadi `Unknown("")`.
    #[must_use]
    pub fn parse(s: &str) -> Self {
        match s {
            "To Do" => Self::ToDo,
            "In Progress" => Self::InProgress,
            "Waiting Review" => Self::WaitingReview,
            "In Review" => Self::InReview,
            "Waiting Fix" => Self::WaitingFix,
            "Waiting Deploy" => Self::WaitingDeploy,
            "Delivered" => Self::Delivered,
            other => Self::Unknown(other.to_owned()),
        }
    }

    /// Nama opsi persis seperti di Lark.
    #[must_use]
    pub fn as_str(&self) -> &str {
        match self {
            Self::ToDo => "To Do",
            Self::InProgress => "In Progress",
            Self::WaitingReview => "Waiting Review",
            Self::InReview => "In Review",
            Self::WaitingFix => "Waiting Fix",
            Self::WaitingDeploy => "Waiting Deploy",
            Self::Delivered => "Delivered",
            Self::Unknown(s) => s,
        }
    }
}

/// Kolom kanban app.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Column {
    Todo,
    InProgress,
    Review,
    Done,
    /// "Tidak dikenal": status di luar tujuh nilai yang dipetakan.
    Unknown,
}

impl Column {
    /// Nilai cermin di field Lark `kanban_column`; `None` untuk kolom tak dikenal
    /// (cermin tidak ditulis).
    #[must_use]
    pub fn mirror(self) -> Option<&'static str> {
        match self {
            Self::Todo => Some("todo"),
            Self::InProgress => Some("in_progress"),
            Self::Review => Some("review"),
            Self::Done => Some("done"),
            Self::Unknown => None,
        }
    }
}

/// Kolom turunan dari Task Status.
#[must_use]
pub fn column_of(status: &TaskStatus) -> Column {
    match status {
        TaskStatus::ToDo => Column::Todo,
        TaskStatus::InProgress | TaskStatus::WaitingFix => Column::InProgress,
        TaskStatus::WaitingReview | TaskStatus::InReview => Column::Review,
        TaskStatus::WaitingDeploy | TaskStatus::Delivered => Column::Done,
        TaskStatus::Unknown(_) => Column::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tujuh_status_terpetakan_dan_sisanya_tidak_dikenal() {
        let cases = [
            ("To Do", Column::Todo),
            ("In Progress", Column::InProgress),
            ("Waiting Fix", Column::InProgress),
            ("Waiting Review", Column::Review),
            ("In Review", Column::Review),
            ("Waiting Deploy", Column::Done),
            ("Delivered", Column::Done),
            ("Blocked", Column::Unknown),
            ("", Column::Unknown),
        ];
        for (raw, col) in cases {
            let s = TaskStatus::parse(raw);
            assert_eq!(column_of(&s), col, "{raw}");
            assert_eq!(s.as_str(), raw);
        }
    }

    #[test]
    fn cermin_sesuai_opsi_kanban_column_di_lark() {
        assert_eq!(Column::Todo.mirror(), Some("todo"));
        assert_eq!(Column::InProgress.mirror(), Some("in_progress"));
        assert_eq!(Column::Review.mirror(), Some("review"));
        assert_eq!(Column::Done.mirror(), Some("done"));
        assert_eq!(Column::Unknown.mirror(), None);
    }
}
