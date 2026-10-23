//! Token kapabilitas per giliran untuk tool ledger milik daemon (KTD-26).
//!
//! Token hanya berlaku untuk satu run dan satu peran, hidup selama satu
//! giliran, dan disimpan di memori: restart menghentikan semua sesi (KTD-13),
//! jadi token lama tidak perlu dipulihkan.

use std::sync::{Arc, PoisonError};

use mandor_domain::{AcRow, Design, Verdict};
use secrecy::{ExposeSecret, SecretString};

use crate::ports::{Deps, Ids};
use crate::run::RunService;
use crate::run_ports::LedgerStore;

/// Batas panjang bukti reviewer per AC.
const EVIDENCE_MAX: usize = 2_000;
const TRUNCATED: &str = " [truncated]";

/// Peran token: giliran plan mengisi Design, reviewer mengisi Verdict.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CapabilityRole {
    Design,
    Verdict,
}

impl CapabilityRole {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Design => "design",
            Self::Verdict => "verdict",
        }
    }
}

/// Token kapabilitas. `Debug` tidak pernah mencetak isinya; perbandingan
/// berjalan dalam waktu konstan.
#[derive(Clone)]
pub struct CapabilityToken(SecretString);

impl CapabilityToken {
    #[must_use]
    pub fn expose(&self) -> &str {
        self.0.expose_secret()
    }

    /// Token tetap untuk test adapter; tidak pernah terdaftar di registri.
    #[must_use]
    pub fn for_test(raw: &str) -> Self {
        Self(SecretString::from(raw.to_owned()))
    }
}

impl std::fmt::Debug for CapabilityToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("CapabilityToken(..)")
    }
}

impl PartialEq for CapabilityToken {
    fn eq(&self, other: &Self) -> bool {
        ct_eq(self.expose().as_bytes(), other.expose().as_bytes())
    }
}

impl Eq for CapabilityToken {}

/// Akses ledger untuk satu giliran agent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LedgerAccess {
    pub role: CapabilityRole,
    pub token: CapabilityToken,
}

/// Lingkup koneksi yang memakai token kapabilitas.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapabilityScope {
    pub run_id: String,
    pub role: CapabilityRole,
}

/// Kesalahan operasi ledger dari agent.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LedgerToolError {
    #[error("this session cannot change that column")]
    WrongRole,
    #[error("unknown acceptance criterion: {0}")]
    UnknownAc(String),
    #[error("value not allowed here: {0}")]
    NotAllowed(&'static str),
    #[error("storage error: {0}")]
    Storage(String),
}

impl From<crate::PortError> for LedgerToolError {
    fn from(e: crate::PortError) -> Self {
        Self::Storage(e.to_string())
    }
}

/// Perbandingan byte dalam waktu yang tidak bergantung pada isi.
fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

fn truncate_evidence(text: &str) -> String {
    let text = text.trim();
    if text.chars().count() <= EVIDENCE_MAX {
        return text.to_owned();
    }
    let mut out: String = text.chars().take(EVIDENCE_MAX).collect();
    out.push_str(TRUNCATED);
    out
}

/// Token yang berlaku: (token, lingkup).
pub(crate) type Issued = Vec<(CapabilityToken, CapabilityScope)>;

impl<D: Deps> RunService<D> {
    /// Membuat token untuk run dan peran.
    pub fn issue_capability(&self, run_id: &str, role: CapabilityRole) -> CapabilityToken {
        let raw = format!("{}{}", self.sync.ids.nonce(), self.sync.ids.nonce()).replace('-', "");
        let token = CapabilityToken(SecretString::from(raw));
        self.capabilities
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push((
                token.clone(),
                CapabilityScope {
                    run_id: run_id.to_owned(),
                    role,
                },
            ));
        token
    }

    /// Lingkup token bila masih berlaku. Semua token dibandingkan tanpa berhenti
    /// lebih awal.
    #[must_use]
    pub fn verify_capability(&self, token: &str) -> Option<CapabilityScope> {
        if token.is_empty() {
            return None;
        }
        let issued = self
            .capabilities
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let mut found = None;
        for (t, scope) in issued.iter() {
            if ct_eq(t.expose().as_bytes(), token.as_bytes()) {
                found = Some(scope.clone());
            }
        }
        found
    }

    /// Mencabut semua token run (giliran selesai atau run ditutup).
    pub fn revoke_capabilities(&self, run_id: &str) {
        self.capabilities
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .retain(|(_, s)| s.run_id != run_id);
    }

    /// Buku AC untuk tool `ac_ledger_read`.
    ///
    /// # Errors
    /// Penyimpanan gagal.
    pub async fn ledger_read(
        &self,
        scope: &CapabilityScope,
    ) -> Result<Vec<AcRow>, LedgerToolError> {
        Ok(self.reviews.ac_rows(&scope.run_id).await?)
    }

    /// Tool `ac_set_design` (peran Design). Follow design hanya dari dev (R8a).
    ///
    /// # Errors
    /// Peran salah, AC tidak dikenal, nilai tidak diizinkan, atau penyimpanan gagal.
    pub async fn ledger_set_design(
        &self,
        scope: &CapabilityScope,
        ac_id: &str,
        design: Design,
        note: Option<&str>,
    ) -> Result<Vec<AcRow>, LedgerToolError> {
        if scope.role != CapabilityRole::Design {
            return Err(LedgerToolError::WrongRole);
        }
        if design == Design::FollowDesign {
            return Err(LedgerToolError::NotAllowed("follow_design"));
        }
        self.update_row(&scope.run_id, ac_id, |r| {
            r.design = design;
            r.design_note = note.map(truncate_evidence).filter(|n| !n.is_empty());
        })
        .await
    }

    /// Tool `ac_set_verdict` (peran Verdict): lulus, gagal, atau tidak bisa
    /// dipastikan, dengan bukti teks biasa.
    ///
    /// # Errors
    /// Peran salah, AC tidak dikenal atau di luar gerbang, nilai tidak
    /// diizinkan, atau penyimpanan gagal.
    pub async fn ledger_set_verdict(
        &self,
        scope: &CapabilityScope,
        ac_id: &str,
        verdict: Verdict,
        evidence: Option<&str>,
    ) -> Result<Vec<AcRow>, LedgerToolError> {
        if scope.role != CapabilityRole::Verdict {
            return Err(LedgerToolError::WrongRole);
        }
        if !matches!(verdict, Verdict::Pass | Verdict::Fail | Verdict::CantVerify) {
            return Err(LedgerToolError::NotAllowed(verdict.as_str()));
        }
        let rows = self.reviews.ac_rows(&scope.run_id).await?;
        if rows.iter().any(|r| r.ac_id == ac_id && !r.in_gate()) {
            return Err(LedgerToolError::NotAllowed("ac outside the gate"));
        }
        self.update_row(&scope.run_id, ac_id, |r| {
            r.verdict = verdict;
            r.evidence = evidence.map(truncate_evidence).filter(|e| !e.is_empty());
            r.stale = false;
            // Keputusan dev lama tetap di baris; review run grup memulihkannya
            // bila verdict tidak berubah (R18). Run tunggal sudah mengosongkannya
            // saat review dimulai.
        })
        .await
    }

    async fn update_row(
        &self,
        run_id: &str,
        ac_id: &str,
        change: impl FnOnce(&mut AcRow),
    ) -> Result<Vec<AcRow>, LedgerToolError> {
        let mut rows = self.reviews.ac_rows(run_id).await?;
        let row = rows
            .iter_mut()
            .find(|r| r.ac_id == ac_id)
            .ok_or_else(|| LedgerToolError::UnknownAc(ac_id.to_owned()))?;
        change(row);
        self.reviews.save_ac_rows(run_id, &rows).await?;
        Ok(rows)
    }
}

/// Registri kosong untuk [`RunService::new`].
pub(crate) fn empty() -> Arc<std::sync::Mutex<Issued>> {
    Arc::new(std::sync::Mutex::new(Vec::new()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn perbandingan_konstan_dan_debug_tidak_membocorkan() {
        assert!(ct_eq(b"abc", b"abc"));
        assert!(!ct_eq(b"abc", b"abd"));
        assert!(!ct_eq(b"abc", b"ab"));
        let t = CapabilityToken(SecretString::from("rahasia".to_owned()));
        assert_eq!(format!("{t:?}"), "CapabilityToken(..)");
        assert_eq!(t, t.clone());
        assert_eq!(CapabilityRole::Design.as_str(), "design");
        assert_eq!(CapabilityRole::Verdict.as_str(), "verdict");
    }

    #[test]
    fn bukti_dipotong_dengan_penanda() {
        assert_eq!(truncate_evidence("  ok  "), "ok");
        let long = truncate_evidence(&"y".repeat(3_000));
        assert!(long.ends_with(TRUNCATED));
        assert_eq!(
            long.chars().count(),
            EVIDENCE_MAX + TRUNCATED.chars().count()
        );
    }
}
