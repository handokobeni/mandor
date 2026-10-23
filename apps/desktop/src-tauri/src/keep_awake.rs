//! Mode penjaga keep-awake (KTD-45 plan service OS): `mandor --keep-awake`
//! menahan sleep karena idle (bukan layar, bukan Sleep pilihan pengguna)
//! sampai stdin ditutup. Daemon `mandord` di WSL menjalankan salinan exe ini
//! lewat interop selama ada run yang bekerja; bila daemon mati, stdin tertutup
//! dan pegangan ikut lepas.
//!
//! Juga pegangan sementara milik app saat daemon yang tersambung belum
//! mendukung keep-awake.

use std::io::Read;
use std::process::ExitCode;
use std::sync::mpsc;

/// Argumen yang menyalakan mode penjaga.
pub const FLAG: &str = "--keep-awake";

/// Mode penjaga diminta lewat argumen baris perintah.
#[must_use]
pub fn requested(mut args: impl Iterator<Item = String>) -> bool {
    args.nth(1).is_some_and(|a| a == FLAG)
}

/// Menjalankan mode penjaga: memegang keep-awake di thread ini sampai stdin
/// EOF. Tidak membuka jendela atau webview.
#[must_use]
pub fn run() -> ExitCode {
    let hold = match hold() {
        Ok(h) => h,
        Err(e) => {
            tracing::error!(error = %e, "keep-awake tidak bisa dipegang");
            return ExitCode::FAILURE;
        }
    };
    let result = hold_until_eof(hold, std::io::stdin().lock());
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            tracing::error!(error = %e, "stdin penjaga gagal dibaca");
            ExitCode::FAILURE
        }
    }
}

/// Memegang `_held` sampai `input` EOF; pegangan dilepas saat fungsi selesai.
fn hold_until_eof(_held: Hold, input: impl Read) -> std::io::Result<()> {
    wait_for_eof(input)
}

/// Membaca sampai EOF; isi diabaikan.
fn wait_for_eof(mut input: impl Read) -> std::io::Result<()> {
    let mut buf = [0_u8; 256];
    loop {
        match input.read(&mut buf) {
            Ok(0) => return Ok(()),
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e),
        }
    }
}

#[cfg(windows)]
type Hold = keepawake::KeepAwake;
/// Pengganti pegangan di luar Windows (mode penjaga tidak tersedia).
#[cfg(not(windows))]
#[derive(Debug)]
struct Hold;

#[cfg(windows)]
fn hold() -> Result<Hold, String> {
    // Hanya sleep karena idle (`ES_SYSTEM_REQUIRED`, A4 plan service OS); tanpa
    // `sleep(true)` (away mode), sehingga Sleep yang dipilih pengguna tetap tidur.
    keepawake::Builder::default()
        .idle(true)
        .reason("Mandor agents are working")
        .app_name("Mandor")
        .app_reverse_domain("dev.mandor.app")
        .create()
        .map_err(|e| e.to_string())
}

#[cfg(not(windows))]
#[allow(
    clippy::unnecessary_wraps,
    reason = "tanda tangan sama dengan versi Windows"
)]
fn hold() -> Result<Hold, String> {
    Err("the keep-awake helper only runs on Windows".to_owned())
}

/// Pegangan keep-awake milik app. `SetThreadExecutionState` terikat ke thread
/// pemanggil, jadi pegangan dibuat dan dilepas di satu thread khusus yang
/// menerima perintah lewat channel; thread berhenti saat app berhenti.
#[derive(Debug)]
pub struct AppHold {
    tx: mpsc::Sender<bool>,
}

impl AppHold {
    #[must_use]
    pub fn start() -> Self {
        let (tx, rx) = mpsc::channel::<bool>();
        std::thread::spawn(move || {
            let mut held: Option<Hold> = None;
            // Berhenti saat semua pengirim di-drop (app keluar).
            while let Ok(want) = rx.recv() {
                match (want, held.is_some()) {
                    (true, false) => match hold() {
                        Ok(h) => held = Some(h),
                        Err(e) => tracing::warn!(error = %e, "keep-awake app gagal"),
                    },
                    (false, true) => held = None,
                    _ => {}
                }
            }
        });
        Self { tx }
    }

    /// Meminta keep-awake dipegang (`true`) atau dilepas (`false`).
    pub fn set(&self, hold: bool) {
        if self.tx.send(hold).is_err() {
            tracing::warn!("thread keep-awake app sudah berhenti");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flag_hanya_sebagai_argumen_pertama() {
        let args = |v: &[&str]| v.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
        assert!(requested(args(&["mandor.exe", "--keep-awake"]).into_iter()));
        assert!(!requested(args(&["mandor.exe"]).into_iter()));
        assert!(!requested(
            args(&["mandor.exe", "x", "--keep-awake"]).into_iter()
        ));
    }

    #[test]
    fn menunggu_sampai_eof() {
        let data = b"abc\nlagi\n".repeat(200);
        assert!(wait_for_eof(&data[..]).is_ok());
        assert!(wait_for_eof(&b""[..]).is_ok());
    }

    #[cfg(not(windows))]
    #[test]
    fn di_luar_windows_mode_penjaga_tidak_tersedia() {
        assert!(hold().is_err());
    }
}
