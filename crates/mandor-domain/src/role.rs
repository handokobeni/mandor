//! Peran daemon (KTD-54 plan VM): laptop atau VM lembur.

/// Peran daemon dari konfigurasi.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DaemonRole {
    /// Daemon di mesin developer.
    #[default]
    Laptop,
    /// Daemon lembur di VM: antrean, siklus pemakaian, gerbang AC selalu
    /// memblokir, review headless, dan agent di user terpisah.
    Vm,
}

impl DaemonRole {
    /// Nama stabil untuk konfigurasi dan protokol.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Laptop => "laptop",
            Self::Vm => "vm",
        }
    }

    /// Kebalikan [`DaemonRole::as_str`]; nilai tak dikenal menjadi `None`.
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim() {
            "laptop" => Some(Self::Laptop),
            "vm" => Some(Self::Vm),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nama_peran_bolak_balik() {
        for r in [DaemonRole::Laptop, DaemonRole::Vm] {
            assert_eq!(DaemonRole::parse(r.as_str()), Some(r));
        }
        assert_eq!(DaemonRole::parse("server"), None);
        assert_eq!(DaemonRole::default(), DaemonRole::Laptop);
    }
}
