//! Keputusan `mandord install` dan isi unit systemd user (KTD-43, KTD-53 plan
//! service OS). Logika murni; I/O ada di `mandord`.

/// Biner terpasang dibandingkan biner yang menjalankan `install`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryState {
    /// Belum ada biner di jalur pemasangan.
    Missing,
    /// Isinya identik.
    Same,
    /// Versi terpasang lebih lama.
    Older,
    /// Versi terpasang lebih baru (app lebih lama dari daemon).
    Newer,
    /// Versi sama atau tidak bisa dibaca, tapi isinya berbeda.
    Differs,
}

/// Unit systemd user Mandor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServiceState {
    /// systemd user tidak tersedia (fallback setsid, KTD-50).
    Unavailable,
    Missing,
    Matches,
    Differs,
}

/// Daemon yang sedang berjalan untuk direktori data ini.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Running {
    No,
    /// Dikelola unit systemd.
    Systemd,
    /// Dijalankan terlepas (setsid) atau daemon lama tanpa `managed_by`.
    Detached,
}

/// Cara menghentikan daemon lama setelah berkas baru siap.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stop {
    None,
    /// `systemctl --user restart`.
    RestartService,
    /// SIGTERM ke pid terverifikasi, lalu tunggu lock lepas.
    TerminateDetached,
}

/// Hasil `install`/`uninstall` dalam keluaran JSON.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstallResult {
    Installed,
    Updated,
    ServiceOnly,
    UpToDate,
    Deferred,
    Uninstalled,
    Error,
}

impl InstallResult {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Installed => "installed",
            Self::Updated => "updated",
            Self::ServiceOnly => "service_only",
            Self::UpToDate => "up_to_date",
            Self::Deferred => "deferred",
            Self::Uninstalled => "uninstalled",
            Self::Error => "error",
        }
    }
}

/// Langkah pemasangan yang harus dijalankan.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InstallPlan {
    pub copy_binary: bool,
    pub write_unit: bool,
    /// Daemon yang berjalan harus di-drain dulu; `busy` berarti `deferred`.
    pub needs_drain: bool,
    pub stop: Stop,
    /// Menyalakan service setelah daemon lama berhenti.
    pub start: Start,
    pub result: InstallResult,
    /// Catatan untuk pengguna.
    pub notice: Option<Notice>,
}

/// Menyalakan service setelah berkas siap.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Start {
    Nothing,
    /// `systemctl --user enable --now`.
    EnableNow,
}

/// Catatan hasil pemasangan.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Notice {
    /// Daemon terpasang lebih baru dari biner ini; tidak diturunkan.
    NewerInstalled,
}

impl InstallPlan {
    fn up_to_date(notice: Option<Notice>) -> Self {
        Self {
            copy_binary: false,
            write_unit: false,
            needs_drain: false,
            stop: Stop::None,
            start: Start::Nothing,
            result: InstallResult::UpToDate,
            notice,
        }
    }
}

/// Keputusan pemasangan sesuai tabel di plan service OS.
#[must_use]
pub fn plan_install(binary: BinaryState, service: ServiceState, running: Running) -> InstallPlan {
    if binary == BinaryState::Newer {
        return InstallPlan::up_to_date(Some(Notice::NewerInstalled));
    }
    let copy_binary = matches!(
        binary,
        BinaryState::Missing | BinaryState::Older | BinaryState::Differs
    );
    let systemd = service != ServiceState::Unavailable;
    let write_unit = matches!(service, ServiceState::Missing | ServiceState::Differs);
    if !copy_binary && !write_unit {
        return InstallPlan::up_to_date(None);
    }
    let result = match binary {
        BinaryState::Missing => InstallResult::Installed,
        _ if copy_binary => InstallResult::Updated,
        _ => InstallResult::ServiceOnly,
    };
    let (stop, start) = match (systemd, running) {
        (true, Running::Systemd) => (Stop::RestartService, Start::Nothing),
        (true, Running::Detached) => (Stop::TerminateDetached, Start::EnableNow),
        (true, Running::No) => (Stop::None, Start::EnableNow),
        // Tanpa systemd: daemon lama dihentikan agar `connect` berikutnya
        // menyalakan biner baru lewat setsid.
        (false, Running::No) => (Stop::None, Start::Nothing),
        (false, _) => (Stop::TerminateDetached, Start::Nothing),
    };
    InstallPlan {
        copy_binary,
        write_unit,
        needs_drain: running != Running::No,
        stop,
        start,
        result,
        notice: None,
    }
}

/// Isi unit tidak bisa dibangun dengan aman.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum UnitError {
    #[error("the path contains a control character")]
    ControlCharacter,
}

/// Teks unit `mandord.service`: `ExecStart` ke jalur absolut biner,
/// `Restart=always`, dan perintah keep-awake bila ada. Tidak memuat token.
///
/// # Errors
/// [`UnitError`] bila jalur atau perintah memuat karakter kontrol.
pub fn unit_text(exe: &str, keep_awake_cmd: Option<&str>) -> Result<String, UnitError> {
    let exec = exec_quote(exe)?;
    let env = match keep_awake_cmd {
        Some(cmd) => format!(
            "Environment={}\n",
            quote(&format!("MANDOR_KEEP_AWAKE_CMD={cmd}"))?
        ),
        None => String::new(),
    };
    Ok(format!(
        "[Unit]\nDescription=Mandor daemon\n\n[Service]\nType=simple\n\
         ExecStart={exec} serve\nRestart=always\nRestartSec=2\n{env}\n\
         [Install]\nWantedBy=default.target\n"
    ))
}

/// Kutipan systemd: `\` dan `"` di-escape, `%` (specifier) digandakan.
fn quote(value: &str) -> Result<String, UnitError> {
    if value.chars().any(char::is_control) {
        return Err(UnitError::ControlCharacter);
    }
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for c in value.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '%' => out.push_str("%%"),
            _ => out.push(c),
        }
    }
    out.push('"');
    Ok(out)
}

/// Seperti [`quote`], ditambah `$` digandakan karena `ExecStart` mengekspansi
/// variabel environment.
fn exec_quote(value: &str) -> Result<String, UnitError> {
    Ok(quote(value)?.replace('$', "$$"))
}

/// Versi dari keluaran `mandord --version` (`mandord <versi>`). Keluaran lain,
/// termasuk baris log daemon lama, tidak dikenali.
#[must_use]
pub fn parse_version_line(output: &str) -> Option<&str> {
    let line = output.trim_end_matches(['\n', '\r']);
    let version = line.strip_prefix("mandord ")?;
    let valid = !version.is_empty()
        && version
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '+'));
    valid.then_some(version)
}

/// Membandingkan dua versi `major.minor.patch` (bagian pre-release diabaikan).
#[must_use]
pub fn compare_versions(a: &str, b: &str) -> Option<std::cmp::Ordering> {
    Some(numeric(a)?.cmp(&numeric(b)?))
}

fn numeric(v: &str) -> Option<[u64; 3]> {
    let core = v.split(['-', '+']).next()?;
    let mut parts = core.split('.').map(str::parse::<u64>);
    let out = [
        parts.next()?.ok()?,
        parts.next()?.ok()?,
        parts.next()?.ok()?,
    ];
    parts.next().is_none().then_some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use BinaryState as B;
    use Running as R;
    use ServiceState as S;

    fn plan(b: BinaryState, s: ServiceState, r: Running) -> InstallPlan {
        plan_install(b, s, r)
    }

    #[test]
    fn biner_belum_ada_tanpa_daemon_dipasang_dan_service_dinyalakan() {
        let p = plan(B::Missing, S::Missing, R::No);
        assert!(p.copy_binary && p.write_unit && p.start == Start::EnableNow && !p.needs_drain);
        assert_eq!(p.stop, Stop::None);
        assert_eq!(p.result, InstallResult::Installed);
    }

    #[test]
    fn biner_belum_ada_dengan_daemon_detached_dihentikan_lalu_service() {
        let p = plan(B::Missing, S::Missing, R::Detached);
        assert!(p.copy_binary && p.write_unit && p.needs_drain && p.start == Start::EnableNow);
        assert_eq!(p.stop, Stop::TerminateDetached);
    }

    #[test]
    fn versi_sama_dan_service_sesuai_tidak_mengubah_apa_pun() {
        for s in [S::Matches, S::Unavailable] {
            for r in [R::No, R::Systemd, R::Detached] {
                let p = plan(B::Same, s, r);
                assert_eq!(p.result, InstallResult::UpToDate, "{s:?} {r:?}");
                assert!(!p.copy_binary && !p.write_unit && !p.needs_drain);
            }
        }
    }

    #[test]
    fn versi_sama_tapi_unit_hilang_atau_berbeda_menulis_service_saja() {
        let p = plan(B::Same, S::Missing, R::No);
        assert_eq!(p.result, InstallResult::ServiceOnly);
        assert!(!p.copy_binary && p.write_unit && p.start == Start::EnableNow && !p.needs_drain);

        let p = plan(B::Same, S::Differs, R::Systemd);
        assert_eq!(p.stop, Stop::RestartService);
        assert!(p.needs_drain && p.start == Start::Nothing);

        let p = plan(B::Same, S::Differs, R::Detached);
        assert_eq!(p.stop, Stop::TerminateDetached);
        assert!(p.needs_drain && p.start == Start::EnableNow);
    }

    #[test]
    fn versi_lebih_lama_atau_isi_berbeda_diperbarui() {
        for b in [B::Older, B::Differs] {
            let p = plan(b, S::Matches, R::No);
            assert_eq!(p.result, InstallResult::Updated);
            assert!(p.copy_binary && !p.write_unit && p.start == Start::EnableNow);

            let p = plan(b, S::Matches, R::Systemd);
            assert_eq!(p.stop, Stop::RestartService);
            assert!(p.needs_drain);

            let p = plan(b, S::Matches, R::Detached);
            assert_eq!(p.stop, Stop::TerminateDetached);
            assert!(p.needs_drain && p.start == Start::EnableNow);
        }
    }

    #[test]
    fn versi_terpasang_lebih_baru_tidak_diturunkan() {
        let p = plan(B::Newer, S::Missing, R::Systemd);
        assert_eq!(p.result, InstallResult::UpToDate);
        assert_eq!(p.notice, Some(Notice::NewerInstalled));
        assert!(!p.copy_binary && !p.write_unit);
    }

    #[test]
    fn tanpa_systemd_hanya_salin_dan_hentikan_daemon_lama() {
        let p = plan(B::Missing, S::Unavailable, R::No);
        assert!(p.copy_binary && !p.write_unit && p.start == Start::Nothing);
        assert_eq!(p.result, InstallResult::Installed);

        let p = plan(B::Older, S::Unavailable, R::Detached);
        assert_eq!(p.stop, Stop::TerminateDetached);
        assert!(p.needs_drain && p.start == Start::Nothing);
    }

    #[test]
    fn unit_memuat_jalur_absolut_restart_dan_keep_awake() {
        let text = unit_text(
            "/home/beni/.local/bin/mandord",
            Some(r#"["/mnt/c/x.exe","--keep-awake"]"#),
        )
        .unwrap();
        assert!(text.contains("ExecStart=\"/home/beni/.local/bin/mandord\" serve\n"));
        assert!(text.contains("Restart=always\n"));
        assert!(text.contains(
            r#"Environment="MANDOR_KEEP_AWAKE_CMD=[\"/mnt/c/x.exe\",\"--keep-awake\"]""#
        ));
        assert!(text.contains("WantedBy=default.target"));
        assert!(!text.to_lowercase().contains("token"));
    }

    #[test]
    fn unit_meng_escape_spasi_persen_kutip_dan_dolar() {
        let text = unit_text("/home/a b/100%/\"x\"/$HOME/mandord", None).unwrap();
        assert!(text.contains(r#"ExecStart="/home/a b/100%%/\"x\"/$$HOME/mandord" serve"#));
        assert!(!text.contains("Environment="));
    }

    #[test]
    fn unit_menolak_karakter_kontrol() {
        assert_eq!(
            unit_text("/home/a\n/mandord", None),
            Err(UnitError::ControlCharacter)
        );
        assert_eq!(
            unit_text("/bin/mandord", Some("x\ny")),
            Err(UnitError::ControlCharacter)
        );
    }

    #[test]
    fn versi_dibaca_dari_baris_polos_saja() {
        assert_eq!(parse_version_line("mandord 0.3.1\n"), Some("0.3.1"));
        assert_eq!(parse_version_line("mandord 0.3.1-rc.1"), Some("0.3.1-rc.1"));
        assert_eq!(parse_version_line(""), None);
        assert_eq!(
            parse_version_line("2026-10-23T01:00:00Z  INFO mandord: version=0.1.0"),
            None
        );
        assert_eq!(parse_version_line("mandord \u{1b}[1m0.1\n"), None);
    }

    #[test]
    fn perbandingan_versi() {
        use std::cmp::Ordering::{Equal, Greater, Less};
        assert_eq!(compare_versions("0.1.0", "0.2.0"), Some(Less));
        assert_eq!(compare_versions("1.10.0", "1.9.9"), Some(Greater));
        assert_eq!(compare_versions("0.2.0-rc.1", "0.2.0"), Some(Equal));
        assert_eq!(compare_versions("0.2", "0.2.0"), None);
        assert_eq!(compare_versions("x.y.z", "0.2.0"), None);
    }

    #[test]
    fn nama_hasil() {
        assert_eq!(InstallResult::ServiceOnly.as_str(), "service_only");
        assert_eq!(InstallResult::UpToDate.as_str(), "up_to_date");
        assert_eq!(InstallResult::Uninstalled.as_str(), "uninstalled");
    }
}
