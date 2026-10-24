//! Build script app desktop: Tauri, ditambah SHA-256 biner daemon yang dibawa
//! installer Windows dan paket Linux (KTD-48 plan service OS, KTD-L5 plan paket
//! Linux) sebagai `MANDOR_DAEMON_SHA256`.
//!
//! Biner diambil dari `resources/mandord` (diunduh workflow `release` dari job
//! daemon). Berkas placeholder (dipakai CI dan dev lokal) menghasilkan hash
//! kosong sehingga app tidak pernah memasangnya. Build release Windows atau
//! Linux tanpa biner asli gagal.

use std::path::Path;

use sha2::{Digest, Sha256};

/// Isi placeholder yang dipakai saat biner daemon tidak tersedia.
const PLACEHOLDER: &[u8] = b"placeholder\n";
/// Header resource daemon di paket Linux (sama dengan `provision::RESOURCE_MAGIC`);
/// hash dihitung dari biner tanpa header.
const RESOURCE_MAGIC: &[u8] = b"MANDORD-RESOURCE\n";

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes.iter().fold(String::with_capacity(64), |mut out, b| {
        let _ = write!(out, "{b:02x}");
        out
    })
}

fn main() -> Result<(), String> {
    let resource = Path::new("resources").join("mandord");
    println!("cargo:rerun-if-changed={}", resource.display());
    let raw = std::fs::read(&resource).unwrap_or_default();
    let bytes = raw.strip_prefix(RESOURCE_MAGIC).unwrap_or(&raw);
    let real = !bytes.is_empty() && bytes != PLACEHOLDER;
    let bundles_daemon =
        std::env::var("CARGO_CFG_TARGET_OS").is_ok_and(|os| os == "windows" || os == "linux");
    let release = std::env::var("PROFILE").is_ok_and(|p| p == "release");
    if bundles_daemon && release && !real {
        return Err(format!(
            "{} must contain the Linux mandord binary for a Windows or Linux release build \
             (the release workflow downloads it from the daemon job)",
            resource.display()
        ));
    }
    let hash = if real {
        hex(&Sha256::digest(bytes))
    } else {
        String::new()
    };
    println!("cargo:rustc-env=MANDOR_DAEMON_SHA256={hash}");
    tauri_build::build();
    Ok(())
}
