//! Screenshot perbandingan tampilan run VM (plan 2026-11-03-002, KTD-P8):
//! salinan daemon dari folder output Playwright reviewer, hanya berkas biasa
//! di folder itu, dibatasi ukuran dan jenisnya.

use std::io::Read;
use std::path::{Path, PathBuf};

/// Batas ukuran satu screenshot.
pub(crate) const MAX_BYTES: u64 = 5 * 1024 * 1024;
/// Batas panjang nama berkas dari reviewer.
const NAME_MAX: usize = 128;
const PNG: &[u8] = b"\x89PNG\r\n\x1a\n";
const JPEG: &[u8] = &[0xFF, 0xD8, 0xFF];

/// Nama berkas polos di dalam folder output: tanpa pemisah path, `..`, atau
/// karakter kontrol, berakhiran gambar.
pub(crate) fn valid_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    !name.is_empty()
        && name.len() <= NAME_MAX
        && !name.starts_with('.')
        && !name
            .chars()
            .any(|c| c == '/' || c == '\\' || c.is_control())
        && [".png", ".jpg", ".jpeg"]
            .iter()
            .any(|ext| lower.ends_with(ext))
}

/// Membaca `dir/name` sekali: harus berkas biasa (bukan symlink) yang sama
/// dengan yang dibuka, tidak lebih dari [`MAX_BYTES`], dan berupa PNG/JPEG.
pub(crate) fn read_checked(dir: &Path, name: &str) -> Result<Vec<u8>, String> {
    if !valid_name(name) {
        return Err(format!("{name}: not a plain screenshot file name"));
    }
    let path = dir.join(name);
    let before = std::fs::symlink_metadata(&path).map_err(|e| format!("{name}: {e}"))?;
    if !before.file_type().is_file() {
        return Err(format!("{name}: not a regular file"));
    }
    let file = std::fs::File::open(&path).map_err(|e| format!("{name}: {e}"))?;
    let opened = file.metadata().map_err(|e| format!("{name}: {e}"))?;
    if !same_file(&before, &opened) || !opened.is_file() {
        return Err(format!("{name}: the file changed while it was being read"));
    }
    if opened.len() > MAX_BYTES {
        return Err(format!(
            "{name}: larger than {} MB",
            MAX_BYTES / 1024 / 1024
        ));
    }
    let mut bytes = Vec::new();
    file.take(MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| format!("{name}: {e}"))?;
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > MAX_BYTES {
        return Err(format!(
            "{name}: larger than {} MB",
            MAX_BYTES / 1024 / 1024
        ));
    }
    if !(bytes.starts_with(PNG) || bytes.starts_with(JPEG)) {
        return Err(format!("{name}: not a PNG or JPEG image"));
    }
    Ok(bytes)
}

#[cfg(unix)]
fn same_file(a: &std::fs::Metadata, b: &std::fs::Metadata) -> bool {
    use std::os::unix::fs::MetadataExt;
    a.dev() == b.dev() && a.ino() == b.ino()
}

#[cfg(not(unix))]
fn same_file(a: &std::fs::Metadata, b: &std::fs::Metadata) -> bool {
    a.len() == b.len() && a.modified().ok() == b.modified().ok()
}

/// Menyalin screenshot `names` dari `from` ke `to` dengan nama
/// `<prefix>-<i>.<ext>`; mengembalikan nama salinan, urut seperti `names`.
pub(crate) fn keep(
    from: &Path,
    to: &Path,
    prefix: &str,
    names: &[String],
) -> Result<Vec<String>, String> {
    std::fs::create_dir_all(to).map_err(|e| e.to_string())?;
    let mut kept = Vec::with_capacity(names.len());
    for (i, name) in names.iter().enumerate() {
        let bytes = read_checked(from, name)?;
        let ext = if bytes.starts_with(PNG) { "png" } else { "jpg" };
        let stored = format!("{prefix}-{i}.{ext}");
        std::fs::write(to.join(&stored), &bytes).map_err(|e| e.to_string())?;
        kept.push(stored);
    }
    Ok(kept)
}

/// Folder salinan screenshot satu run.
pub(crate) fn run_dir(root: &Path, run_key: &str) -> PathBuf {
    root.join("screens").join(run_key)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]
    use super::*;

    fn png() -> Vec<u8> {
        let mut b = PNG.to_vec();
        b.extend_from_slice(b"data");
        b
    }

    #[test]
    fn nama_berkas_harus_polos_dan_berupa_gambar() {
        for ok in ["a.png", "AC-1 app.JPG", "x.jpeg"] {
            assert!(valid_name(ok), "{ok}");
        }
        for bad in [
            "../x.png",
            "/etc/x.png",
            "a/b.png",
            ".x.png",
            "x.txt",
            "",
            "a\nb.png",
            "a\\b.png",
        ] {
            assert!(!valid_name(bad), "{bad:?}");
        }
    }

    #[test]
    fn salinan_dibuat_dari_berkas_biasa_dan_yang_lain_ditolak() {
        let dir = tempfile::tempdir().unwrap();
        let from = dir.path().join("out");
        let to = dir.path().join("keep");
        std::fs::create_dir_all(&from).unwrap();
        std::fs::write(from.join("app.png"), png()).unwrap();
        std::fs::write(from.join("teks.png"), b"bukan gambar").unwrap();
        let kept = keep(&from, &to, "ac1", &["app.png".to_owned()]).unwrap();
        assert_eq!(kept, ["ac1-0.png"]);
        assert_eq!(std::fs::read(to.join("ac1-0.png")).unwrap(), png());

        assert!(keep(&from, &to, "ac1", &["teks.png".to_owned()]).is_err());
        assert!(keep(&from, &to, "ac1", &["../out/app.png".to_owned()]).is_err());
        assert!(keep(&from, &to, "ac1", &["hilang.png".to_owned()]).is_err());
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(from.join("app.png"), from.join("link.png")).unwrap();
            assert!(keep(&from, &to, "ac1", &["link.png".to_owned()]).is_err());
        }
        let big = vec![0u8; usize::try_from(MAX_BYTES).unwrap() + 1];
        let mut big_png = PNG.to_vec();
        big_png.extend_from_slice(&big);
        std::fs::write(from.join("besar.png"), big_png).unwrap();
        let err = keep(&from, &to, "ac1", &["besar.png".to_owned()]).unwrap_err();
        assert!(err.contains("larger than"), "{err}");
    }
}
