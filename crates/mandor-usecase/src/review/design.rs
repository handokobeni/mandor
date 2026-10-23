//! Sumber desain 2.0: folder atau repo yang dibaca agent tanpa bisa ditulis
//! (KTD-33). Pemeriksaan berkas dilakukan adapter; aturan keputusannya di sini.

use std::path::{Path, PathBuf};

/// Sumber desain yang dipilih untuk sebuah run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesignRoot {
    pub id: String,
    pub name: String,
    /// Akar folder, sudah dikanonisasi saat Start.
    pub path: PathBuf,
}

/// Hasil pemeriksaan berkas oleh adapter.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PathProbe {
    pub exists: bool,
    pub is_dir: bool,
    /// `realpath`; `None` bila path tidak ada.
    pub canonical: Option<PathBuf>,
}

/// Status sumber desain.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceCheck {
    /// Siap dipakai, dengan akar yang sudah dikanonisasi.
    Ready(PathBuf),
    Missing,
    NotDirectory,
    /// Akar menyentuh state mandor atau berkas kredensial.
    Forbidden,
}

impl SourceCheck {
    /// Pesan UI (bahasa Inggris).
    #[must_use]
    pub fn message(&self) -> &'static str {
        match self {
            Self::Ready(_) => "Ready",
            Self::Missing => "Path missing",
            Self::NotDirectory => "Not a folder",
            Self::Forbidden => "This path holds Mandor state or credentials and cannot be shared",
        }
    }
}

/// Memutuskan status sumber dari hasil pemeriksaan: harus ada, berupa folder,
/// dan lolos aturan path hook (KTD-21) sebelum maupun sesudah kanonisasi.
#[must_use]
pub fn check_source(path: &Path, probe: &PathProbe) -> SourceCheck {
    let raw = path.to_string_lossy();
    if mandor_domain::hook::is_credential_path(&raw) {
        return SourceCheck::Forbidden;
    }
    if !probe.exists {
        return SourceCheck::Missing;
    }
    if !probe.is_dir {
        return SourceCheck::NotDirectory;
    }
    let canonical = probe
        .canonical
        .clone()
        .unwrap_or_else(|| path.to_path_buf());
    // Kanonisasi bisa membuka path terlarang lewat symlink; penanda folder
    // `/.mandor` dicek dengan garis miring penutup.
    let resolved = format!("{}/", canonical.to_string_lossy());
    if mandor_domain::hook::is_credential_path(&resolved) {
        return SourceCheck::Forbidden;
    }
    SourceCheck::Ready(canonical)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn probe(exists: bool, is_dir: bool, canonical: &str) -> PathProbe {
        PathProbe {
            exists,
            is_dir,
            canonical: Some(PathBuf::from(canonical)),
        }
    }

    #[test]
    fn sumber_harus_ada_berupa_folder_dan_bukan_path_kredensial() {
        let p = Path::new("/work/kelola-mock");
        assert_eq!(
            check_source(p, &probe(true, true, "/work/kelola-mock")),
            SourceCheck::Ready(PathBuf::from("/work/kelola-mock"))
        );
        assert_eq!(check_source(p, &PathProbe::default()), SourceCheck::Missing);
        assert_eq!(SourceCheck::Missing.message(), "Path missing");
        assert_eq!(
            check_source(p, &probe(true, false, "/work/kelola-mock")),
            SourceCheck::NotDirectory
        );
        assert_eq!(
            check_source(Path::new("/home/dev/.mandor/x"), &probe(true, true, "/x")),
            SourceCheck::Forbidden
        );
        // Symlink yang menunjuk ke state mandor.
        assert_eq!(
            check_source(p, &probe(true, true, "/home/dev/.mandor")),
            SourceCheck::Forbidden
        );
    }
}
