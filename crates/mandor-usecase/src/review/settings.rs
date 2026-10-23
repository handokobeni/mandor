//! Resolusi setelan gerbang (KTD-37) dan format waktu sinkron AC untuk PR.

use mandor_domain::GateMode;

/// Mode gerbang run: override repo > workspace > bawaan Block, lalu yang
/// paling ketat di antara anggota.
#[must_use]
pub fn resolve_gate_mode(
    repo_overrides: &[Option<GateMode>],
    workspace: Option<GateMode>,
) -> GateMode {
    let fallback = workspace.unwrap_or_default();
    GateMode::strictest(repo_overrides.iter().map(|o| o.unwrap_or(fallback)))
}

/// Waktu UTC `YYYY-MM-DD HH:MM UTC` dari ms epoch (catatan sinkron AC di PR).
#[must_use]
pub fn utc_minutes(ms: i64) -> String {
    let secs = ms.div_euclid(1000);
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let (y, m, d) = civil_from_days(days);
    format!(
        "{y:04}-{m:02}-{d:02} {:02}:{:02} UTC",
        rem / 3600,
        (rem % 3600) / 60
    )
}

/// Tanggal sipil dari jumlah hari sejak 1970-01-01 (algoritma Howard Hinnant).
fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn override_repo_menang_dan_anggota_paling_ketat_berlaku() {
        // Override repo Block menang atas workspace Warn.
        assert_eq!(
            resolve_gate_mode(&[Some(GateMode::Block)], Some(GateMode::Warn)),
            GateMode::Block
        );
        // Workspace Warn tanpa override.
        assert_eq!(
            resolve_gate_mode(&[None, None], Some(GateMode::Warn)),
            GateMode::Warn
        );
        // Satu anggota Block membuat run Block.
        assert_eq!(
            resolve_gate_mode(&[Some(GateMode::Warn), Some(GateMode::Block)], None),
            GateMode::Block
        );
        assert_eq!(resolve_gate_mode(&[None], None), GateMode::Block);
    }

    #[test]
    fn waktu_utc_dibulatkan_ke_menit() {
        assert_eq!(utc_minutes(0), "1970-01-01 00:00 UTC");
        assert_eq!(utc_minutes(1_792_640_190_881), "2026-10-22 03:36 UTC");
    }
}
