//! Keep-alive WSL lewat `%UserProfile%\.wslconfig` (KTD-46 plan service OS):
//! `[general] instanceIdleTimeout=-1` dan `[wsl2] vmIdleTimeout=-1`, hanya
//! setelah persetujuan pengguna. Kunci, komentar, dan urutan baris lain
//! dipertahankan; cadangan dibuat sekali sebelum perubahan pertama.

use std::path::{Path, PathBuf};

/// Dua kunci milik Mandor: (bagian, kunci).
const KEYS: [(&str, &str); 2] = [
    ("general", "instanceIdleTimeout"),
    ("wsl2", "vmIdleTimeout"),
];
const VALUE: &str = "-1";

/// Status keep-alive untuk UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum KeepAliveStatus {
    /// Kedua kunci bernilai `-1`.
    Enabled,
    /// Belum diaktifkan, atau hanya salah satu kunci.
    Disabled,
    /// Windows 10: `vmIdleTimeout` tidak berlaku.
    Unsupported,
}

/// Nama bagian `[nama]` pada sebuah baris, huruf kecil.
fn section_of(line: &str) -> Option<String> {
    let t = line.trim();
    let inner = t.strip_prefix('[')?.strip_suffix(']')?;
    Some(inner.trim().to_ascii_lowercase())
}

/// Kunci pada baris `kunci = nilai` (bukan komentar), huruf kecil.
fn key_of(line: &str) -> Option<String> {
    let t = line.trim_start();
    if t.starts_with('#') || t.starts_with(';') {
        return None;
    }
    let (k, _) = t.split_once('=')?;
    Some(k.trim().to_ascii_lowercase())
}

fn value_of(line: &str) -> Option<&str> {
    let (_, v) = line.split_once('=')?;
    let v = v.trim();
    Some(v.split(['#', ';']).next().unwrap_or(v).trim())
}

/// Indeks baris kunci `key` di bagian `section`, bila ada.
fn find_key(lines: &[String], section: &str, key: &str) -> Option<usize> {
    let key = key.to_ascii_lowercase();
    let mut current = None;
    for (i, line) in lines.iter().enumerate() {
        if let Some(s) = section_of(line) {
            current = Some(s);
        } else if current.as_deref() == Some(section) && key_of(line).as_deref() == Some(&key) {
            return Some(i);
        }
    }
    None
}

/// Indeks setelah baris terakhir bagian `section` (sebelum bagian berikutnya
/// dan baris kosong penutupnya), bila bagian ada.
fn section_end(lines: &[String], section: &str) -> Option<usize> {
    let start = lines
        .iter()
        .position(|l| section_of(l).as_deref() == Some(section))?;
    let mut end = start + 1;
    let mut last_content = start + 1;
    while end < lines.len() && section_of(&lines[end]).is_none() {
        if !lines[end].trim().is_empty() {
            last_content = end + 1;
        }
        end += 1;
    }
    Some(last_content)
}

fn split_lines(text: &str) -> Vec<String> {
    text.lines().map(str::to_owned).collect()
}

fn join_lines(lines: &[String], crlf: bool) -> String {
    let sep = if crlf { "\r\n" } else { "\n" };
    let mut out = lines.join(sep);
    if !out.is_empty() {
        out.push_str(sep);
    }
    out
}

/// Isi `.wslconfig` dengan kedua kunci bernilai `-1`.
#[must_use]
pub fn enable(text: &str) -> String {
    let crlf = text.contains("\r\n");
    let mut lines = split_lines(text);
    for (section, key) in KEYS {
        let line = format!("{key}={VALUE}");
        if let Some(i) = find_key(&lines, section, key) {
            lines[i] = line;
        } else if let Some(end) = section_end(&lines, section) {
            lines.insert(end, line);
        } else {
            if lines.last().is_some_and(|l| !l.trim().is_empty()) {
                lines.push(String::new());
            }
            lines.push(format!("[{section}]"));
            lines.push(line);
        }
    }
    join_lines(&lines, crlf)
}

/// Isi `.wslconfig` tanpa kedua kunci Mandor; isi lain tidak diubah.
///
/// `original` adalah isi sebelum `enable` pertama (cadangan), bila ada.
/// Bagian yang kosong setelah kunci dihapus hanya ikut dihapus bila tidak
/// ada di `original`, yaitu bila bagian itu dibuat Mandor.
#[must_use]
pub fn disable(text: &str, original: Option<&str>) -> String {
    let crlf = text.contains("\r\n");
    let mut lines = split_lines(text);
    let kept = original.map(split_lines).unwrap_or_default();
    for (section, key) in KEYS {
        if let Some(i) = find_key(&lines, section, key) {
            lines.remove(i);
            if !kept
                .iter()
                .any(|l| section_of(l).as_deref() == Some(section))
            {
                drop_empty_section(&mut lines, section);
            }
        }
    }
    join_lines(&lines, crlf)
}

/// Menghapus header bagian yang tidak lagi berisi apa pun, beserta baris
/// kosong pemisah sebelumnya, agar berkas kembali seperti sebelum `enable`.
fn drop_empty_section(lines: &mut Vec<String>, section: &str) {
    let Some(start) = lines
        .iter()
        .position(|l| section_of(l).as_deref() == Some(section))
    else {
        return;
    };
    let empty = lines[start + 1..]
        .iter()
        .take_while(|l| section_of(l).is_none())
        .all(|l| l.trim().is_empty());
    if !empty {
        return;
    }
    let mut end = start + 1;
    while end < lines.len() && section_of(&lines[end]).is_none() {
        end += 1;
    }
    lines.drain(start..end);
    if start > 0 && start == lines.len() && lines[start - 1].trim().is_empty() {
        lines.remove(start - 1);
    }
}

/// Kedua kunci bernilai `-1`.
#[must_use]
pub fn is_enabled(text: &str) -> bool {
    let lines = split_lines(text);
    KEYS.iter().all(|(section, key)| {
        find_key(&lines, section, key).and_then(|i| value_of(&lines[i])) == Some(VALUE)
    })
}

/// Nomor build dari keluaran `cmd /c ver`
/// (`Microsoft Windows [Version 10.0.26200.1234]`).
#[must_use]
pub fn windows_build(ver_output: &str) -> Option<u32> {
    let start = ver_output.find("Version ")? + "Version ".len();
    let rest = &ver_output[start..];
    let version = rest.split(']').next()?;
    version.split('.').nth(2)?.trim().parse().ok()
}

/// Windows 11 dimulai dari build 22000; `vmIdleTimeout` hanya berlaku di sana.
#[must_use]
pub fn supports_vm_idle_timeout(build: u32) -> bool {
    build >= 22_000
}

/// Kegagalan membaca atau menulis `.wslconfig`.
#[derive(Debug, thiserror::Error)]
pub enum WslConfigError {
    #[error("{0} is a link or not a regular file; edit it manually")]
    NotRegular(PathBuf),
    #[error("cannot update .wslconfig: {0}")]
    Io(String),
}

impl serde::Serialize for WslConfigError {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}

fn io(e: &std::io::Error) -> WslConfigError {
    WslConfigError::Io(e.to_string())
}

/// Isi berkas, atau kosong bila belum ada. Symlink ditolak.
fn read(path: &Path) -> Result<String, WslConfigError> {
    match std::fs::symlink_metadata(path) {
        Ok(m) if m.file_type().is_symlink() || !m.is_file() => {
            Err(WslConfigError::NotRegular(path.to_owned()))
        }
        Ok(_) => std::fs::read_to_string(path).map_err(|e| io(&e)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(e) => Err(io(&e)),
    }
}

/// Menulis atomik: berkas sementara di direktori yang sama, lalu rename.
fn write(path: &Path, text: &str) -> Result<(), WslConfigError> {
    let tmp = path.with_extension("mandor-tmp");
    std::fs::write(&tmp, text).map_err(|e| io(&e))?;
    std::fs::rename(&tmp, path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        io(&e)
    })
}

/// Path cadangan `.wslconfig.mandor-bak`.
fn backup_path(path: &Path) -> PathBuf {
    if path.extension().is_none() {
        PathBuf::from(format!("{}.mandor-bak", path.display()))
    } else {
        path.with_extension("wslconfig.mandor-bak")
    }
}

/// Cadangan `.wslconfig.mandor-bak`, dibuat sekali bila berkas asli ada.
fn backup_once(path: &Path) -> Result<(), WslConfigError> {
    let bak = backup_path(path);
    if bak.exists() || !path.exists() {
        return Ok(());
    }
    std::fs::copy(path, &bak).map(drop).map_err(|e| io(&e))
}

/// Status keep-alive dari berkas di `path`.
///
/// # Errors
/// Berkas tidak bisa dibaca atau berupa symlink.
pub fn status_at(path: &Path) -> Result<bool, WslConfigError> {
    Ok(is_enabled(&read(path)?))
}

/// Mengaktifkan keep-alive di `path`.
///
/// # Errors
/// Berkas tidak bisa dibaca, dicadangkan, atau ditulis.
pub fn enable_at(path: &Path) -> Result<(), WslConfigError> {
    let text = read(path)?;
    let next = enable(&text);
    if next == text {
        return Ok(());
    }
    backup_once(path)?;
    write(path, &next)
}

/// Menonaktifkan keep-alive di `path` (hanya dua kunci Mandor).
///
/// # Errors
/// Berkas tidak bisa dibaca atau ditulis.
pub fn disable_at(path: &Path) -> Result<(), WslConfigError> {
    let text = read(path)?;
    let bak = backup_path(path);
    let original = if bak.exists() {
        Some(read(&bak)?)
    } else {
        None
    };
    let next = disable(&text, original.as_deref());
    if next == text {
        return Ok(());
    }
    write(path, &next)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn berkas_kosong_mendapat_dua_bagian() {
        let out = enable("");
        assert_eq!(
            out,
            "[general]\ninstanceIdleTimeout=-1\n\n[wsl2]\nvmIdleTimeout=-1\n"
        );
        assert!(is_enabled(&out));
    }

    #[test]
    fn kunci_lain_dan_komentar_dipertahankan() {
        let text = "# pengaturan saya\n[wsl2]\nmemory=8GB\nswap=0 # tanpa swap\n\n[experimental]\nautoMemoryReclaim=gradual\n";
        let out = enable(text);
        assert_eq!(
            out,
            "# pengaturan saya\n[wsl2]\nmemory=8GB\nswap=0 # tanpa swap\nvmIdleTimeout=-1\n\n[experimental]\nautoMemoryReclaim=gradual\n\n[general]\ninstanceIdleTimeout=-1\n"
        );
        assert_eq!(
            disable(&out, Some(text)),
            text,
            "kembali persis seperti semula"
        );
    }

    #[test]
    fn nilai_lain_diganti_dan_huruf_besar_kecil_diabaikan() {
        let text = "[WSL2]\nVMIdleTimeout = 60000\n[General]\ninstanceidletimeout=15000\n";
        let out = enable(text);
        assert_eq!(
            out,
            "[WSL2]\nvmIdleTimeout=-1\n[General]\ninstanceIdleTimeout=-1\n"
        );
        assert!(is_enabled(&out));
    }

    #[test]
    fn crlf_dipertahankan() {
        let out = enable("[wsl2]\r\nmemory=4GB\r\n");
        assert!(out.contains("memory=4GB\r\nvmIdleTimeout=-1\r\n"));
        assert!(!out.replace("\r\n", "").contains('\n'));
    }

    #[test]
    fn status_butuh_kedua_kunci() {
        assert!(!is_enabled("[wsl2]\nvmIdleTimeout=-1\n"));
        assert!(!is_enabled("[general]\ninstanceIdleTimeout=-1\n"));
        assert!(!is_enabled(
            "[wsl2]\nvmIdleTimeout=-1\n[general]\ninstanceIdleTimeout=0\n"
        ));
        assert!(is_enabled(
            "[wsl2]\nvmIdleTimeout = -1 # mandor\n[general]\ninstanceIdleTimeout=-1\n"
        ));
        assert!(!is_enabled(
            "[wsl2]\n# vmIdleTimeout=-1\n[general]\ninstanceIdleTimeout=-1\n"
        ));
    }

    #[test]
    fn kunci_di_bagian_lain_tidak_dianggap() {
        let text = "[experimental]\nvmIdleTimeout=5\n";
        let out = enable(text);
        assert!(out.starts_with("[experimental]\nvmIdleTimeout=5\n"));
        assert!(is_enabled(&out));
        assert_eq!(disable(&out, Some(text)), text);
    }

    #[test]
    fn bagian_yang_sudah_ada_sebelum_enable_tidak_dihapus() {
        // Kasus nyata uji U9: `.wslconfig` hanya berisi `[wsl2]` tanpa baris baru.
        let text = "[wsl2]";
        let out = enable(text);
        assert!(is_enabled(&out));
        assert_eq!(disable(&out, Some(text)), "[wsl2]\n");
        // Tanpa cadangan (berkas dibuat Mandor), bagian kosong ikut dihapus.
        assert_eq!(disable(&enable(""), None), "");
    }

    #[test]
    fn versi_windows() {
        assert_eq!(
            windows_build("\r\nMicrosoft Windows [Version 10.0.26200.6584]\r\n"),
            Some(26_200)
        );
        assert_eq!(
            windows_build("Microsoft Windows [Version 10.0.19045.3803]"),
            Some(19_045)
        );
        assert_eq!(windows_build("lain"), None);
        assert!(supports_vm_idle_timeout(22_000));
        assert!(!supports_vm_idle_timeout(19_045));
    }

    #[test]
    fn berkas_cadangan_sekali_dan_tulis_atomik() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(".wslconfig");
        std::fs::write(&path, "[wsl2]\nmemory=8GB\n").unwrap();
        enable_at(&path).unwrap();
        let bak = dir.path().join(".wslconfig.mandor-bak");
        assert_eq!(
            std::fs::read_to_string(&bak).unwrap(),
            "[wsl2]\nmemory=8GB\n"
        );
        assert!(status_at(&path).unwrap());

        disable_at(&path).unwrap();
        enable_at(&path).unwrap();
        assert_eq!(
            std::fs::read_to_string(&bak).unwrap(),
            "[wsl2]\nmemory=8GB\n",
            "cadangan tidak ditimpa"
        );
        disable_at(&path).unwrap();
        assert!(!status_at(&path).unwrap());
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "[wsl2]\nmemory=8GB\n"
        );
    }

    #[test]
    fn disable_memakai_cadangan_untuk_mempertahankan_bagian_asli() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(".wslconfig");
        std::fs::write(&path, "[wsl2]").unwrap();
        enable_at(&path).unwrap();
        disable_at(&path).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "[wsl2]\n");
    }

    #[test]
    fn berkas_belum_ada_dibuat_tanpa_cadangan() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(".wslconfig");
        assert!(!status_at(&path).unwrap());
        enable_at(&path).unwrap();
        assert!(status_at(&path).unwrap());
        assert!(!dir.path().join(".wslconfig.mandor-bak").exists());
    }

    #[cfg(unix)]
    #[test]
    fn symlink_ditolak() {
        let dir = tempfile::tempdir().unwrap();
        let real = dir.path().join("asli");
        std::fs::write(&real, "").unwrap();
        let path = dir.path().join(".wslconfig");
        std::os::unix::fs::symlink(&real, &path).unwrap();
        assert!(matches!(
            enable_at(&path),
            Err(WslConfigError::NotRegular(_))
        ));
        assert_eq!(std::fs::read_to_string(&real).unwrap(), "");
    }
}
