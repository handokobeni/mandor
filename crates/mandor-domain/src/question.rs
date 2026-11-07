//! Pertanyaan agent yang bisa diklik (plan 2026-11-07-001): validasi isi
//! `ask_developer` dan status kartu pertanyaan.

/// Batas panjang teks pertanyaan (karakter).
pub const QUESTION_MAX: usize = 500;
/// Batas panjang label pilihan (karakter).
pub const LABEL_MAX: usize = 80;
/// Batas panjang penjelasan pilihan (karakter).
pub const DESCRIPTION_MAX: usize = 300;
/// Jumlah pilihan paling sedikit.
pub const OPTIONS_MIN: usize = 2;
/// Jumlah pilihan paling banyak.
pub const OPTIONS_MAX: usize = 4;

/// Satu pilihan jawaban.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuestionOption {
    pub label: String,
    pub description: String,
    pub recommended: bool,
}

/// Pertanyaan dari agent sebelum disimpan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuestionDraft {
    pub question: String,
    pub options: Vec<QuestionOption>,
}

/// Alasan pertanyaan ditolak; pesannya dikirim ke agent (R3).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum QuestionRejected {
    #[error("the question is empty")]
    EmptyQuestion,
    #[error("the question is longer than {QUESTION_MAX} characters")]
    QuestionTooLong,
    #[error("give {OPTIONS_MIN} to {OPTIONS_MAX} options (got {0})")]
    OptionCount(usize),
    #[error("option {0} has an empty label")]
    EmptyLabel(usize),
    #[error("option {0} has a label longer than {LABEL_MAX} characters")]
    LabelTooLong(usize),
    #[error("option {0} repeats the label of an earlier option")]
    DuplicateLabel(usize),
    #[error("option {0} has an empty description")]
    EmptyDescription(usize),
    #[error("option {0} has a description longer than {DESCRIPTION_MAX} characters")]
    DescriptionTooLong(usize),
    #[error("mark at most one option as recommended")]
    TooManyRecommended,
}

impl QuestionDraft {
    /// Memeriksa isi pertanyaan (R2). Nomor pilihan di pesan dimulai dari 1.
    ///
    /// # Errors
    /// [`QuestionRejected`] dengan aturan pertama yang dilanggar.
    pub fn validate(&self) -> Result<(), QuestionRejected> {
        let question = self.question.trim();
        if question.is_empty() {
            return Err(QuestionRejected::EmptyQuestion);
        }
        if question.chars().count() > QUESTION_MAX {
            return Err(QuestionRejected::QuestionTooLong);
        }
        let n = self.options.len();
        if !(OPTIONS_MIN..=OPTIONS_MAX).contains(&n) {
            return Err(QuestionRejected::OptionCount(n));
        }
        let mut seen: Vec<String> = Vec::with_capacity(n);
        for (i, o) in self.options.iter().enumerate() {
            let no = i + 1;
            let label = o.label.trim();
            if label.is_empty() {
                return Err(QuestionRejected::EmptyLabel(no));
            }
            if label.chars().count() > LABEL_MAX {
                return Err(QuestionRejected::LabelTooLong(no));
            }
            let key = label.to_lowercase();
            if seen.contains(&key) {
                return Err(QuestionRejected::DuplicateLabel(no));
            }
            seen.push(key);
            let description = o.description.trim();
            if description.is_empty() {
                return Err(QuestionRejected::EmptyDescription(no));
            }
            if description.chars().count() > DESCRIPTION_MAX {
                return Err(QuestionRejected::DescriptionTooLong(no));
            }
        }
        if self.options.iter().filter(|o| o.recommended).count() > 1 {
            return Err(QuestionRejected::TooManyRecommended);
        }
        Ok(())
    }

    /// Salinan dengan spasi di tepi teks dibuang, untuk disimpan.
    #[must_use]
    pub fn trimmed(&self) -> Self {
        Self {
            question: self.question.trim().to_owned(),
            options: self
                .options
                .iter()
                .map(|o| QuestionOption {
                    label: o.label.trim().to_owned(),
                    description: o.description.trim().to_owned(),
                    recommended: o.recommended,
                })
                .collect(),
        }
    }
}

/// Status kartu pertanyaan.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum QuestionStatus {
    Pending,
    /// Dijawab dengan klik pilihan.
    Answered,
    /// Dijawab dengan pesan chat biasa (R10).
    AnsweredInChat,
    /// Dev melanjutkan ke plan tanpa menjawab (R16).
    Skipped,
}

/// Transisi yang tidak diizinkan: kartu sudah tidak Pending.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("the question was already {}", .0.as_str())]
pub struct QuestionClosed(pub QuestionStatus);

impl QuestionStatus {
    pub const ALL: [Self; 4] = [
        Self::Pending,
        Self::Answered,
        Self::AnsweredInChat,
        Self::Skipped,
    ];

    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Answered => "answered",
            Self::AnsweredInChat => "answered_in_chat",
            Self::Skipped => "skipped",
        }
    }

    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|v| v.as_str() == s)
    }

    /// Menutup kartu ke `to`; hanya kartu Pending yang bisa ditutup, dan
    /// tujuannya bukan Pending.
    ///
    /// # Errors
    /// [`QuestionClosed`] bila kartu sudah ditutup atau tujuannya Pending.
    pub fn close(self, to: Self) -> Result<Self, QuestionClosed> {
        if self != Self::Pending || to == Self::Pending {
            return Err(QuestionClosed(self));
        }
        Ok(to)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    fn opt(label: &str, description: &str, recommended: bool) -> QuestionOption {
        QuestionOption {
            label: label.to_owned(),
            description: description.to_owned(),
            recommended,
        }
    }

    fn draft(options: Vec<QuestionOption>) -> QuestionDraft {
        QuestionDraft {
            question: "Keep the old result?".to_owned(),
            options,
        }
    }

    fn two() -> Vec<QuestionOption> {
        vec![
            opt("Keep both", "show both", true),
            opt("Replace", "only new", false),
        ]
    }

    #[test]
    fn pertanyaan_valid_diterima() {
        assert_eq!(draft(two()).validate(), Ok(()));
        let mut four = two();
        four.push(opt("C", "c", false));
        four.push(opt("D", "d", false));
        assert_eq!(draft(four).validate(), Ok(()));
    }

    #[test]
    fn jumlah_pilihan_dibatasi() {
        assert_eq!(
            draft(vec![opt("A", "a", false)]).validate(),
            Err(QuestionRejected::OptionCount(1))
        );
        let five = (0..5).map(|i| opt(&format!("L{i}"), "d", false)).collect();
        assert_eq!(
            draft(five).validate(),
            Err(QuestionRejected::OptionCount(5))
        );
        assert!(
            QuestionRejected::OptionCount(1)
                .to_string()
                .contains("2 to 4 options")
        );
    }

    #[test]
    fn teks_pertanyaan_kosong_atau_terlalu_panjang_ditolak() {
        let mut d = draft(two());
        d.question = "   ".to_owned();
        assert_eq!(d.validate(), Err(QuestionRejected::EmptyQuestion));
        d.question = "x".repeat(QUESTION_MAX + 1);
        assert_eq!(d.validate(), Err(QuestionRejected::QuestionTooLong));
        d.question = "é".repeat(QUESTION_MAX);
        assert_eq!(d.validate(), Ok(()));
    }

    #[test]
    fn pilihan_yang_salah_menyebut_nomornya() {
        let cases = [
            (opt(" ", "d", false), QuestionRejected::EmptyLabel(2)),
            (
                opt(&"l".repeat(LABEL_MAX + 1), "d", false),
                QuestionRejected::LabelTooLong(2),
            ),
            (
                opt("keep BOTH", "d", false),
                QuestionRejected::DuplicateLabel(2),
            ),
            (opt("B", "  ", false), QuestionRejected::EmptyDescription(2)),
            (
                opt("B", &"d".repeat(DESCRIPTION_MAX + 1), false),
                QuestionRejected::DescriptionTooLong(2),
            ),
        ];
        for (second, want) in cases {
            let d = draft(vec![opt("Keep both", "d", false), second]);
            assert_eq!(d.validate(), Err(want.clone()));
            assert!(want.to_string().contains("option 2"), "{want}");
        }
    }

    #[test]
    fn paling_banyak_satu_rekomendasi() {
        let d = draft(vec![opt("A", "a", true), opt("B", "b", true)]);
        assert_eq!(d.validate(), Err(QuestionRejected::TooManyRecommended));
        let d = draft(vec![opt("A", "a", false), opt("B", "b", false)]);
        assert_eq!(d.validate(), Ok(()));
    }

    #[test]
    fn trimmed_membuang_spasi_tepi() {
        let d = QuestionDraft {
            question: "  Q?  ".to_owned(),
            options: vec![opt(" A ", " a ", true)],
        };
        let t = d.trimmed();
        assert_eq!(t.question, "Q?");
        assert_eq!(t.options[0], opt("A", "a", true));
    }

    #[test]
    fn hanya_pending_yang_bisa_ditutup() {
        use QuestionStatus::*;
        for to in [Answered, AnsweredInChat, Skipped] {
            assert_eq!(Pending.close(to), Ok(to));
            for from in [Answered, AnsweredInChat, Skipped] {
                assert_eq!(from.close(to), Err(QuestionClosed(from)));
            }
        }
        assert_eq!(Pending.close(Pending), Err(QuestionClosed(Pending)));
    }

    #[test]
    fn status_bolak_balik_string() {
        for s in QuestionStatus::ALL {
            assert_eq!(QuestionStatus::parse(s.as_str()), Some(s));
        }
        assert_eq!(QuestionStatus::parse("open"), None);
    }
}
