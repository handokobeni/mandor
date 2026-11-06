//! Screenshot perbandingan tampilan run VM (plan 2026-11-03-002, KTD-P8):
//! salinan daemon dari folder output Playwright reviewer, hanya berkas biasa
//! di folder itu, dibatasi ukuran dan jenisnya.

use std::io::{Read, Write};
use std::path::Path;

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
    // Tidak mengikuti symlink dan tidak menunggu FIFO; jenis berkas diperiksa
    // dari handle yang sudah terbuka.
    let file = open_no_follow(&path).map_err(|e| format!("{name}: {e}"))?;
    let opened = file.metadata().map_err(|e| format!("{name}: {e}"))?;
    if !opened.is_file() {
        return Err(format!("{name}: not a regular file"));
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
fn open_no_follow(path: &Path) -> std::io::Result<std::fs::File> {
    use std::os::unix::fs::OpenOptionsExt;
    std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
}

#[cfg(not(unix))]
fn open_no_follow(path: &Path) -> std::io::Result<std::fs::File> {
    if std::fs::symlink_metadata(path)?.file_type().is_symlink() {
        return Err(std::io::Error::other("symbolic link"));
    }
    std::fs::File::open(path)
}

/// Folder tujuan dibuat khusus daemon (0700) dan tidak boleh berupa symlink.
fn private_dir(dir: &Path) -> Result<(), String> {
    let mut builder = std::fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(dir).map_err(|e| e.to_string())?;
    let meta = std::fs::symlink_metadata(dir).map_err(|e| e.to_string())?;
    if meta.file_type().is_symlink() || !meta.is_dir() {
        return Err("the screenshot folder is not a plain directory".to_owned());
    }
    Ok(())
}

/// Berkas baru (0600) yang tidak mengikuti symlink; entri lama dihapus dulu.
fn write_new(path: &Path, bytes: &[u8]) -> Result<(), String> {
    match std::fs::remove_file(path) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e.to_string()),
        _ => {}
    }
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600).custom_flags(libc::O_NOFOLLOW);
    }
    let mut file = opts.open(path).map_err(|e| e.to_string())?;
    file.write_all(bytes).map_err(|e| e.to_string())
}

/// Menyalin screenshot `names` dari `from` ke `to` dengan nama
/// `<prefix>-<i>.<ext>`; mengembalikan nama salinan, urut seperti `names`.
pub(crate) fn keep(
    from: &Path,
    to: &Path,
    prefix: &str,
    names: &[String],
) -> Result<Vec<String>, String> {
    private_dir(to)?;
    let mut kept = Vec::with_capacity(names.len());
    for (i, name) in names.iter().enumerate() {
        let bytes = read_checked(from, name)?;
        let ext = if bytes.starts_with(PNG) { "png" } else { "jpg" };
        let stored = format!("{prefix}-{i}.{ext}");
        write_new(&to.join(&stored), &bytes)?;
        kept.push(stored);
    }
    Ok(kept)
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

    #[cfg(unix)]
    #[test]
    fn fifo_ditolak_tanpa_menunggu_dan_tujuan_symlink_ditolak() {
        let dir = tempfile::tempdir().unwrap();
        let from = dir.path().join("out");
        std::fs::create_dir_all(&from).unwrap();
        let fifo = from.join("pipa.png");
        let ok = std::process::Command::new("mkfifo")
            .arg(&fifo)
            .status()
            .unwrap()
            .success();
        assert!(ok);
        let err = read_checked(&from, "pipa.png").unwrap_err();
        assert!(err.contains("not a regular file"), "{err}");

        std::fs::write(from.join("app.png"), png()).unwrap();
        let target = dir.path().join("lain");
        std::fs::create_dir_all(&target).unwrap();
        let link = dir.path().join("keep");
        std::os::unix::fs::symlink(&target, &link).unwrap();
        assert!(keep(&from, &link, "ac1", &["app.png".to_owned()]).is_err());
        assert!(std::fs::read_dir(&target).unwrap().next().is_none());

        let to = dir.path().join("asli");
        std::fs::create_dir_all(&to).unwrap();
        std::os::unix::fs::symlink(dir.path().join("dicuri"), to.join("ac1-0.png")).unwrap();
        keep(&from, &to, "ac1", &["app.png".to_owned()]).unwrap();
        assert!(
            !dir.path().join("dicuri").exists(),
            "symlink lama diganti, bukan diikuti"
        );
        let mode = std::os::unix::fs::PermissionsExt::mode(
            &std::fs::metadata(to.join("ac1-0.png"))
                .unwrap()
                .permissions(),
        );
        assert_eq!(mode & 0o777, 0o600);
    }
}
